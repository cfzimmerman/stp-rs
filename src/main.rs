use anyhow::bail;
use pnet::{
    datalink::{
        self, Channel::Ethernet, Config, DataLinkReceiver, DataLinkSender, NetworkInterface,
    },
    packet::{
        ethernet::{EthernetPacket, MutableEthernetPacket},
        MutablePacket, Packet,
    },
    util::MacAddr,
};
use std::{io::ErrorKind, time::Duration};

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
#[derive(bytemuck::AnyBitPattern, Copy, Clone)]
struct Bpdu {
    root_cost: u8,
    root_id: [u8; 6],
    bridge_id: [u8; 6],
}

impl Bpdu {
    /// Builds a new bpdu type, casting MacAddresses into raw octets to satisfy bytemuck.
    pub fn new(root_cost: u8, root_id: MacAddr, bridge_id: MacAddr) -> Self {
        Bpdu {
            root_cost,
            root_id: root_id.octets(),
            bridge_id: bridge_id.octets(),
        }
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
}

struct EthPort {
    intf: NetworkInterface,
    rx: Box<dyn DataLinkReceiver>,
    state: PortState,
}

impl EthPort {
    /// Builds an abstraction that supports sending and receiving network packets from
    /// an ethernet port. Receive blocks until a packet arries or `poll_timeout` has elapsed.
    pub fn build(
        intf: NetworkInterface,
        poll_timeout: Option<Duration>,
    ) -> anyhow::Result<(Self, Box<dyn DataLinkSender>)> {
        let mut port_cfg = Config::default();
        port_cfg.read_timeout = poll_timeout;
        let Ok(Ethernet(tx, rx)) = datalink::channel(&intf, port_cfg) else {
            bail!("Failed to parse ethernet channel on interface: {:#?}", intf);
        };
        Ok((
            Self {
                intf,
                rx,
                state: PortState::Forward,
            },
            tx,
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

struct EthRouter;

impl EthRouter {
    pub fn run(poll_timeout: Option<Duration>) -> anyhow::Result<()> {
        let interfaces = datalink::interfaces();
        let mut ingress = Vec::with_capacity(interfaces.len());
        let mut egress = Vec::with_capacity(interfaces.len());

        // filters out all ethernet interfaces that don't have mininet names
        for intf in datalink::interfaces()
            .into_iter()
            .filter(|intf| intf.name.contains("-eth"))
        {
            println!("{:#?}", intf);
            let (port, tx) = EthPort::build(intf, poll_timeout)?;
            ingress.push(port);
            egress.push(tx);
        }

        loop {
            // There were two accessible ways to run this given the constraints of
            // the pnet channel: (1) spawn a thread for each port and send
            // messages to a central handler via channel, or (2) poll ethernet
            // ports in a busy loop.
            // I'd do (1) if running a single process. However, I need to be able to
            // run +16 switches on a single emulated network on qemu on a macbook. There
            // will be zero free cores no matter what, so a busy loop actually seems
            // more efficient than multithreading + blocking in this situation.
            for (portnum_in, port) in ingress.iter_mut().enumerate() {
                let PortState::Forward = port.state else {
                    continue;
                };

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
    }
}

fn main() -> anyhow::Result<()> {
    EthRouter::run(Some(Duration::from_micros(1000)))?;
    Ok(())
}
