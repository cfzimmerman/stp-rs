use anyhow::bail;
use pnet::{
    datalink::{
        self, Channel::Ethernet, Config, DataLinkReceiver, DataLinkSender, NetworkInterface,
    },
    packet::{
        ethernet::{EthernetPacket, MutableEthernetPacket},
        MutablePacket, Packet,
    },
};
use std::{
    io::ErrorKind,
    thread::sleep,
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq, Eq)]
enum EthState {
    Block,
    Forward,
    // Listen
    // Learn
}

struct EthPort {
    intf: NetworkInterface,
    rx: Box<dyn DataLinkReceiver>,
    state: EthState,
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
        let Ok(Ethernet(tx, rx)) = datalink::channel(&intf, Config::default()) else {
            bail!("Failed to parse ethernet channel on interface: {:#?}", intf);
        };
        Ok((
            Self {
                intf,
                rx,
                state: EthState::Forward,
            },
            tx,
        ))
    }
}

struct EthRouter;

impl EthRouter {
    pub fn run(poll_timeout: Option<Duration>) -> anyhow::Result<()> {
        // filters out all ethernet interfaces that don't have mininet names
        let interfaces = datalink::interfaces();
        let mut ingress = Vec::with_capacity(interfaces.len());
        let mut egress = Vec::with_capacity(interfaces.len());

        for intf in datalink::interfaces()
            .into_iter()
            .filter(|intf| intf.name.contains("-eth"))
        {
            let (port, tx) = EthPort::build(intf, poll_timeout)?;
            ingress.push(port);
            egress.push(tx);
        }

        loop {
            for (portnum_in, port) in ingress.iter_mut().enumerate() {
                let EthState::Forward = port.state else {
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
