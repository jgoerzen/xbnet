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
use crate::xb::*;
use crate::xbpacket::*;
use crate::xbrx::*;
use bytes::*;
use crossbeam_channel;
use std::io;
use std::thread;
use std::time::Duration;

const INTERVAL: u64 = 5; // FIXME: this should be configurable

pub fn genpings(dest: u64, sender: crossbeam_channel::Sender<XBTX>) -> io::Result<()> {
    let mut counter: u64 = 1;
    loop {
        let sendstr = format!("Ping {}", counter);
        println!("SEND: {}", sendstr);
        sender
            .send(XBTX::TXData(XBDestAddr::U64(dest), Bytes::from(sendstr)))
            .unwrap();
        thread::sleep(Duration::from_secs(INTERVAL));
        counter += 1;
    }
}

/// Show pongs
pub fn displaypongs(xbreframer: &mut XBReframer, ser: &mut XBSerReader) -> () {
    loop {
        let (fromu64, _fromu16, payload) = xbreframer.rxframe(ser);
        println!(
            "RECV from {}: {}",
            hex::encode(fromu64.to_be_bytes()),
            String::from_utf8_lossy(&payload)
        );
    }
}

/// Reply to pings
pub fn pong(
    xbreframer: &mut XBReframer,
    ser: &mut XBSerReader,
    sender: crossbeam_channel::Sender<XBTX>,
) -> io::Result<()> {
    loop {
        let (fromu64, _addr_16, payload) = xbreframer.rxframe(ser);
        if payload.starts_with(b"Ping ") {
            println!(
                "RECV from {}: {}",
                hex::encode(fromu64.to_be_bytes()),
                String::from_utf8_lossy(&payload)
            );
            let resp = Bytes::from(format!("Pong {}", String::from_utf8_lossy(&payload[5..])));
            sender
                .send(XBTX::TXData(XBDestAddr::U64(fromu64), resp))
                .unwrap();
        }
    }
}
