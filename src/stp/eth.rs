use super::bpdu::{self, Bpdu, BpduBuf};
use anyhow::bail;
use pnet::{
    datalink::{
        self, Channel::Ethernet, Config, DataLinkReceiver, DataLinkSender, NetworkInterface,
    },
    packet::{ethernet::EthernetPacket, Packet},
    util::MacAddr,
};
use std::{
    cmp::Ordering,
    collections::HashMap,
    io::ErrorKind,
    mem,
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq, Eq)]
enum PortState {
    /// The port is the switch's path to the root. All traffic is served.
    Root,
    /// This port is part of a loop. Only BPDU packets are accepted.
    Block,
    /// This port services other nodes' access to the root. All traffic is served.
    Forward,
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
        if Bpdu::BPDU_MAC != pkt.get_destination() {
            return None;
        };
        Some(bytemuck::from_bytes(pkt.payload()))
    }
}

pub struct EthRouter {
    ports: Vec<EthPort>,
    inbound: Vec<Box<dyn DataLinkReceiver>>,
    switch_id: MacAddr,
    curr_bpdu: Bpdu,
    bpdu_buf: BpduBuf,
    bpdu_resend_timeout: Duration,
    last_resent_bpdu: Instant,
    /// MacAddr is the destination mac, and the value usize is an
    /// index into the egress table.
    fwd_table: HashMap<MacAddr, usize>,
}

impl EthRouter {
    /// Queries ethernet interfaces and opens read/write connections with all
    /// mininet ports. Assigns a mac address to represent the whole switch and
    /// establishes an initial Bpdu for this switch.
    pub fn build(
        bpdu_resend_timeout: Duration,
        eth_poll_timeout: Option<Duration>,
    ) -> anyhow::Result<Self> {
        let interfaces = datalink::interfaces();
        let mut ports = Vec::with_capacity(interfaces.len());
        let mut inbound = Vec::with_capacity(interfaces.len());
        let mut switch_id = MacAddr::broadcast();

        // Note: Port egress and ingress are separated because simultanous
        // borrows to both the tx and rx are almost always needed. That supports
        // no data copying except from the ethernet inflow buffer into
        // the outflow buffer.

        for intf in datalink::interfaces()
            .into_iter()
            .filter(|intf| intf.name.contains("-eth"))
        {
            let (port, port_rx) = EthPort::build(intf, eth_poll_timeout)?;
            switch_id = switch_id.min(port.mac);
            ports.push(port);
            inbound.push(port_rx);
        }

        if switch_id == MacAddr::broadcast() {
            bail!("Failed to identify any viable interfaces for this switch");
        }

        Ok(EthRouter {
            ports,
            inbound,
            switch_id,
            curr_bpdu: Bpdu::new(0, switch_id, switch_id),
            bpdu_buf: Bpdu::make_buf(),
            bpdu_resend_timeout,
            last_resent_bpdu: Instant::now() - bpdu_resend_timeout,
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
        assert_ne!(
            eth_pkt.get_destination(),
            Bpdu::BPDU_MAC,
            "These should only be host to host packets"
        );
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

    /// Makes a control packet with the current bpdu and sends it to all neighbors
    /// (including blocked neighbors).
    fn broadcast_bpdu(&mut self) {
        let pkt = self
            .curr_bpdu
            .make_packet(&mut self.bpdu_buf, self.switch_id);
        for port in &mut self.ports {
            Self::send(&mut port.tx, &pkt);
        }
    }

    /// Blocks the current root port, replacing them with the new root. Marks
    /// the new root as root.
    /// Also overwrites the current bpdu with the neighbor's cost-adjusted bpdu.
    fn reset_root(&mut self, new_root: usize, neighbor: &Bpdu, pkt: &EthernetPacket) {
        for (port_num, port) in self.ports.iter_mut().enumerate() {
            if port_num == new_root {
                port.state = PortState::Root;
                continue;
            }
            if port.state == PortState::Root {
                port.state = PortState::Block;
            }
        }
        self.curr_bpdu = Bpdu::new(neighbor.cost() + 1, neighbor.root_id(), pkt.get_source());
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
            if self.bpdu_resend_timeout < self.last_resent_bpdu.elapsed() {
                println!("Broadcasting bpdu: {:#?}", self.broadcast_bpdu());
                self.broadcast_bpdu();
                self.last_resent_bpdu = Instant::now();
            }

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

                let Some(neighbor) = EthPort::try_routing(&eth_pkt) else {
                    println!("Forwarding data packet");
                    self.fwd_client(portnum_in, &eth_pkt);
                    continue;
                };

                // first take the smaller root id
                // then take the shortest path to the smallest root id

                println!("handling bpdu: {:#?}", neighbor);
                let agree_on_root = match neighbor.root_id().cmp(&self.curr_bpdu.root_id()) {
                    Ordering::Less => {
                        self.reset_root(portnum_in, neighbor, &eth_pkt);
                        self.broadcast_bpdu();
                        println!("Found a better root, broadcasting");
                        continue;
                    }
                    Ordering::Greater => {
                        self.broadcast_bpdu();
                        println!("They sent a greater root, broadcasting");
                        continue;
                    }
                    Ordering::Equal => true,
                };
                assert!(
                    agree_on_root,
                    "The code below only applies to switches that already agree on the root"
                );

                match neighbor.cost().cmp(&(self.curr_bpdu.cost() + 1)) {
                    Ordering::Less => {
                        println!("Got a lower cost path, taking it");
                        self.reset_root(portnum_in, neighbor, &eth_pkt);
                        self.broadcast_bpdu();
                    }
                    Ordering::Equal => {
                        println!("Got an equal cost path, blocking them");
                        self.ports[portnum_in].state = PortState::Block;
                    }
                    Ordering::Greater => {
                        self.ports[portnum_in].state = if neighbor.bridge_id() == self.switch_id {
                            println!("Got a greater cost path, forwarding");
                            PortState::Forward
                        } else {
                            println!("Got a greater cost path, blocking");
                            PortState::Block
                        };
                    }
                };
            }
        }
    }
}
