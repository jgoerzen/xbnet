/*
    Copyright (C) 2019  John Goerzen <jgoerzen@complete.org

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

use std::io;
use std::io::{Read, Write};
use crate::xb::*;
use crate::xbpacket::*;
use crate::ser::*;
use crate::xbrx::*;
use crossbeam_channel;
use std::thread;
use std::time::Duration;
use bytes::*;

const INTERVAL: u64 = 5;

pub fn stdin_processor(dest: u64, maxframesize: usize,
                       sender: crossbeam_channel::Sender<XBTX>) -> io::Result<()> {
    let stdin = io::stdin();
    let mut br = io::BufReader::new(stdin);
    let mut buf = vec![0u8; maxframesize - 1];

    loop {
        let res = br.read(&mut buf)?;
        if res == 0 {
            // EOF
            sender.send(XBTX::Shutdown).unwrap();
            return Ok(());
        }

        sender.send(XBTX::TXData(XBDestAddr::U64(dest), Bytes::copy_from_slice(&buf[0..res]))).unwrap();
    }
}

pub fn stdout_processor(xbreframer: &mut XBReframer, ser: &mut XBSerReader) -> io::Result<()> {
    let mut stdout = io::stdout();
    loop {
        let (_fromu64, _fromu16, payload) = xbreframer.rxframe(ser);
        stdout.write_all(&payload)?;
        stdout.flush()?;
    }
}
