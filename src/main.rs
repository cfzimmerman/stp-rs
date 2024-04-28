use std::time::Duration;

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

struct EthPort {
    intf: NetworkInterface,
    tx: Box<dyn DataLinkSender>,
    rx: Box<dyn DataLinkReceiver>,
}

impl EthPort {
    /// Builds an abstraction that supports sending and receiving network packets from
    /// an ethernet port. Receive blocks until a packet arries or `poll_timeout` has elapsed.
    pub fn build(intf: NetworkInterface, poll_timeout: Option<Duration>) -> anyhow::Result<Self> {
        let mut port_cfg = Config::default();
        port_cfg.read_timeout = poll_timeout;
        let Ok(Ethernet(tx, rx)) = datalink::channel(&intf, Config::default()) else {
            bail!("Failed to parse ethernet channel on interface: {:#?}", intf);
        };
        Ok(Self { intf, tx, rx })
    }
}

struct EthRouter {
    ports: Vec<EthPort>,
}

impl EthRouter {
    pub fn build(poll_timeout: Option<Duration>) -> anyhow::Result<Self> {
        // filters out all ethernet interfaces that don't have mininet names
        let ports = datalink::interfaces()
            .into_iter()
            .filter(|intf| intf.name.contains("-eth"))
            .map(|intf| EthPort::build(intf, poll_timeout))
            .collect::<anyhow::Result<Vec<EthPort>>>()?;

        /*
        let Ok(Ethernet(_i1_tx, mut i1_rx)) = datalink::channel(&mn_intf[0], Config::default())
        else {

        };

        let Ok(Ethernet(mut i2_tx, _i2_rx)) = datalink::channel(&mn_intf[1], Config::default())
        else {
            bail!(
                "Failed to parse ethernet channel on interface: {:#?}",
                mn_intf[1]
            );
        };

        println!("Entering packet loop");

        while let Ok(i1_pkt) = i1_rx.next() {
            let Some(eth_pkt) = EthernetPacket::new(i1_pkt) else {
                eprintln!("Failed to parse packet: {:#?}", i1_pkt);
                continue;
            };
            println!("received packet: {:#?}", eth_pkt);

            i2_tx.build_and_send(1, eth_pkt.packet().len(), &mut |outbound| {
                let mut outbound = MutableEthernetPacket::new(outbound)
                    .expect("MutableEthernetPacket must construct successfully");
                outbound.clone_from(&eth_pkt);
            });
        }

        */
        Ok(EthRouter { ports })
    }
}

fn main() -> anyhow::Result<()> {
    EthRouter::build(Some(Duration::from_micros(100)))?;
    Ok(())
}
