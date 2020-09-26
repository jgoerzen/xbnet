# XBee Networking Tools

![build](https://github.com/jgoerzen/xbnet/workflows/build/badge.svg) ![docs](https://docs.rs/xbnet/badge.svg)


This package is for doing fantastic things with your XBee device.  You can, of course, already use it as a serial replacement, so you can run PPP and UUCP across it.  XBee radios are low-cost, long-range, low-speed devices; with bitrates from 10Kbps to 250Kbps, they can reach many miles using simple antennas and low cost.

With xbnet, you can also run Ethernet across it.  Or ZModem.  Or TCP/IP (IPv4 and IPv6).  SPX if you want?  I guess so.  SSH?  Of course!

This is tested with the XBee SX modules, but ought to work with any modern XBee module.

XBee devices are particularly interesting because of their self-healing mesh (DigiMesh) technology.  They will auto-route traffic to the destination, via intermediate hops if necessary.  They also support bitrates high enough for a TCP stack, with nearly the range of LoRA.

**For details, see the [extensive documentation](https://github.com/jgoerzen/xbnet/blob/master/doc/xbnet.1.md)**.

This is a followup to, and fork of, my [lorapipe](https://github.com/jgoerzen/lorapipe) project, which is something similar for LoRA radios.

# Copyright

    Copyright (C) 2019-2020 John Goerzen <jgoerzen@complete.org>

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
