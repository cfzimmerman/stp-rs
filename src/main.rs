use anyhow::bail;
use bytemuck::{Pod, Zeroable};
use pnet::{
    datalink::{
        self, Channel::Ethernet, Config, DataLinkReceiver, DataLinkSender, NetworkInterface,
    },
    packet::{
        ethernet::{EthernetPacket, MutableEthernetPacket},
        Packet,
    },
    util::MacAddr,
};
use std::{collections::HashMap, io::ErrorKind, mem, time::Duration};

/// This is a reserved mac address often used for layer 2 protocols like STP
/// https://notes.networklessons.com/stp-bpdu-destination-mac-address
const BPDU_MAC: MacAddr = MacAddr(0x01, 0x80, 0xc2, 0x0, 0x0, 0x0);

#[derive(Debug, PartialEq, Eq)]
enum PortState {
    /// The port is the switch's path to the root. All traffic is served.
    Root,
    /// This port is part of a loop. Only BPDU packets are accepted.
    Block,
    /// This port services other nodes' access to the root. All traffic is served.
    Forward,
}

/// A bridge protocol data unit packet. Note, this is not authentic. I'm
/// choosing a subset of fields and using aligned data types instead of
/// protocol field sizes. This is simply for ease of implementation.
/// Assumes packets are unversioned and for spanning tree.
/// https://support.huawei.com/enterprise/en/doc/EDOC1000178168/e99e1364/bpdu-format
#[repr(C)]
#[derive(Pod, Zeroable, Copy, Clone)]
struct Bpdu {
    root_cost: u8,
    root_id: [u8; 6],
    bridge_id: [u8; 6],
}

/// A wrapper over a buffer used to construct Bpdu packets. All Bpdu
/// packets have the same size and are sent one at a time, so this is
/// just a nice way to reuse a single allocation for all packet construction.
struct BpduBuf(pub Vec<u8>);

impl Bpdu {
    /// Builds a new bpdu type, casting MacAddresses into raw octets to satisfy bytemuck.
    pub fn new(root_cost: u8, root_id: MacAddr, bridge_id: MacAddr) -> (Self, BpduBuf) {
        (
            Bpdu {
                root_cost,
                root_id: root_id.octets(),
                bridge_id: bridge_id.octets(),
            },
            BpduBuf(vec![
                0;
                EthernetPacket::minimum_packet_size()
                    + mem::size_of::<Bpdu>()
            ]),
        )
    }

    pub fn get_cost(&self) -> u8 {
        self.root_cost
    }

    pub fn get_root_id(&self) -> MacAddr {
        self.root_id.into()
    }

    pub fn get_bridge_id(&self) -> MacAddr {
        self.bridge_id.into()
    }

    /// Makes a bpdu ethernet packet in the given bpdu_buf.
    pub fn make_packet<'a>(
        &self,
        bpdu_buf: &'a mut BpduBuf,
        src_mac: MacAddr,
    ) -> MutableEthernetPacket<'a> {
        let mut pkt = MutableEthernetPacket::new(&mut bpdu_buf.0)
            .expect("Bpdu packet size should be constant, and the buf should always accomodate what's needed");

        pkt.set_payload(bytemuck::bytes_of(self));
        pkt.set_source(src_mac);
        pkt.set_destination(BPDU_MAC);
        pkt
    }
}

struct EthPort {
    mac: MacAddr,
    tx: Box<dyn DataLinkSender>,
    state: PortState,
}

impl EthPort {
    /// Builds an abstraction that supports sending and receiving network packets from
    /// an ethernet port. Receive blocks until a packet arries or `poll_timeout` has elapsed.
    pub fn build(
        intf: NetworkInterface,
        poll_timeout: Option<Duration>,
    ) -> anyhow::Result<(Self, Box<dyn DataLinkReceiver>)> {
        let mut port_cfg = Config::default();
        port_cfg.read_timeout = poll_timeout;
        let Ok(Ethernet(tx, rx)) = datalink::channel(&intf, port_cfg) else {
            bail!("Failed to parse ethernet channel on interface: {:#?}", intf);
        };
        let Some(mac) = intf.mac else {
            bail!("Cannot create an eth port without a mac address");
        };
        Ok((
            Self {
                mac,
                state: PortState::Forward,
                tx,
            },
            rx,
        ))
    }

    /// Returns whether a packet is marked for the purpose of ethernet routing
    /// Panics if the packet matches the BPDU mac address but cannot be serialized.
    /// Such a case indicates a bug or some serious misunderstanding of the network.
    pub fn try_routing<'a>(pkt: &'a EthernetPacket) -> Option<&'a Bpdu> {
        if BPDU_MAC != pkt.get_destination() {
            return None;
        };
        Some(bytemuck::from_bytes(pkt.payload()))
    }
}

struct EthRouter {
    ports: Vec<EthPort>,
    inbound: Vec<Box<dyn DataLinkReceiver>>,
    port_id: MacAddr,
    curr_bpdu: Bpdu,
    bpdu_buf: BpduBuf,
    /// MacAddr is the destination mac, and the value usize is an
    /// index into the egress table.
    fwd_table: HashMap<MacAddr, usize>,
}

