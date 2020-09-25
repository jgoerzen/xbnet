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

PPP is the fastest way to run TCP/IP over XBee with **xbnet** - and can work in transparent mode without xbnet as well.  It
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

## OPTIMIZING TCP/IP OVER LORA

It should be noted that a TCP ACK encapsulated in AX.25 takes 69 bytes
to transmit -- that's a header with no data, and it's 69 bytes!  This
is a significant overhead.  It can be dramatically reduced by using a
larger packet size; for instance, in /etc/ax25/axports, thange the
packet length of 70 to 1024.  This will now cause the
**--maxpacketsize** option to take precedence and fragment the TCP/IP
packets for transmission over XBee; they will, of course, be
reassembled on the other end.  Setting **--txslot 2000** or a similar
value will also be helpful in causing TCP ACKs to reach the remote end
quicker, hopefully before timeouts expire.  **--pack** may also
produce some marginal benefit.

I have been using:

```
xbnet --initfile=init-fast.txt --txslot 2000 --pack --debug --maxpacketsize 200 --txwait 150
```

with success on a very clean (reasonably error-free) link.

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

**--readqual**
:  Attempt to read and log information about the RF quality of
   incoming packets after each successful packet received.  There are
   some corner cases where this is not possible.  The details will be
   logged with **xbnet**'s logging facility, and are therefore only
   visible if **--debug** is also used.

**--pack**
:  Attempt to pack as many bytes into each transmitted frame as
   possible.  Ordinarily, the **pipe** and **kiss** commands attempt
   -- though do not guarantee -- to preserve original framing from the
   operating system.  With **--pack**, instead the effort is made to
   absolutely minimize the number of transmitted frames by putting as
   much data as possible into each.

**-V**, **--version**
:  Display the version number of **xbnet**.

**--eotwait** *TIME*
:  The amount of time in milliseconds to wait after receiving a packet
   that indicates more are coming before giving up on receiving an
   additional packet and proceeding to transmit.  Ideally this would
   be at least the amount of time it takes to transmit 2 packets.
   Default: 1000.
   
**--initfile** *FILE*
:  A file listing commands to send to the radio to initialize it.
   If not given, a default set will be used.
   
**--txwait** *TIME*
:  Amount of time in milliseconds to pause before transmitting each
   packet.  Due to processing delays on the receiving end, packets
   cannot be transmitted immediately back to back.  Increase this if
   you are seeing frequent receive errors for back-to-back packets,
   which may be indicative of a late listen.  Experimentation has
   shown that a value of 120 is needed for very large packets, and is
   the default.  You may be able to use 50ms or less if you are
   sending small packets.  In my testing, with 100-byte packets, 
   a txwait of 50 was generally sufficient.

**--txslot** TIME**
:  The maximum of time in milliseconds for one end of the conversation
   to continue transmitting without switching to receive mode.  This
   is useful for protocols such as TCP that expect periodic ACKs and
   get perturbed when they are not delivered in a timely manner.  If
   **--txslot** is given, then after the given number of milliseconds
   have elapsed, the next packet transmitted will signal to the other
   end that it should take a turn.  If the transmitter has more data
   to send, it is sent with a special flag of 2 to request the other
   end to immediately send back a frame - data if it has some, or a "I
   don't have anything, continue" frame otherwise.  After transmitting
   flag 2, it will wait up to **txwait** seconds for the first packet
   from the other end before continuing to transmit.  This setting is
   not suitable when more than 2 radios are on-frequency.  Setting
   txslot also enables responses to flag 2.  The default is 0, which
   disables the txslot feature and is suitable for uses which do not
   expect ACKs.

**--maxpacketsize** *BYTES*
:  The maximum frame size, in the range of 10 - 250.  The actual frame
   transmitted over the air will be one byte larger due to
   **xbnet** collision mitigation as described above.
   Experimentation myself, and reports from others, suggests that XBee
   works best when this is 100 or less.

*PORT*
:  The name of the serial port to which the radio is attached.

*COMMAND*
:  The subcommand which will be executed.

# SUBCOMMANDS

## xbnet ... pipe

The **pipe** subcommand is the main workhorse of the application and
is described extensively above.

## xbnet ... ping

The **ping** subcommand will transmit a simple line of text every 10
seconds including an increasing counter.  It can be displayed at the
other end with **xbnet ... pipe** or reflected with **xbnet
... pong**.

## xbnet ... pong

The **pong** subcommand receives packets and crafts a reply.  It is
intended to be used with **xbnet ... ping**.  Its replies include
the signal quality SNR and RSSI if available.

# AUTHOR

John Goerzen <jgoerzen@complete.org>

# SEE ALSO

I wrote an
[introduction](https://changelog.complete.org/archives/10042-long-range-radios-a-perfect-match-for-unix-protocols-from-the-70s)
and a [follow-up about
TCP/IP](https://changelog.complete.org/archives/10048-tcp-ip-over-lora-radios)
on my blog.

# COPYRIGHT AND LICENSE

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
