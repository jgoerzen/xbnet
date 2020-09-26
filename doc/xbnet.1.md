% XBNET(1) John Goerzen | xbnet Manual
% John Goerzen
% October 2019

# NAME

xbnet - Transfer data and run a network over XBee long-range radios

# SYNOPSIS

**xbnet** [ *OPTIONS* ] **PORT** **COMMAND** [ *command_options* ]

# OVERVIEW

**xbnet** is designed to integrate XBee long-range radios into a
Unix/Linux system.  In particular, xbnet can:

- Bidirectionally pipe data across a XBee radio system
- Do an RF ping
- Operate as a virtual Ethernet device or a virtual tunnel device
  - Run TCP/IP (IPv4 and IPv6) atop either of these.

# HARDWARE REQUIREMENTS

**xbnet** is designed to run with a Digi XBee device.  It is tested with the SX devices but should work with any.

Drivers for other hardware may be added in the future.

# PROTOCOL

XBee frames are smaller than typical Ethernet or TCP frames.  XBee frames, in fact, are typically limited to about 255 bytes on the SX series; other devices may have different limits.  Therefore, xbnet supports fragmentation and reassembly.  It will split a frame to be transmitted into the size supported by XBee, and reassemble on the other end.

XBee, of course, cannot guarantee that all frames will be received, and therefore xbnet can't make that guarantee either.  However, the protocols you may run atop it -- from UUCP to ZModem to TCP/IP -- should handle this.

When running in **xbnet tap** mode, it is simulating an Ethernet interface.  Every Ethernet packet has a source and destination MAC address.  xbnet will maintain a cache of the Ethernet MAC addresses it has seen and what XBee MAC address they came from.  Therefore, when it sees a request to transmit to a certain Ethernet MAC, it will reuse what it knows from its cache and direct the packet to the appropriate XBee destination.  Ethernet broadcasts are converted into XBee broadcasts.

The **xbnet tun** mode operates in a similar fashion; it keeps a cache of seen IP addresses and their corresponding XBee MAC addresses, and directs packets appropriately.

# RADIO PARAMETERS AND INITIALIZATION

This program requires API mode from the board.  It will perform that initialization automatically.   Additional configurations may be added by you using the **--initfile** option.

# APPLICATION HINTS

## FULL TCP/IP USING TUN

This is the marquee feature of xbnet.  It provides a full TCP/IP stack across the XBee links, supporting both IPv4 and IPv6.  You can do anything you wish with the participating nodes in your mesh: ping, ssh, route the Internet across them, etc.  Up to you!  A Raspberry Pi with wifi and xbnet could provide an Internet gateway for an entire XBee mesh, if you so desire.

This works by creating a virtual network device in Linux, called a "tun" device.  Traffic going out that device will be routed onto XBee, and traffic coming in will be routed to the computer.

To make this work, you will first bring up the interface with xbnet.  Then, give it an IP address with ifconfig or ipaddr.  Do the same on the remote end, and boom, you can ping!

Note that for this mode, xbnet must be run as root (or granted `CAP_NET_ADMIN`).

Here's an example.  Start on machine A:

```
sudo xbnet /dev/ttyUSB3 tun
```

Wait until it tells you what interface it created.  By default, this will be **xbnet0**.  Now run:

```
sudo ip addr add 192.168.3.3/24 dev xbnet0
sudo ip link set dev xbnet0 up
```

If you don't have the **ip** program, you can use the older-style **ifconfig** instead.  This one command does the same as the two newer-style ones above:

```
sudo ifconfig xbnet0 192.168.3.3 netmask 255.255.255.0 
```

Now, on machine B, start xbnet the same as on machine A.  Give it a different IP

```
sudo ip addr add 192.168.3.4/24 dev xbnet0
sudo ip link set dev xbnet0 up
```

Now you can ping from A to B:

```
ping 192.168.3.4
PING 192.168.3.4 (192.168.3.4) 56(84) bytes of data.
64 bytes from 192.168.3.4: icmp_seq=1 ttl=64 time=130 ms
64 bytes from 192.168.3.4: icmp_seq=2 ttl=64 time=89.1 ms
64 bytes from 192.168.3.4: icmp_seq=3 ttl=64 time=81.6 ms
```

