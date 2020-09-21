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
    pub fn new(ser: XBSer, initfile: Option<PathBuf>) -> (XB, crossbeam_channel::Sender<(XBDestAddr, Bytes)>) {
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
        thread::spawn(move || writerthread(ser2, maxpacketsize, writerrx));
        
        (XB {
            ser,
            mymac,
            maxpacketsize,
        }, writertx)
    }
}

fn writerthread(ser: XBSer, maxpacketsize: usize,
                writerrx: crossbeam_channel::Receiver<(XBDestAddr, Bytes)>) {
    for (dest, data) in writerrx.iter() {
        // Here we receive a block of data, which hasn't been
        // packetized.  Packetize it and send out the result.

        match packetize_data(maxpacketsize, dest, data) {
            Ok(packets) => {
                let serport = ser.swrite.lock().unwrap();
                for packet in packets.into_iter() {
                    match packet.serialize() {
                        Ok(datatowrite) => {
                            trace!("TX to {:?} data {}", dest, hex::encode(datatowrite));
                            serport.write_all(datatowrite).unwrap();
                            serport.flush();
                        },
                        Err(e) => {
                            error!("Serialization error: {}", e),
                        },
                    },
                }
            },
            Err(e) => {
                error!("Packetization error: {}", e);
            }
        }
    }
}
