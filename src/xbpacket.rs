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

/** XBee transmissions can give either a 64-bit or a 16-bit destination
address.  This permits the user to select one. */
#[derive(Eq, PartialEq, Debug)]
pub enum XBDestAddr {
    /// A 16-bit destination address.  When a 64-bit address is given, this is transmitted as 0xFFFE.
    U16(u16),

    /// The 64-bit destination address.  0xFFFF for broadcast.
    /// When a 16-bit destination is given, this will be transmitted as 0xFFFFFFFFFFFFFFFF.
    U64(u64)
}

/** Possible errors from serialization */
#[derive(Eq, PartialEq, Debug)]
pub enum TXGenError {
    /// The payload was an invalid length
    InvalidLen
}

/** A Digi 64-bit transmit request, frame type 0x10 */
#[derive(Eq, PartialEq, Debug)]
pub struct XBTXRequest<'a> {
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
    pub payload: &'a [u8],
}

impl XBTXRequest {
    pub fn serialize(&self) -> Result<Bytes, TXGenError> {
        if self.payload.is_empty() {
            return Err(TXGenError::InvalidLen);
        }

        // We generate the bits that are outside the length & checksum parts, then the
        // inner parts, then combine them.
        let mut fullframe = BytesMut::new();

        fullframe.put_u8(0x7e);       // Start delimeter

        let mut innerframe = BytesMut::new();
        // Frame type
        innerframe.put_u8(0x10);

        innerframe.put_u8(self.frame_id);
        match self.dest_addr {
            XBDestAddr::U16(dest) => {
                innerframe.put_u64(0xFFFFFFFFFFFFFFFFu64);
                innerframe.put_u16(dest);
            },
            XBDestAddr::U64(dest) => {
                innerframe.put_u64(dest);
                innerframe.put_u16(0xFFFEu16);
            }
        };

        innerframe.put_u8(self.broadcast_radius);
        innerframe.put_u8(self.transmit_options);
        innerframe.put_slice(self.payload);

        // That's it for the inner frame.  Now fill in the outer frame.
        if let Some(lenu16) = u16::try_from(self.payload.len()) {
            fullframe.put_u16(lenu16);
            fullframe.put_slice(self.innerframe);
            fullframe.put_u8(xbchecksum(self.innerframe));
            Ok(fullframe.freeze())
        } else {
            Err(TXGenError::InvalidLen)
        }
    }
}

/// Calculate an XBee checksum over a slice
pub fn xbchecksum(data: &[u8]) -> u8 {
    let sumu64 = data.into_iter().map(|x| u64::from(x)).sum();
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
    let macbytes = mac64.to_be_bytes;
    macbytes[2..]
}

/** Return a 64-bit MAC given a pattern 64-bit MAC and a 48-bit MAC. The 16 most
significant bits from the pattern will be used to complete the 48-bit MAC to 64-bit.
*/
pub fn mac48to64(mac48: &[u8; 6], pattern64: u64) -> u64 {
    let mut mac64bytes = [0u8; 8];
    mac64bytes[2..] = mac48;
    let mut mac64 = u64::from_be_bytes(mac64bytes);
    mac64 |= pattern64 & 0xffff000000000000;
    mac64
}