For more details, see the tun command below.

## ETHERNET MODE WITH TAP

The tap mode is similar to the tun mode, except it simulates a full Ethernet connection.  You might want this if you need to run a non-IP protocol, or if you want to do something like bridge two Ethernet segments.  The configuration is very similar.

Be aware that a lot of programs generate broadcasts across an Ethernet interface, and bridging will do even more.  It would be easy to overwhelm your XBee network with this kind of cruft, so the tun mode is recommended unless you have a specific need for tap.

## TRANSPARENT MODE

XBee systems have a "transparent mode" in which you can configure a particular destination and use them as a raw serial port.  You should definitely consider if this meets your needs for serial-based protocols; it would eliminate xbnet from the path entirely.

However, you may still wish to use xbnet; perhaps for its debugging.  Also there are some scenarios (such at TCP/IP with multiple destinations) that really cannot be done in transparent mode -- and that is what xbnet is for, and where it shines.

## SOCAT

The **socat**(1) program can be particularly helpful; it can gateway TCP
ports and various other sorts of things into **xbnet**.  This is
helpful if the **xbnet** system is across a network from the system
you wish to run an application on.  **ssh**(1) can also be useful for
this purpose.

A basic command might be like this:

```
socat TCP-LISTEN:12345 EXEC:'xbnet /dev/ttyUSB0 pipe --dest=1234,pty,rawer'
```

Some systems might require disabling buffering in some situations, or
using a pty.  In those instances, something like this may be in order:

```
socat TCP-LISTEN:10104 EXEC:'stdbuf -i0 -o0 -e0 xbnet /dev/ttyUSB4 pipe --dest=1234,pty,rawer'
```

You can send a file this way; for instance, on one end:

```
socet 'EXEC:sz -vv -b /bin/sh,pipes' EXEC:xbnet /dev/ttyUSB4 pipe --dest 1234,nofork,pipes'
```

And on the other, you use `rz` instead of `sz`.

## UUCP

For UUCP, I recommend protocol `i` with the default window-size
setting.  Use as large of a packet size as you can; for slow links,
perhaps 32, up to around 244 for fast, high-quality links.

Protocol `g` (or `G` with a smaller packet size) can also work, but
won't work as well.

Make sure to specify `half-duplex true` in `/etc/uucp/port`.

Here is an example of settings in `sys`:
```
protocol i
protocol-parameter i packet-size 90
protocol-parameter i timeout 30
chat-timeout 60
```

Note that UUCP protocol i adds 10 bytes of overhead per packet and xbnet adds 1 byte of overhead, so
this is designed to work with the default recommended packet size of
255.

Then in `/etc/uucp/port`:

```
half-duplex true
reliable false
```

## YMODEM (and generic example of bidirectional pipe)

ZModem makes a good fit for the higher bitrate XBee modules.  For the slower settings, consider YModem; its 128-byte block size may be more suitable for very slow links than ZModem's 1K.

Here's an example
of how to make it work.  Let's say we want to transmit /bin/true over
the radio.  We could run this:

```
socat EXEC:'sz --ymodem /bin/true' EXEC:'xbnet /dev/ttyUSB0 pipe --dest=1234,pty,rawer'
```

And on the receiving end:

```
socat EXEC:'rz --ymodem' EXEC:'xbnet /dev/ttyUSB0 pipe --dest=5678,pty,rawer'
```

This approach can also be used with many other programs.  For
instance, `uucico -l` for UUCP logins.

## KERMIT

Using the C-kermit distribution (**apt-get install ckermit**), you can
configure for **xbnet** like this:

```
set duplex half
set window 2
set receive timeout 10
set send timeout 10
```

Then, on one side, run:

```
pipe xbnet /dev/ttyUSB0 pipe --dest=1234
Ctrl-\ c
server
```

And on the other:

```
pipe xbnet /dev/ttyUSB0 pipe --dest=5678
Ctrl-\ c
```

Now you can do things like `rdir` (to see ls from the remote), `get`,
`put`, etc.

## DEBUGGING WITH CU

To interact directly with the modem, something like this will work:

