/*
    Copyright (C) 2019-2020  John Goerzen <jgoerzen@complete.org

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <http://www.gnu.org/licenses/>.

*/

use crate::ser::XBSer;
use log::*;
use std::fs;
use std::io::{BufRead, BufReader, Error, ErrorKind};
use std::io;
use crossbeam_channel;
use hex;
use std::thread;
use std::time::{Duration, Instant};
use format_escape_default::format_escape_default;
use std::path::PathBuf;
use bytes::Bytes;

pub fn mkerror(msg: &str) -> Error {
    Error::new(ErrorKind::Other, msg)
}

/// Received frames.  The option is populated only if
/// readqual is true, and reflects the SNR and RSSI of the
/// received packet.
#[derive(Clone, Debug, PartialEq)]
pub struct ReceivedFrames(pub Vec<u8>, pub Option<(String, String)>);

#[derive(Clone)]
pub struct XB {
    ser: XBSer,

    /// My 64-bit MAC address
    mymac: u64,

    /// Frames to transmit
    writerrx: crossbeam_channel::Receiver<Bytes>,

    /// Maximum packet size
    maxpacketsize: usize,
}

/// Assert that a given response didn't indicate an EOF, and that it
/// matches the given text.  Return an IOError if either of these
/// conditions aren't met.  The response type is as given by
/// ['ser::XBSer::readln'].
pub fn assert_response(resp: String, expected: String) -> io::Result<()> {
    if resp == expected {
        Ok(())
    } else {
        Err(mkerror(&format!("Unexpected response: got {}, expected {}", resp, expected)))
    }
}

impl XB {
    /** Creates a new XB.  Returns an instance to be used for reading,
    as well as a separate sender to be used in a separate thread to handle
    outgoing frames.  This will spawn a thread to handle the writing to XBee.

    If initfile is given, its lines will be sent to the radio, one at a time,
    expecting OK after each one, to initialize it.

    May panic if an error occurs during initialization.
    */
    pub fn new(ser: XBSer, initfile: Option<PathBuf>) -> (XB, crossbeam_channel::Sender<Bytes>) {
        // FIXME: make this maximum of 5 configurable
        let (writertx, writerrx) = crossbeam_channel::bounded(5);

        debug!("Configuring radio");
        thread::sleep(Duration::from_msecs(1100));
        ser.swrite.lock().unwrap().write_all(b"+++").unwrap();
        ser.swrite.lock().unwrap().flush();

        assert_response(ser.readln().unwrap(), "OK");

        if let Some(file) = initfile {
            let f = fs::File::open(file).unwrap();
            let reader = BufReader::new(f);
            for line in reader.lines() {
                if line.len() > 0 {
                    self.ser.writeln(line).unwrap();
                    assert_response(ser.readln().unwrap(), "OK")
                }
            }
        }

        // Enter API mode
        ser.writeln("ATAP 1").unwrap();
        assert_response(ser.readln().unwrap, "OK");

        // Standard API output mode
        ser.writeln("ATAO 0").unwrap();
        assert_response(ser.readln().unwrap(), "OK");

        // Get our own MAC address
        ser.writeln("ATSH").unwrap();
        let serialhigh = ser.readln().unwrap();
        let serialhighu64 = u64::from(u32::from_be_bytes(hex::decode(serialhigh).unwrap()));

        ser.writeln("ATSL").unwrap();
        let seriallow = ser.readln().unwrap();
        let seriallowu64 = u64::from(u32::from_be_bytes(hex::decode(seriallow).unwrap()));

        let mymac = serialhighu64 << 32 | seriallowu64;

        // Get maximum packet size
        ser.writeln("ATNP").unwrap();
        let maxpacket = ser.readln().unwrap();
        let maxpacketsize = usize::from(u16::from_be_bytes(hex::decode(maxpacket).unwrap()));


        // Exit command mode
        ser.writeln("ATCN").unwrap();
        assert_response(ser.readln().unwrap(), "OK");

        let ser2 = ser.clone();
        
        (XB {
            ser,
            mymac,
            maxpacketsize,
            writerrx,
        }, writertx)
    }

    pub fn mainloop(&mut self) -> io::Result<()> {
        loop {
            // First, check to see if we're allowed to transmit.  If not, just
            // try to read and ignore all else.
            if let Some(delayamt) = self.txdelayrequired() {
                // We can't transmit yet.  Just read, but with a time box.
                self.enterrxmode()?;
                let res = self.readerlinesrx.recv_timeout(delayamt);
                match res {
                    Ok(msg) => {
                        self.handlerx(msg, self.readqual)?;
                        continue;
                    },
                    Err(e) => {
                        if e.is_timeout() {
                            debug!("readerthread: txdelay timeout expired");
                            self.txdelay = None;
                            // Now we can fall through to the rest of the logic - already in read mode.
                        } else {
                            res.unwrap(); // disconnected - crash
                        }
                    }
                }
            } else {
                // We are allowed to transmit.
                
                // Do we have anything to send?  Check at the top and keep checking
                // here so we send as much as possible before going back into read
                // mode.
                if ! self.extradata.is_empty() {
                    // Send the extradata immediately
                    self.dosend(vec![])?;
                    continue;
                }
                let r = self.txblocksrx.try_recv();
                match r {
                    Ok(data) => {
                        self.dosend(data)?;
                        continue;
                    },
                    Err(e) => {
                        if e.is_disconnected() {
                            // other threads crashed
                            r.unwrap();
                        }
                        // Otherwise - nothing to write, go on through.
                    }
                }

                self.enterrxmode()?;
            }

            // At this point, we're in rx mode with no timeout.  No extradata
            // is waiting either.
            // Now we wait for either a write request or data.

            let mut sel = crossbeam_channel::Select::new();
            let readeridx = sel.recv(&self.readerlinesrx);
            let blocksidx = sel.recv(&self.txblocksrx);
            match sel.ready() {
                i if i == readeridx => {
                    // We have data coming in from the radio.
                    let msg = self.readerlinesrx.recv().unwrap();
                    self.handlerx(msg, self.readqual)?;
                },
                i if i == blocksidx => {
                    // We have something to send.  Stop the receiver and then go
                    // back to the top of the loop to handle it.

                    self.rxstop()?;
                    
                },
                _ => panic!("Invalid response from sel.ready()"),
            }
        }
    }

    pub fn transmit(&mut self, data: &[u8])  {
        self.txblockstx.send(data.to_vec()).unwrap();
    }
}


