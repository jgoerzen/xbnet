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

use crate::ser::*;
use crate::xbpacket::*;
use bytes::Bytes;
use crossbeam_channel;
use hex;
use log::*;
use std::fs;
use std::io;
use std::io::{BufRead, BufReader, Error, ErrorKind};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

pub fn mkerror(msg: &str) -> Error {
    Error::new(ErrorKind::Other, msg)
}

/// Data to be transmitted out XBee.
pub enum XBTX {
    /// Transmit this data
    TXData(XBDestAddr, Bytes),
    /// Shut down the transmitting thread
    Shutdown,
}

/// Main XBeeNet struct
pub struct XB {
    pub ser_reader: XBSerReader,

    /// My 64-bit MAC address
    pub mymac: u64,

    /// Maximum packet size
    pub maxpacketsize: usize,
}

/// Assert that a given response didn't indicate an EOF, and that it
/// matches the given text.  Return an IOError if either of these
/// conditions aren't met.  The response type is as given by
/// ['ser::XBSer::readln'].
pub fn assert_response(resp: String, expected: String) -> io::Result<()> {
    if resp == expected {
        Ok(())
    } else {
        Err(mkerror(&format!(
            "Unexpected response: got {}, expected {}",
            resp, expected
        )))
    }
}

impl XB {
    /** Creates a new XB.  Returns an instance to be used for reading,
    as well as a separate sender to be used in a separate thread to handle
    outgoing frames.  This will spawn a thread to handle the writing to XBee, which is returned.

    If initfile is given, its lines will be sent to the radio, one at a time,
    expecting OK after each one, to initialize it.

    May panic if an error occurs during initialization.
    */
    pub fn new(
        mut ser_reader: XBSerReader,
        mut ser_writer: XBSerWriter,
        initfile: Option<PathBuf>,
        disable_xbee_acks: bool,
        request_xbee_tx_reports: bool,
    ) -> (XB, crossbeam_channel::Sender<XBTX>, thread::JoinHandle<()>) {
        // FIXME: make this maximum of 5 configurable
        let (writertx, writerrx) = crossbeam_channel::bounded(5);

        debug!("Configuring radio");
        thread::sleep(Duration::from_secs(2));
        trace!("Sending +++");
        ser_writer.swrite.write_all(b"+++").unwrap();
        ser_writer.swrite.flush().unwrap();

        loop {
            // There might be other packets flowing in while we wait for the OK.  FIXME: this could still find
            // it prematurely if OK\r occurs in a packet.
            trace!("Waiting for OK");
            let line = ser_reader.readln().unwrap().unwrap();
            if line.ends_with("OK") {
                trace!("Received OK");
                break;
            } else {
                trace!("Will continue waiting for OK");
            }
        }

        if let Some(file) = initfile {
            let f = fs::File::open(file).unwrap();
            let reader = BufReader::new(f);
            for line in reader.lines() {
                let line = line.unwrap();
                if line.len() > 0 {
                    ser_writer.writeln(&line).unwrap();
                    assert_eq!(ser_reader.readln().unwrap().unwrap(), String::from("OK"));
                }
            }
        }

        // Enter API mode
        ser_writer.writeln("ATAP 1").unwrap();
        assert_eq!(ser_reader.readln().unwrap().unwrap(), String::from("OK"));

        // Standard API output mode
        ser_writer.writeln("ATAO 0").unwrap();
        assert_eq!(ser_reader.readln().unwrap().unwrap(), String::from("OK"));

        // Get our own MAC address
        ser_writer.writeln("ATSH").unwrap();
        let serialhigh = ser_reader.readln().unwrap().unwrap();
        let serialhighu64 = u64::from_str_radix(&serialhigh, 16).unwrap();

        ser_writer.writeln("ATSL").unwrap();
        let seriallow = ser_reader.readln().unwrap().unwrap();
        let seriallowu64 = u64::from_str_radix(&seriallow, 16).unwrap();

        let mymac = serialhighu64 << 32 | seriallowu64;

        // Get maximum packet size
        ser_writer.writeln("ATNP").unwrap();
        let maxpacket = ser_reader.readln().unwrap().unwrap();
        let maxpacketsize = usize::from(u16::from_str_radix(&maxpacket, 16).unwrap());

        // Exit command mode
        ser_writer.writeln("ATCN").unwrap();
        assert_eq!(ser_reader.readln().unwrap().unwrap(), String::from("OK"));

        debug!("Radio configuration complete");

        let writerthread = thread::spawn(move || {
            writerthread(
                ser_writer,
                maxpacketsize,
                writerrx,
                disable_xbee_acks,
                request_xbee_tx_reports,
            )
        });

        (
            XB {
                ser_reader,
                mymac,
                maxpacketsize,
            },
            writertx,
            writerthread,
        )
    }
}

fn writerthread(
    mut ser: XBSerWriter,
    maxpacketsize: usize,
    writerrx: crossbeam_channel::Receiver<XBTX>,
    disable_xbee_acks: bool,
    request_xbee_tx_reports: bool,
) {
    let mut packetstream = PacketStream::new();
    for item in writerrx.iter() {
        match item {
            XBTX::Shutdown => return,
            XBTX::TXData(dest, data) => {
                // Here we receive a block of data, which hasn't been
                // packetized.  Packetize it and send out the result.

                match packetstream.packetize_data(
                    maxpacketsize,
                    &dest,
                    &data,
                    disable_xbee_acks,
                    request_xbee_tx_reports,
                ) {
                    Ok(packets) => {
                        for packet in packets.into_iter() {
                            match packet.serialize() {
                                Ok(datatowrite) => {
                                    trace!(
                                        "TX ID {:X} to {:?} data {}",
                                        packet.frame_id,
                                        &dest,
                                        hex::encode(&datatowrite)
                                    );
                                    ser.swrite.write_all(&datatowrite).unwrap();
                                    ser.swrite.flush().unwrap();
                                }
                                Err(e) => {
                                    error!("Serialization error: {:?}", e);
                                }
                            };
                        }
                    }
                    Err(e) => {
                        error!("Packetization error: {}", e);
                    }
                }
            }
        }
    }
}
