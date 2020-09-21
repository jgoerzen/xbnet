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
use serialport::prelude::*;
use std::io::{BufReader, BufRead, Write};
use log::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::path::PathBuf;
use bytes::*;

#[derive(Clone)]
pub struct XBSer {
    // BufReader can't be cloned.  Sigh.
    pub br: Arc<Mutex<BufReader<Box<dyn SerialPort>>>>,
    pub swrite: Arc<Mutex<Box<dyn SerialPort>>>,
    pub portname: PathBuf
}

impl XBSer {

    /// Initialize the serial system, configuring the port.
    pub fn new(portname: PathBuf) -> io::Result<XBSer> {
        let settings = SerialPortSettings {
            baud_rate: 115200, // FIXME: make this configurable, default 9600
            data_bits: DataBits::Eight,
            flow_control: FlowControl::Hardware,
            parity: Parity::None,
            stop_bits: StopBits::One,
            timeout: Duration::new(60 * 60 * 24 * 365 * 20, 0),
        };
        let readport = serialport::open_with_settings(&portname, &settings)?;
        let writeport = readport.try_clone()?;
        
        Ok(XBSer {br: Arc::new(Mutex::new(BufReader::new(readport))),
                    swrite: Arc::new(Mutex::new(writeport)),
                    portname})
    }

    /// Read a line from the port.  Return it with EOL characters removed.
    /// None if EOF reached.
    pub fn readln(&mut self) -> io::Result<Option<String>> {
        let mut buf = Vec::new();
        let size = self.br.lock().unwrap().read_until(0x0D, &mut buf)?;
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

    /// Transmits a command with terminating EOL characters
    pub fn writeln(&mut self, data: &str) -> io::Result<()> {
        trace!("{:?} SEROUT: {}", self.portname, data);
        let mut data = BytesMut::from(data.as_bytes());
        data.put(&b"\r\n"[..]);
        // Give the receiver a chance to process
        // FIXME: lock this only once
        self.swrite.lock().unwrap().write_all(&data)?;
        self.swrite.lock().unwrap().flush()
    }
}


