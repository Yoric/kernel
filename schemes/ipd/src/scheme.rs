use std::cell::RefCell;
use std::collections::VecDeque;
use std::rand;
use std::rc::Rc;
use std::{str, u16};

use netutils::{getcfg, n16, MacAddr, Ipv4Addr, ArpHeader, Arp, Ipv4};
use resource_scheme::ResourceScheme;
use syscall;
use syscall::error::{Error, Result, EACCES, ENOENT, EINVAL};
use syscall::flag::O_RDWR;

use resource::*;

/// The IP address of the localhost.
const LOCALHOST: Ipv4Addr = Ipv4Addr { bytes: [127, 0, 0, 1] };

/// A ARP entry (MAC + IP)
pub struct ArpEntry {
    ip: Ipv4Addr,
    mac: MacAddr,
}

/// A IP scheme
pub struct IpScheme {
    pub arp: RefCell<Vec<ArpEntry>>,

    /// FIFO queue of packets written to the loopback and waiting to be read.
    ///
    /// The data stored contains the exact data that has been added by the client
    /// calling `write()`, without adding any headers.
    ///
    /// This buffer is shared between all loopback connections.
    pub loopback_fifo: Rc<RefCell<VecDeque<Vec<u8>>>>,
}

impl IpScheme {
    pub fn new() -> IpScheme {
        IpScheme {
            arp: RefCell::new(Vec::new()),
            loopback_fifo: Rc::new(RefCell::new(VecDeque::new())),
        }
    }
}

impl ResourceScheme<IpResource> for IpScheme {
    fn open_resource(&self, url: &[u8], _flags: usize, uid: u32, _gid: u32) -> Result<Box<IpResource>> {
        if uid == 0 {
            let mac_addr = MacAddr::from_str(&getcfg("mac").map_err(|err| err.into_sys())?);
            let ip_addr = Ipv4Addr::from_str(&getcfg("ip").map_err(|err| err.into_sys())?);
            let ip_subnet = Ipv4Addr::from_str(&getcfg("ip_subnet").map_err(|err| err.into_sys())?);
            let ip_router = Ipv4Addr::from_str(&getcfg("ip_router").map_err(|err| err.into_sys())?);

            let path = try!(str::from_utf8(url).or(Err(Error::new(EINVAL))));
            let mut parts = path.split('/');
            if let Some(host_string) = parts.next() {
                if let Some(proto_string) = parts.next() {
                    let proto = u8::from_str_radix(proto_string, 16).unwrap_or(0);

                    if ! host_string.is_empty() {
                        let peer_addr = Ipv4Addr::from_str(host_string);
                        let mut route_mac = MacAddr::BROADCAST;

                        if ! peer_addr.equals(Ipv4Addr::BROADCAST) {
                            if peer_addr.equals(LOCALHOST) {
                                return Ok(Box::new(IpResource {
                                    connection: Connection::Loopback {
                                        packets: self.loopback_fifo.clone()
                                    },
                                    host_addr: ip_addr,
                                    peer_addr: peer_addr,
                                    proto: proto,
                                    id: (rand() % 65536) as u16,
                                }));
                            }

                            let mut needs_routing = false;

                            for octet in 0..4 {
                                let me = ip_addr.bytes[octet];
                                let mask = ip_subnet.bytes[octet];
                                let them = peer_addr.bytes[octet];
                                if me & mask != them & mask {
                                    needs_routing = true;
                                    break;
                                }
                            }

                            let route_addr = if needs_routing {
                                ip_router
                            } else {
                                peer_addr
                            };

                            for entry in self.arp.borrow().iter() {
                                if entry.ip.equals(route_addr) {
                                    route_mac = entry.mac;
                                    break;
                                }
                            }

                            if route_mac.equals(MacAddr::BROADCAST) {
                                if let Ok(link) = syscall::open(&format!("ethernet:{}/806", &route_mac.to_string()), O_RDWR) {
                                    let arp = Arp {
                                        header: ArpHeader {
                                            htype: n16::new(1),
                                            ptype: n16::new(0x800),
                                            hlen: 6,
                                            plen: 4,
                                            oper: n16::new(1),
                                            src_mac: mac_addr,
                                            src_ip: ip_addr,
                                            dst_mac: route_mac,
                                            dst_ip: route_addr,
                                        },
                                        data: Vec::new(),
                                    };

                                    match syscall::write(link, &arp.to_bytes()) {
                                        Ok(_) => loop {
                                            let mut bytes = [0; 65536];
                                            match syscall::read(link, &mut bytes) {
                                                Ok(count) => if let Some(packet) = Arp::from_bytes(&bytes[..count]) {
                                                    if packet.header.oper.get() == 2 &&
                                                       packet.header.src_ip.equals(route_addr) {
                                                        route_mac = packet.header.src_mac;
                                                        self.arp.borrow_mut().push(ArpEntry {
                                                            ip: route_addr,
                                                            mac: route_mac,
                                                        });
                                                        break;
                                                    }
                                                },
                                                Err(_) => (),
                                            }
                                        },
                                        Err(err) => println!("IP: ARP Write Failed: {}", err),
                                    }
                                }
                            }
                        }

                        if let Ok(link) = syscall::open(&format!("ethernet:{}/800", &route_mac.to_string()), O_RDWR) {
                            return Ok(Box::new(IpResource {
                                connection: Connection::Device {
                                    link: link,
                                    init_data: Vec::new(),
                                },
                                host_addr: ip_addr,
                                peer_addr: peer_addr,
                                proto: proto,
                                id: (rand() % 65536) as u16,
                            }));
                        }
                    } else {
                        while let Ok(link) = syscall::open("ethernet:/800", O_RDWR) {
                            let mut bytes = [0; 65536];
                            // FIXME: Blocking call?
                            match syscall::read(link, &mut bytes) {
                                Ok(count) => {
                                    if let Some(packet) = Ipv4::from_bytes(&bytes[..count]) {
                                        if packet.header.proto == proto &&
                                           (packet.header.dst.equals(ip_addr) || packet.header.dst.equals(Ipv4Addr::BROADCAST)) {
                                            return Ok(Box::new(IpResource {
                                                connection: Connection::Device {
                                                    link: link,
                                                    init_data: packet.data,
                                                },
                                                host_addr: ip_addr,
                                                peer_addr: packet.header.src,
                                                proto: proto,
                                                id: (rand() % 65536) as u16,
                                            }));
                                        }
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }
                } else {
                    println!("IP: No protocol provided");
                }
            } else {
                println!("IP: No host provided");
            }

            Err(Error::new(ENOENT))
        } else {
            Err(Error::new(EACCES))
        }
    }
}
