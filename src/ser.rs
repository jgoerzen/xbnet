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

use bytes::*;
use log::*;
use serialport::prelude::*;
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::Duration;

pub struct XBSerReader {
    pub br: BufReader<Box<dyn SerialPort>>,
    pub portname: PathBuf,
}

pub struct XBSerWriter {
    pub swrite: Box<dyn SerialPort>,
    pub portname: PathBuf,
}

/// Initialize the serial system, configuring the port.
pub fn new(portname: PathBuf, speed: u32) -> io::Result<(XBSerReader, XBSerWriter)> {
    let settings = SerialPortSettings {
        baud_rate: speed,
        data_bits: DataBits::Eight,
        flow_control: FlowControl::Hardware,
        parity: Parity::None,
        stop_bits: StopBits::One,
        timeout: Duration::new(60 * 60 * 24 * 365 * 20, 0),
    };
    let readport = serialport::open_with_settings(&portname, &settings)?;
    let writeport = readport.try_clone()?;

    Ok((
        XBSerReader {
            br: BufReader::new(readport),
            portname: portname.clone(),
        },
        XBSerWriter {
            swrite: writeport,
            portname,
        },
    ))
}

impl XBSerReader {
    /// Read a line from the port.  Return it with EOL characters removed.
    /// None if EOF reached.
    pub fn readln(&mut self) -> io::Result<Option<String>> {
        let mut buf = Vec::new();
        let size = self.br.read_until(0x0D, &mut buf)?;
        let buf = String::from_utf8_lossy(&buf);
        if size == 0 {
            debug!("{:?}: Received EOF from serial port", self.portname);
            Ok(None)
        } else {
            let buf = String::from(buf.trim());
            trace!("{:?} SERIN: {}", self.portname, buf);
            Ok(Some(buf))
        }
    }
}

impl XBSerWriter {
    /// Transmits a command with terminating EOL characters
    pub fn writeln(&mut self, data: &str) -> io::Result<()> {
        trace!("{:?} SEROUT: {}", self.portname, data);
        let mut data = BytesMut::from(data.as_bytes());
        data.put(&b"\r\n"[..]);
        // Give the receiver a chance to process
        self.swrite.write_all(&data)?;
        self.swrite.flush()
    }
}
