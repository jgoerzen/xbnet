/*
    Copyright (C) 2019-2020 John Goerzen <jgoerzen@complete.org

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

use log::*;
use simplelog::*;
use std::io;
use std::thread;

mod ping;
mod pipe;
mod ser;
mod xb;
mod xbpacket;
mod xbrx;

use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "xbnet",
    about = "Networking for XBee Radios",
    author = "John Goerzen <jgoerzen@complete.org>"
)]
struct Opt {
    /// Activate debug mode
    // short and long flags (-d, --debug) will be deduced from the field's name
    #[structopt(short, long)]
    debug: bool,

    /// Radio initialization command file
    #[structopt(long, parse(from_os_str))]
    initfile: Option<PathBuf>,

    #[structopt(parse(from_os_str))]
    /// Serial port to use to communicate with radio
    port: PathBuf,

    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Transmit ping requests
    Ping {
        /// The 64-bit destination for the ping, in hex
        #[structopt(long)]
        dest: String,
    },
    /// Receive ping requests and transmit pongs
    Pong,
    /// Pipe data across radios using the xbnet protocol
    Pipe {
        /// The 64-bit destination for the pipe, in hex
        #[structopt(long)]
        dest: String,
        // FIXME: add a paremter to accept data from only that place
    },
}

fn main() {
    let opt = Opt::from_args();

    if opt.debug {
        WriteLogger::init(LevelFilter::Trace, Config::default(), io::stderr())
            .expect("Failed to init log");
    }
    info!("xbnet starting");

    let (ser_reader, ser_writer) = ser::new(opt.port).expect("Failed to initialize serial port");
    let (mut xb, xbeesender, writerthread) = xb::XB::new(ser_reader, ser_writer, opt.initfile);
    let mut xbreframer = xbrx::XBReframer::new();

    match opt.cmd {
        Command::Ping { dest } => {
            let dest_u64: u64 = u64::from_str_radix(&dest, 16).expect("Invalid destination");
            thread::spawn(move || {
                ping::genpings(dest_u64, xbeesender).expect("Failure in genpings")
            });
            ping::displaypongs(&mut xbreframer, &mut xb.ser_reader);
        }
        Command::Pong => {
            ping::pong(&mut xbreframer, &mut xb.ser_reader, xbeesender).expect("Failure in pong");
        }
        Command::Pipe { dest } => {
            let dest_u64: u64 = u64::from_str_radix(&dest, 16).expect("Invalid destination");
            let maxpacketsize = xb.maxpacketsize;
            thread::spawn(move || {
                pipe::stdout_processor(&mut xbreframer, &mut xb.ser_reader)
                    .expect("Failure in stdout_processor")
            });
            pipe::stdin_processor(dest_u64, maxpacketsize - 1, xbeesender)
                .expect("Failure in stdin_processor");
            // Make sure queued up data is sent
            let _ = writerthread.join();
        }
    }
}