impl EthRouter {
    /// Queries ethernet interfaces and opens read/write connections with all
    /// mininet ports. Assigns a mac address to represent the whole switch and
    /// establishes an initial Bpdu for this switch.
    pub fn build(poll_timeout: Option<Duration>) -> anyhow::Result<Self> {
        let interfaces = datalink::interfaces();
        let mut ports = Vec::with_capacity(interfaces.len());
        let mut inbound = Vec::with_capacity(interfaces.len());
        let mut port_id = MacAddr::broadcast();

        // Note: Port egress and ingress are separated because simultanous
        // borrows to both the tx and rx are almost always needed. That supports
        // no data copying except from the ethernet receive buffer into the send buffer.

        // filters out all ethernet interfaces that don't have mininet names
        for intf in datalink::interfaces()
            .into_iter()
            .filter(|intf| intf.name.contains("-eth"))
        {
            let (port, port_rx) = EthPort::build(intf, poll_timeout)?;
            port_id = port_id.min(port.mac);
            ports.push(port);
            inbound.push(port_rx);
        }

        if port_id == MacAddr::broadcast() {
            bail!("Failed to identify any viable interfaces for this switch");
        }
        let (curr_bpdu, bpdu_buf) = Bpdu::new(0, port_id, port_id);

        Ok(EthRouter {
            ports,
            inbound,
            port_id,
            curr_bpdu,
            bpdu_buf,
            fwd_table: HashMap::new(),
        })
    }

    /// Sends the packet to the given outbound transmitter.
    /// The given packet is copied directly into the send buffer.
    fn send(tx: &mut Box<dyn DataLinkSender>, pkt: &EthernetPacket) {
        tx.build_and_send(1, pkt.packet().len(), &mut |outbound| {
            outbound.clone_from_slice(pkt.packet());
        });
    }

    /// Forwards client packets (not control) using the forwarding table.
    /// Learns source/port pairs when possible.
    fn fwd_client(&mut self, portnum_in: usize, eth_pkt: &EthernetPacket) {
        if self.ports[portnum_in].state == PortState::Block {
            // deny client packets from blocked ports.
            eprintln!("Denied client packet on a blocked port: {:#?}", eth_pkt);
            return;
        };

        // self learning
        *self.fwd_table.entry(eth_pkt.get_source()).or_default() = portnum_in;

        // forward to known destination
        if let Some(next_hop) = self.fwd_table.get(&eth_pkt.get_destination()) {
            let port = &mut self.ports[*next_hop];
            assert_ne!(
                port.state,
                PortState::Block,
                "The forwarding table shouldn't suggest blocked ports."
            );
            Self::send(&mut port.tx, &eth_pkt);
            return;
        }

        // flood to unknown destination
        for (portnum_out, port) in self.ports.iter_mut().enumerate() {
            if portnum_out == portnum_in {
                continue;
            }
            Self::send(&mut port.tx, eth_pkt);
        }
    }

    /// Runs packet control and forwarding as long as the network is live.
    ///
    /// There were two accessible ways to implement this given the constraints of
    /// the pnet channel: (1) spawn a thread for each port and send
    /// messages to a central handler via channel, or (2) poll ethernet
    /// ports in a busy loop.
    /// I'd do (1) if running a single process. However, I need to be able to
    /// run +16 switches on a single emulated network on qemu on a macbook. There
    /// will be zero free cores no matter what, so a busy loop actually seems
    /// more efficient than multithreading + blocking in this situation.
    pub fn run(mut self) -> anyhow::Result<()> {
        let mut inbound = mem::take(&mut self.inbound);
        assert_eq!(inbound.len(), self.ports.len());
        loop {
            for (portnum_in, rx) in inbound.iter_mut().enumerate() {
                let bytes = match rx.next() {
                    Ok(p) => p,
                    Err(e) => {
                        if e.kind() == ErrorKind::TimedOut {
                            continue;
                        }
                        bail!("Exiting on io error: {:#?}", e);
                    }
                };
                let Some(eth_pkt) = EthernetPacket::new(bytes) else {
                    eprintln!("Failed to parse packet: {:#?}", bytes);
                    continue;
                };

                let Some(upd) = EthPort::try_routing(&eth_pkt) else {
                    self.fwd_client(portnum_in, &eth_pkt);
                    continue;
                };

                // TODO: Handle routing
            }
            /*
             */
        }
    }
    /*

            for (portnum_out, outbound) in self.egress.iter_mut().enumerate() {
                if portnum_in == portnum_out {
                    continue;
                }

                outbound.build_and_send(1, eth_pkt.packet().len(), &mut |outbound| {
                    let mut outbound = MutableEthernetPacket::new(outbound)
                        .expect("MutableEthernetPacket must construct successfully");
                    outbound.clone_from(&eth_pkt);
                });
            }


    loop {
        for (portnum_in, port) in ingress.iter_mut().enumerate() {
            let bytes = match port.rx.next() {
                Ok(p) => p,
                Err(e) => {
                    if e.kind() == ErrorKind::TimedOut {
                        continue;
                    }
                    bail!("Exiting on io error: {:#?}", e);
                }
            };

            let Some(eth_pkt) = EthernetPacket::new(bytes) else {
                eprintln!("Failed to parse packet: {:#?}", bytes);
                continue;
            };

            for (portnum_out, outbound) in egress.iter_mut().enumerate() {
                if portnum_in == portnum_out {
                    continue;
                }

                outbound.build_and_send(1, eth_pkt.packet().len(), &mut |outbound| {
                    let mut outbound = MutableEthernetPacket::new(outbound)
                        .expect("MutableEthernetPacket must construct successfully");
                    outbound.clone_from(&eth_pkt);
                });
            }
        }
    }
    */
}

fn main() -> anyhow::Result<()> {
    EthRouter::build(Some(Duration::from_micros(1000)))?;
    Ok(())
}