```
cu -h --line /dev/ttyUSB0 -s 9600 -e -o  --nostop
```

# RUNNING TCP/IP OVER XBEE WITH PPP

PPP is the fastest way to run TCP/IP over XBee with **xbnet** if you only need to have two nodes talk to each other.  PPP can work in transparent mode without xbnet as well.  It
is subject to a few limitations:

- PPP cannot support
  ad-hoc communication to multiple devices.  It is strictly point-to-point between two devices.
- PPP compression should not be turned on.  This is because PPP
  normally assumes a lossless connection, and any dropped packets
  become rather expensive for PPP to handle, since compression has to
  be re-set.  Better to use compression at the protocol level; for
  instance, with **ssh -C**.
  
To set up PPP, on one device, create /etc/ppp/peers/xbee with this
content:

```
hide-password 
noauth
debug
nodefaultroute
192.168.2.3:192.168.2.2 
mru 1024
passive
115200
nobsdcomp
nodeflate
```

On the other device, swap the order of those IP addresses.

Now, fire it up on each end with a command like this:

```
socat EXEC:'pppd nodetach file /etc/ppp/peers/lora,pty,rawer' \
  EXEC:'xbnet --initfile=init-fast.txt /dev/ttyUSB0 pipe --dest=1234,pty,rawer'
```

According to the PPP docs, an MRU of 296 might be suitable for slower
links.

This will now permit you to ping across the link.  Additional options
can be added to add, for instance, a bit of authentication at the
start and so forth (though note that XBee, being RF, means that a
session could be hijacked, so don't put a lot of stock in this as a
limit; best to add firewall rules, etc.)

Of course, ssh can nicely run over this, but for more versatility, consider the tap or tun options.

# PERFORMANCE TUNING

Here are some tips to improve performance:

## DISABLING XBEE ACKS

By default, the XBee system requests an acknowledgment from the remote node.  The XBee firmware will automatically attempt retransmits if they don't get an ACK in the expected timeframe.  Although higher-level protocols also will do ACK and retransmit, they don't have the XBee level of knowledge of the link layer timing and so XBee may be able to detect and correct for a missing packet much quicker.

However, sometimes all these ACKs can cause significant degredation in performance.  Whether or not they do for you will depend on your network topology and usage patterns; you probably should just try it both ways.  Use **disable-xbee-acks** to disable the XBee level ACKs on messages sent from a given node and see what it does.

## PROTOCOL SELECTION

If all you really need is point-to-point, then consider using PPP rather than tun.  PPP supports header compression which may reduce the TCP/IP overhead significantly.

## PACKET SIZE

Bear in mind the underlying packet size.  For low-overhead protocols, you might want to use a packet size less than the XBee packet size.  For high-overhead protocols such as TCP, you may find that using large packet sizes and letting **xbnet** do fragmentation gives much better performance on clean links, especially at the lower XBee bitrates.

## SERIAL COMMUNICATION SPEED

By defualt, XBee modules communicate at 9600bps.  You should change this and write the updated setting to the module, and give it to xbnet with **--serial-speed**.

# TROUBLESHOOTING

## BROADCAST ISSUES

# SECURITY

xbnet is a low-level tool and should not be considered secure on its own.  The **xbnet pipe** command, for instance, will display information from any node on your mesh.  Here are some tips:

Of course, begin by securing things at the XBee layer.  Enable encryption and passwords for remote AT commands in XBee.

If you are running a network protocol across XBee, enable firewalls at every node on the network.  Remember, joining a node to a networked mesh is like giving it a port on your switch!  Consider how nodes can talk to each other.

Use encryption and authentication at the application layer as well.  ssh or gpg would be a fantastic choice here.

For nodes that are using xbnet to access the Internet, consider not giving them direct Internet access, but rather requiring them to access via something like OpenVPN or SSH forwarding.

# INSTALLATION

**xbnet** is a Rust program and can be built by running **`cargo
build --release`**.  The executable will then be placed in
**target/release/xbnet**. Rust can be easily installed from
<https://www.rust-lang.org/>. 

# INVOCATION

Every invocation of **xbnet** requires at least the name of a
serial port (for instance, **/dev/ttyUSB0**) and a subcommand to run.

