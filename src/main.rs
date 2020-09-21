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

use simplelog::*;
use std::io;
use log::*;
use std::thread;

mod ser;
mod xb;
mod xbpacket;
mod xbrx;
// mod pipe;
mod ping;

use std::path::PathBuf;
use structopt::StructOpt;
use std::convert::TryInto;

#[derive(Debug, StructOpt)]
#[structopt(name = "xbnet", about = "Networking for XBee Radios", author = "John Goerzen <jgoerzen@complete.org>")]
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
    cmd: Command
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
}

fn main() {
    let opt = Opt::from_args();

    if opt.debug {
        WriteLogger::init(LevelFilter::Trace, Config::default(), io::stderr()).expect("Failed to init log");
    }
    info!("lora starting");

    let xbser = ser::XBSer::new(opt.port).expect("Failed to initialize serial port");
    let (xb, xbeesender) = xb::XB::new(xbser, opt.initfile);
    let mut xbreframer = xbrx::XBReframer::new();


    match opt.cmd {
        Command::Ping{dest} => {
            let dest_u64:u64 = u64::from_str_radix(&dest, 16).expect("Invalid destination");
            thread::spawn(move || ping::genpings(dest_u64, xbeesender).expect("Failure in genpings"));
            ping::displaypongs(&mut xbreframer, &xb.ser);
        },
        Command::Pong => {
            ping::pong(&mut xbreframer, &xb.ser, xbeesender).expect("Failure in loratostdout");
        }
    }
}
