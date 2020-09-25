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
mod tap;
mod tun;
mod xb;
mod xbpacket;
mod xbrx;

use std::path::PathBuf;
use std::time::Duration;
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

    /// Serial port to use to communicate with radio
    #[structopt(parse(from_os_str))]
    port: PathBuf,

    /// The speed in bps (baud rate) to use to communicate on the serial port
    #[structopt(long, default_value = "9600")]
    serial_speed: u32,

    /// Disable the Xbee-level ACKs
    #[structopt(long)]
    disable_xbee_acks: bool,

    /// Request XBee transmit reports.  These will appear in debug mode but otherwise are not considered.
    #[structopt(long)]
    request_xbee_tx_reports: bool,

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
    /// Create a virtual Ethernet interface and send frames across XBee
    Tap {
        /// Broadcast to XBee, instead of dropping, packets to unknown destinations.  Has no effect if --broadcast_everything is given.
        #[structopt(long)]
        broadcast_unknown: bool,

        /// Broadcast every packet out the XBee side
        #[structopt(long)]
        broadcast_everything: bool,

        /// Name for the interface; defaults to "xbnet%d" which the OS usually turns to "xbnet0".
        /// Note that this name is not guaranteed; the name allocated by the OS is displayed
        /// at startup.
        #[structopt(long, default_value = "xbnet%d")]
        iface_name: String,
    },
    /// Create a virtual IP interface and send frames across XBee
    Tun {
        /// Broadcast every packet out the XBee side
        #[structopt(long)]
        broadcast_everything: bool,

        /** The maximum number of seconds to store the destination XBee MAC for an IP address. */
        #[structopt(long, default_value = "300")]
        max_ip_cache: u64,

        /// Name for the interface; defaults to "xbnet%d" which the OS usually turns to "xbnet0".
        /// Note that this name is not guaranteed; the name allocated by the OS is displayed
        /// at startup.
        #[structopt(long, default_value = "xbnet%d")]
        iface_name: String,

        /// Disable all IPv4 support
        #[structopt(long)]
        disable_ipv4: bool,

        /// Disable all IPv6 support
        #[structopt(long)]
        disable_ipv6: bool,

    },
}

fn main() {
    let opt = Opt::from_args();

    if opt.debug {
        WriteLogger::init(LevelFilter::Trace, Config::default(), io::stderr())
            .expect("Failed to init log");
    }
    info!("xbnet starting");

    let (ser_reader, ser_writer) = ser::new(opt.port, opt.serial_speed).expect("Failed to initialize serial port");
    let (mut xb, xbeesender, writerthread) = xb::XB::new(
        ser_reader,
        ser_writer,
        opt.initfile,
        opt.disable_xbee_acks,
        opt.request_xbee_tx_reports,
    );
    let mut xbreframer = xbrx::XBReframer::new();

    match opt.cmd {
        Command::Ping { dest } => {
            let dest_u64: u64 = u64::from_str_radix(&dest, 16).expect("Invalid destination");
            thread::spawn(move || {
                ping::genpings(dest_u64, xbeesender).expect("Failure in genpings")
            });
            ping::displaypongs(&mut xbreframer, &mut xb.ser_reader);
            // Make sure queued up data is sent
            let _ = writerthread.join();
        }
        Command::Pong => {
            ping::pong(&mut xbreframer, &mut xb.ser_reader, xbeesender).expect("Failure in pong");
            // Make sure queued up data is sent
            let _ = writerthread.join();
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
        Command::Tap {
            broadcast_unknown,
            broadcast_everything,
            iface_name,
        } => {
            let tap_reader = tap::XBTap::new_tap(
                xb.mymac,
                broadcast_unknown,
                broadcast_everything,
                iface_name,
            )
            .expect("Failure initializing tap");
            let tap_writer = tap_reader.clone();
            thread::spawn(move || {
                tap_writer
                    .frames_from_xb_processor(&mut xbreframer, &mut xb.ser_reader)
                    .expect("Failure in frames_from_xb_processor");
            });
            tap_reader
                .frames_from_tap_processor(xbeesender)
                .expect("Failure in frames_from_tap_processor");
            // Make sure queued up data is sent
            let _ = writerthread.join();
        }
        Command::Tun {
            broadcast_everything,
            iface_name,
            max_ip_cache,
            disable_ipv4,
            disable_ipv6,
        } => {
            let max_ip_cache = Duration::from_secs(max_ip_cache);
            let tun_reader =
                tun::XBTun::new_tun(xb.mymac, broadcast_everything, iface_name, max_ip_cache, disable_ipv4, disable_ipv6)
                    .expect("Failure initializing tun");
            let tun_writer = tun_reader.clone();
            thread::spawn(move || {
                tun_writer
                    .frames_from_xb_processor(&mut xbreframer, &mut xb.ser_reader)
                    .expect("Failure in frames_from_xb_processor");
            });
            tun_reader
                .frames_from_tun_processor(xbeesender)
                .expect("Failure in frames_from_tap_processor");
            // Make sure queued up data is sent
            let _ = writerthread.join();
        }
    }
}
