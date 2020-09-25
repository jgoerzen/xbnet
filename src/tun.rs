/*! tap virtual Ethernet gateway */

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
use std::convert::TryInto;
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};
use ifstructs::ifreq;
use libc;

pub const ETHER_BROADCAST: [u8; 6] = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
pub const XB_BROADCAST: u64 = 0xffff;

#[derive(Clone)]
pub struct XBTap {
    pub myethermac: [u8; 6],
    pub myxbmac: u64,
    pub myethermacstr: String,
    pub name: String,
    pub broadcast_unknown: bool,
    pub broadcast_everything: bool,
    pub tap: Arc<Iface>,

    /** We can't just blindly generate destination MACs because there is a bug
    in the firmware that causes the radio to lock up if we send too many
    packets to a MAC that's not online.  So, we keep a translation map of
    MACs we've seen. */
    pub dests: Arc<Mutex<HashMap<[u8; 6], u64>>>,
}

impl XBTap {
    pub fn new_tap(myxbmac: u64, broadcast_unknown: bool, broadcast_everything: bool, iface_name_requested: String) -> io::Result<XBTap> {
        let myethermac = mac64to48(myxbmac);
        let myethermacstr = showmac(&myethermac);
        let tap = Iface::without_packet_info(&iface_name_requested, Mode::Tap)?;
        let name = tap.name();

        // Set the MAC address.
        let mut sa_data = [0u8 as libc::c_char; 14];
        let c_mac: Vec<libc::c_char> = myethermac[..].iter().map(|x| *x as libc::c_char).collect();
        sa_data[0..=5].copy_from_slice(c_mac.as_slice());
        let sockaddr = libc::sockaddr {sa_family: libc::ARPHRD_ETHER,
                                       sa_data
        };

        let mut ifr = ifreq::from_name(name).unwrap();
        ifr.ifr_ifru.ifr_hwaddr = sockaddr;

        unsafe {
        let socket = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
        assert!(socket >= 0);
        let pointer: *mut ifreq = &mut ifr;
        let ioctlres = libc::ioctl(socket, libc::SIOCSIFHWADDR, pointer);
        assert!(ioctlres != -1);
        libc::close(socket);
        }

        println!(
            "Interface {} with ether MAC {} (XBee MAC {:x}) ready",
            name,
            myethermacstr,
            myxbmac,
        );

        let mut desthm = HashMap::new();
        desthm.insert(ETHER_BROADCAST, XB_BROADCAST);


        Ok(XBTap {
            myxbmac,
            myethermac,
            myethermacstr,
            broadcast_unknown,
            broadcast_everything,
            name: String::from(name),
            tap: Arc::new(tap),
            dests: Arc::new(Mutex::new(desthm)),
        })
    }

    pub fn get_xb_dest_mac(&self, ethermac: &[u8; 6]) -> Option<u64> {
        if self.broadcast_everything {
            return Some(XB_BROADCAST);
        }

        match self.dests.lock().unwrap().get(ethermac) {
            None =>
                if self.broadcast_unknown {
                    Some(XB_BROADCAST)
                } else {
                    None
                },
            Some(dest) => Some(*dest),
        }
    }

    pub fn frames_from_tap_processor(
        &self,
        maxframesize: usize,
        sender: crossbeam_channel::Sender<XBTX>,
    ) -> io::Result<()> {
        let mut buf = [0u8; 9100]; // Enough to handle even jumbo frames
        loop {
            let size = self.tap.recv(&mut buf)?;
            let tapdata = &buf[0..size];
            trace!("TAPIN: {}", hex::encode(tapdata));
            match SlicedPacket::from_ethernet(tapdata) {
                Err(x) => {
                    warn!("Error parsing packet from tap; discarding: {:?}", x);
                }
                Ok(packet) => {
                    if let Some(LinkSlice::Ethernet2(header)) = packet.link {
                        trace!("TAPIN: Packet is {} -> {}", hex::encode(header.source()), hex::encode(header.destination()));
                        if header.source() != &self.myethermac {
                            warn!("Packet from tap with MAC address {} mismatches my own MAC address of {}; proceeding anyway",
                                  showmac(header.source().try_into().unwrap()), self.myethermacstr);
                        }
                        match self.get_xb_dest_mac(header.destination().try_into().unwrap()) {
                            None =>
                                warn!("Destination MAC address unknown; discarding packet"),
                            Some(destxbmac) =>
                                {
                                    let res =
                                        sender
                                        .try_send(XBTX::TXData(
                                            XBDestAddr::U64(destxbmac),
                                            Bytes::copy_from_slice(tapdata),
                                        ));
                                    match res {
                                        Ok(()) => (),
                                        Err(crossbeam_channel::TrySendError::Full(_)) =>
                                            debug!("Dropped packet due to full TX buffer"),
                                        Err(e) => Err(e).unwrap(),
                                    }
                                }
                        }
                    } else {
                        warn!("Unable to get Ethernet2 header from tap packet; discarding");
                    }
                }
            }
        }
    }
    pub fn frames_from_xb_processor(
        &self,
        xbreframer: &mut XBReframer,
        ser: &mut XBSerReader) -> io::Result<()> {
        loop {
            let (fromu64, _fromu16, payload) = xbreframer.rxframe(ser);

            // Register the sender in our map of known MACs
            match SlicedPacket::from_ethernet(&payload) {
                Err(x) => {
                    warn!("Packet from XBee wasn't valid Ethernet; continueing anyhow: {:?}", x);
                }
                Ok(packet) => {
                    if let Some(LinkSlice::Ethernet2(header)) = packet.link {
                        trace!("SERIN: Packet Ethernet header is {} -> {}", hex::encode(header.source()), hex::encode(header.destination()));
                        if ! self.broadcast_everything {
                            self.dests.lock().unwrap().insert(header.source().try_into().unwrap(), fromu64);
                        }
                    }
                }
            }

            self.tap.send(&payload)?;
        }
    }
}


pub fn showmac(mac: &[u8; 6]) -> String {
    format!(
        "{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
}
