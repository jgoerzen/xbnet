/*! XBee packet transmission */
/*
    Copyright (C) 2020  John Goerzen <jgoerzen@complete.org

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

use bytes::*;
use log::*;
use std::convert::{TryFrom, TryInto};
use std::fmt;

/** XBee transmissions can give either a 64-bit or a 16-bit destination
address.  This permits the user to select one. */
#[derive(Eq, PartialEq, Clone)]
pub enum XBDestAddr {
    /// A 16-bit destination address.  When a 64-bit address is given, this is transmitted as 0xFFFE.
    U16(u16),

    /// The 64-bit destination address.  0xFFFF for broadcast.
    /// When a 16-bit destination is given, this will be transmitted as 0xFFFFFFFFFFFFFFFF.
    U64(u64),
}

impl fmt::Debug for XBDestAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XBDestAddr::U16(x) => {
                f.write_str("U16(")?;
                f.write_str(&hex::encode(x.to_be_bytes()))?;
                f.write_str(")")
            }
            XBDestAddr::U64(x) => {
                f.write_str("U64(")?;
                f.write_str(&hex::encode(x.to_be_bytes()))?;
                f.write_str(")")
            }
        }
    }
}

/** Possible errors from serialization */
#[derive(Eq, PartialEq, Debug)]
pub enum TXGenError {
    /// The payload was an invalid length
    InvalidLen,
}

/** A Digi 64-bit transmit request, frame type 0x10 */
#[derive(Eq, PartialEq, Debug)]
pub struct XBTXRequest {
    /// The frame ID, which will be returned in the subsequent response frame.
    /// Set to 0 to disable a response for this transmission.
    pub frame_id: u8,

    /// The destination address
    pub dest_addr: XBDestAddr,

    /// The number of hops a broadcast transmission can traverse.  When 0, the value if NH is used.
    pub broadcast_radius: u8,

    /// Transmit options bitfield.  When 0, uses the TO setting.
    pub transmit_options: u8,

    /// The payload
    pub payload: Bytes,
}

impl XBTXRequest {
    pub fn serialize(&self) -> Result<Bytes, TXGenError> {
        if self.payload.is_empty() {
            return Err(TXGenError::InvalidLen);
        }

        // We generate the bits that are outside the length & checksum parts, then the
        // inner parts, then combine them.
        let mut fullframe = BytesMut::new();

        fullframe.put_u8(0x7e); // Start delimeter

        let mut innerframe = BytesMut::new();
        // Frame type
        innerframe.put_u8(0x10);

        innerframe.put_u8(self.frame_id);
        match self.dest_addr {
            XBDestAddr::U16(dest) => {
                innerframe.put_u64(0xFFFFFFFFFFFFFFFFu64);
                innerframe.put_u16(dest);
            }
            XBDestAddr::U64(dest) => {
                innerframe.put_u64(dest);
                innerframe.put_u16(0xFFFEu16);
            }
        };

        innerframe.put_u8(self.broadcast_radius);
        innerframe.put_u8(self.transmit_options);
        innerframe.put_slice(&self.payload);

        // That's it for the inner frame.  Now fill in the outer frame.
        if let Ok(lenu16) = u16::try_from(innerframe.len()) {
            fullframe.put_u16(lenu16);
            fullframe.put_slice(&innerframe);
            fullframe.put_u8(xbchecksum(&innerframe));
            Ok(fullframe.freeze())
        } else {
            Err(TXGenError::InvalidLen)
        }
    }
}

/// Calculate an XBee checksum over a slice
pub fn xbchecksum(data: &[u8]) -> u8 {
    let sumu64: u64 = data.into_iter().map(|x| u64::from(*x)).sum();
    0xffu8 - (sumu64 as u8)
}

/** Return a 48-bit MAC given the 64-bit MAC.  Truncates the most significant bits.

# Example

```
use xbnet::xbpacket::*;

let mac64 = 0x123456789abcdeffu64;
let mac48 = mac64to48(mac64);
assert_eq!([0x56, 0x78, 0x9a, 0xbc, 0xde, 0xff], mac48);
assert_eq(mac64, mac48to64(mac48, mac64));
```
*/
pub fn mac64to48(mac64: u64) -> [u8; 6] {
    let macbytes = mac64.to_be_bytes();
    macbytes[2..].try_into().unwrap()
}

/** Return a 64-bit MAC given a pattern 64-bit MAC and a 48-bit MAC. The 16 most
significant bits from the pattern will be used to complete the 48-bit MAC to 64-bit.
*/
pub fn mac48to64(mac48: &[u8; 6], pattern64: u64) -> u64 {
    let mut mac64bytes = [0u8; 8];
    mac64bytes[2..].copy_from_slice(mac48);
    let mut mac64 = u64::from_be_bytes(mac64bytes);
    mac64 |= pattern64 & 0xffff000000000000;
    mac64
}

pub struct PacketStream {
    /// The counter for the frame
    framecounter: u8,
}

impl PacketStream {
    pub fn new() -> Self {
        PacketStream { framecounter: 1 }
    }

    pub fn get_and_incr_framecounter(&mut self) -> u8 {
        let retval = self.framecounter;
        if self.framecounter == std::u8::MAX {
            self.framecounter = 1
        } else {
            self.framecounter += 1
        }
        retval
    }

    /** Convert the given data into zero or more packets for transmission.

    We create a leading byte that indicates how many more XBee packets are remaining
    for the block.  When zero, the receiver should process the accumulated data. */
    pub fn packetize_data(
        &mut self,
        maxpacketsize: usize,
        dest: &XBDestAddr,
        data: &[u8],
        disable_xbee_acks: bool,
        request_xbee_tx_reports: bool,
    ) -> Result<Vec<XBTXRequest>, String> {
        let mut retval = Vec::new();
        if data.is_empty() {
            return Ok(retval);
        }

        // trace!("xbpacket: data len {}", data.len());
        let chunks: Vec<&[u8]> = data.chunks(maxpacketsize - 1).collect();
        // trace!("xbpacket: chunk count {}", chunks.len());
        let mut chunks_remaining: u8 = u8::try_from(chunks.len())
            .map_err(|e| String::from("More than 255 chunks to transmit"))?;
        for chunk in chunks {
            // trace!("xbpacket: chunks_remaining: {}", chunks_remaining);
            let mut payload = BytesMut::new();
            payload.put_u8(chunks_remaining - 1);
            payload.put_slice(chunk);
            let frame_id = if request_xbee_tx_reports {
                self.get_and_incr_framecounter()
            } else {
                0
            };
            let packet = XBTXRequest {
                frame_id,
                dest_addr: dest.clone(),
                broadcast_radius: 0,
                transmit_options: if disable_xbee_acks { 0x01 } else { 0 },
                payload: Bytes::from(payload),
            };

            retval.push(packet);
            chunks_remaining -= 1;
        }

        Ok(retval)
    }
}

//////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////
// RX side

/** A Digi receive packet, 0x90 */
#[derive(PartialEq, Eq, Debug)]
pub struct RXPacket {
    pub sender_addr64: u64,
    pub sender_addr16: u16,
    pub rx_options: u8,
    pub payload: Bytes,
}

/** A Digi extended transmit status frame, 0x8B */
#[derive(PartialEq, Eq, Debug)]
pub struct ExtTxStatus {
    pub frame_id: u8,
    pub dest_addr_16: u16,
    pub tx_retry_count: u8,
    pub delivery_status: u8,
    pub discovery_status: u8,
}
