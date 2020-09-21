/*! Receiving data from XBee */

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
use crate::xbpacket::*;
use log::*;
use std::fs;
use std::io::{BufRead, BufReader, Error, ErrorKind, Read};
use std::io;
use crossbeam_channel;
use hex;
use std::thread;
use std::time::{Duration, Instant};
use format_escape_default::format_escape_default;
use std::path::PathBuf;
use bytes::*;
use std::collections::HashMap;

/** Attempts to read a packet from the port.  Returns
None if it's not an RX frame, or if there is a checksum mismatch. */
pub fn rxxbpacket(ser: &XBSer) -> Option<RXPacket> {
    let mut junkbytes = BytesMut::new();
    let serport = *ser.br.lock().unwrap();
    loop {
        let mut startdelim = [0u8; 1];
        serport.read_exact(&mut startdelim).unwrap();
        if startdelim[0] != 0x7e {
            if junkbytes.is_empty() {
                error!("Receiving junk");
            }

            junkbytes.put_u8(startdelim[0]);
        } else {
            break;
        }
    }

    // OK, got the start delimeter.  Log the junk, if any.
    if ! junkbytes.is_empty() {
        error!("Found start delimeter after reading junk: {}", hex::encode(junkbytes));
        junkbytes.clear();
    }

    // Read the length.

    let mut lenbytes = [0u8; 2];
    serport.read_exact(&mut lenbytes).unwrap();
    let length = usize::from(u16::from_be_bytes(lenbytes));

    // Now read the rest of the frame.
    let mut inner = [0u8; length];

    serport.read_exact(&mut inner).unwrap();

    // And the checksum.
    let mut checksum = [0u8; 1];
    serport.read_exact(&mut checksum).unwrap();

    if xbchecksum(&inner) != checksum[0] {
        error!("SERIN: Checksum mismatch; data: {}", hex::encode(inner));
        return None;
    }

    let inner = Bytes::from(inner);
    let frametype = inner.get_u8();
    if frametype != 0x90 {
        debug!("SERIN: Non-0x90 frame; data: {}", hex::encode(inner));
        return None;
    }

    let sender_addr64 = inner.get_u64();
    let sender_addr16 = inner.get_u16();
    let rx_options = inner.get_u8();
    let payload = inner.to_bytes();
    trace!("SERIN: packet from {} / {}, payload {}", hex::encode(sender_addr64.to_be_bytes()), hex::encode(sender_addr16.to_be_bytes()), hex::encode(payload));
    Some(RXPacket {sender_addr64, sender_addr16, rx_options, payload})
}

/// Like rxxbpacket, but wait until we have a valid packet.
pub fn rxxbpacket_wait(ser: &XBSer) -> RXPacket {
    loop {
        if let Some(packet) = rxxbpacket(ser) {
            return packet;
        }
    }
}

/// Receives XBee packets, recomposes into larger frames.
pub struct XBReframer {
    buf: HashMap<u64, BytesMut>,
}

/** Receive a frame that may have been split up into multiple XBee frames.  Reassemble
as needed and return when we've got something that can be returned. */
impl XBReframer {
    pub fn new() -> Self {
        XBReframer {
            buf: HashMap::new(),
        }
    }

    /// Receive a frame.  Indicate the sender (u64, u16) and payload.
    pub fn rxframe(&mut self, ser: &XBSer) -> (u64, u16, Bytes) {
        loop {
            let packet = rxxbpacket_wait(ser);
            let mut frame = if let Some(olddata) = self.buf.get(&packet.sender_addr64) {
                *olddata
            } else {
                BytesMut::new()
            }

            frame.extend_from_slice(&packet.payload[1..]);
            if packet.payload[0] == 0x0 {
                self.buf.remove(&packet.sender_addr64);
                return (packet.sender_addr64, packet.sender_addr16, frame.freeze());
            } else {
                self.buf.insert(packet.sender_addr64, frame);
            }
        }
    }

    pub fn discardframes(&mut self, ser: &XBSer) -> () {
        loop {
            let _ = self.rxframe(ser);
        }
    }
}