# GLOBAL OPTIONS

These options may be specified for any command, and must be given
before the port and command on the command line.

**-d**, **--debug**
:  Activate debug mode.  Details of program operation will be sent to
   stderr.
   
**-h**, **--help**
:  Display brief help on program operation.

**--disable-xbee-acks**
:  Disable the XBee protocol-level acknowledgments of transmitted packets.  This may improve, or hurt, performance; see the conversation under the PERFORMANCE TUNING section.

**--initfile** *FILE*
:  A file listing commands to send to the radio to initialize it.  Each command must yield an `OK` result from the radio.  After running these commands, **xbnet** will issue additional commands to ensure the radio is in the operating mode required by **xbnet**.  Enable **--debug** to see all initialization activity.
   
**--request-xbee-tx-reports**
:  The XBee firmware can return back a report about the success or failure of a transmission.  **xbnet** has no use for these reports, though they are displayed for you if **--debug** is given.  By default, **xbnet** suppresses the generation of these reports.  If you give this option and **--debug**, then you can see them.

**--serial-speed** *SPEED*
:  Communicate with the XBee module at the given serial speed, given in bits per second (baud rate).  If not given, defaults to 9600, which is the Digi default for the XBee modules.  You can change this default with XBee commands and save the new default persistently to the board.  It is strongly recommended that you do so, because many XBee modules can communicate much faster than 9600bps.

**-V**, **--version**
:  Display the version number of **xbnet**.

*PORT*
:  The name of the serial port to which the radio is attached.

*COMMAND*
:  The subcommand which will be executed.

# SUBCOMMANDS

## xbnet ... pipe

The **pipe** subcommand permits piping data between radios.  It requires a **--dest** parameter, which gives the hex MAC address of the recipient of data sent to xbnet's stdin.  pipe is described extensively above.

Note that **--dest** will not restrict the devices that xbnet will receive data from.

## xbnet ... ping

The **ping** subcommand will transmit a simple line of text every 5
seconds including an increasing counter.  It can be displayed at the
other end with **xbnet ... pipe** or reflected with **xbnet
... pong**.  Like **pipe**, it requires a destination MAC address. 

## xbnet ... pong

The **pong** subcommand receives packets and crafts a reply.  It is
intended to be used with **xbnet ... ping**. 

## xbnet ... tun & tap

These commands run a network stack across XBee and are described extensively above.  They have several optional parameters:

**--broadcast-everything** (tun and tap)
:  Normally, **xbnet** will use unicast (directed) transmissions to remotes where it knows their XBee MAC address.  This is more efficient on the XBee network.  However, in some cases you may simply want it to use broadcast packets for all transmissions, and this accomplishes that.

**--broadcast-unknown** (tap only)
:  Normally, **xbnet** will drop Ethernet frames destined for MAC addresses that it hasn't seen.  (Broadcast packets still go out.)  This is suitable for most situations.  However, you can also have it broadcast all packets do unknown MAC addresses.  This can be useful in some obscure situations such as multicast.

**--disable-ipv4** and **disable-ipv6** (tun only)
:  Disable all relaying of either IPv4 or IPv6 packets.  This is not valid in tap mode because tap doesn't operate at this protocol level.  It is recommended you disable protocols you don't use.

**--iface-name** *NAME* (tun and tap)
:  Request a specific name for the tun or tap interface.  By default, this requests **xbnet%d**.  The kernel replaces **%d** with an integer starting at 0, finding an unused interface.  It can be useful to specify an explicit interface here for use in scripts.

**--max-ip-cache** *SECONDS* (tun only)
:  Specifies how long it caches the XBee MAC address for a given IP address.  After this many seconds without receiving a packet from the given IP address, **xbnet** will send the next packet to the IP as a broadcast and then cache the result.  The only reason to expire IPs from the cache is if you re-provision them on other devices.  The tap mode doesn't have a timed cache, since the OS will re-ARP (generating a broadcast anyhow) if it fails to communicate with a given IP.


# AUTHOR

John Goerzen <jgoerzen@complete.org>

# COPYRIGHT AND LICENSE

Copyright (C) 2019-2020  John Goerzen <jgoerzen@complete.org>

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
