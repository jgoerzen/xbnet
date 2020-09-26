/*! tun virtual IP gateway */

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

use tun_tap::{Iface, Mode};

use crate::ser::*;
use crate::xb::*;
use crate::xbpacket::*;
use crate::xbrx::*;
use bytes::*;
use crossbeam_channel;
use etherparse::*;
use log::*;
use std::collections::HashMap;
use std::io;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const XB_BROADCAST: u64 = 0xffff;

#[derive(Clone)]
pub struct XBTun {
    pub myxbmac: u64,
    pub name: String,
    pub broadcast_everything: bool,
    pub tun: Arc<Iface>,
    pub max_ip_cache: Duration,
    pub disable_ipv4: bool,
    pub disable_ipv6: bool,

    /** The map from IP Addresses (v4 or v6) to destination MAC addresses.  Also
    includes a timestamp at which the destination expires. */
    pub dests: Arc<Mutex<HashMap<IpAddr, (u64, Instant)>>>,
}

impl XBTun {
    pub fn new_tun(
        myxbmac: u64,
        broadcast_everything: bool,
        iface_name_requested: String,
        max_ip_cache: Duration,
        disable_ipv4: bool,
        disable_ipv6: bool,
    ) -> io::Result<XBTun> {
        let tun = Iface::without_packet_info(&iface_name_requested, Mode::Tun)?;
        let name = tun.name();

        println!("Interface {} (XBee MAC {:x}) ready", name, myxbmac,);

        let desthm = HashMap::new();

        Ok(XBTun {
            myxbmac,
            broadcast_everything,
            max_ip_cache,
            disable_ipv4,
            disable_ipv6,
            name: String::from(name),
            tun: Arc::new(tun),
            dests: Arc::new(Mutex::new(desthm)),
        })
    }

    pub fn get_xb_dest_mac(&self, ipaddr: &IpAddr) -> u64 {
        if self.broadcast_everything {
            return XB_BROADCAST;
        }

        match self.dests.lock().unwrap().get(ipaddr) {
            // Broadcast if we don't know it
            None => {
                XB_BROADCAST
            },
            Some((dest, expiration)) => {
                if Instant::now() >= *expiration {
                    // Broadcast it if the cache entry has expired
                    XB_BROADCAST
                } else {
                    *dest
                }
            }
        }
    }

    pub fn frames_from_tun_processor(
        &self,
        sender: crossbeam_channel::Sender<XBTX>,
    ) -> io::Result<()> {
        let mut buf = [0u8; 9100]; // Enough to handle even jumbo frames
        loop {
            let size = self.tun.recv(&mut buf)?;
            let tundata = &buf[0..size];
            trace!("TUNIN: {}", hex::encode(tundata));
            match SlicedPacket::from_ip(tundata) {
                Err(x) => {
                    warn!("Error parsing packet from tun; discarding: {:?}", x);
                }
                Ok(packet) => {
                    let ips = extract_ips(&packet);
                    if let Some((source, destination)) = ips {
                        match destination {
                            IpAddr::V6(_) =>
                                if self.disable_ipv6 {
                                    debug!("Dropping packet because --disable-ipv6 given");
                                    continue;
                                },
                            IpAddr::V4(_) =>
                                if self.disable_ipv4 {
                                    debug!("Dropping packet because --disable-ipv4 given");
                                    continue;
                                }
                        };

                        let destxbmac = self.get_xb_dest_mac(&destination);
                        trace!(
                            "TAPIN: Packet {} -> {} (MAC {:x})",
                            source,
                            destination,
                            destxbmac
                        );
                        let res = sender.try_send(XBTX::TXData(
                            XBDestAddr::U64(destxbmac),
                            Bytes::copy_from_slice(tundata),
                        ));
                        match res {
                            Ok(()) => (),
                            Err(crossbeam_channel::TrySendError::Full(_)) => {
                                debug!("Dropped packet due to full TX buffer")
                            }
                            Err(e) => Err(e).unwrap(),
                        }
                    } else {
                        warn!("Unable to get IP header from tun packet; discarding");
                    }
                }
            }
        }
    }

    pub fn frames_from_xb_processor(
        &self,
        xbreframer: &mut XBReframer,
        ser: &mut XBSerReader,
    ) -> io::Result<()> {
        loop {
            let (fromu64, _fromu16, payload) = xbreframer.rxframe(ser);

            // Register the sender in our map of known MACs
            match SlicedPacket::from_ip(&payload) {
                Err(x) => {
                    warn!(
                        "Packet from XBee wasn't valid IPv4 or IPv6; continuing anyhow: {:?}",
                        x
                    );
                }
                Ok(packet) => {
                    let ips = extract_ips(&packet);
                    if let Some((source, destination)) = ips {
                        trace!("SERIN: Packet is {} -> {}", source, destination);
                        match source {
                            IpAddr::V6(_) =>
                                if self.disable_ipv6 {
                                    debug!("Dropping packet because --disable-ipv6 given");
                                    continue;
                                },
                            IpAddr::V4(_) =>
                                if self.disable_ipv4 {
                                    debug!("Dropping packet because --disable-ipv4 given");
                                    continue;
                                }
                        }
                        if !self.broadcast_everything {
                            self.dests.lock().unwrap().insert(
                                source,
                                (
                                    fromu64,
                                    Instant::now().checked_add(self.max_ip_cache).unwrap(),
                                ),
                            );
                        }
                    }
                }
            }

            match self.tun.send(&payload) {
                Ok(_) => (),
                Err(e) => {
                    warn!("Failure to send packet to tun interface; have you given it an IP?  Error: {}", e);
                }
            }
        }
    }
}

/// Returns the source and destination IPs
pub fn extract_ips<'a>(packet: &SlicedPacket<'a>) -> Option<(IpAddr, IpAddr)> {
    match &packet.ip {
        Some(InternetSlice::Ipv4(header)) => Some((
            IpAddr::V4(header.source_addr()),
            IpAddr::V4(header.destination_addr()),
        )),
        Some(InternetSlice::Ipv6(header, _)) => Some((
            IpAddr::V6(header.source_addr()),
            IpAddr::V6(header.destination_addr()),
        )),
        _ => None,
    }
}
