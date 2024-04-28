use anyhow::bail;
use pnet::{
    datalink::{self, Channel::Ethernet, Config, NetworkInterface},
    packet::{
        ethernet::{EthernetPacket, MutableEthernetPacket},
        MutablePacket, Packet,
    },
};

struct EthRouter {}

impl EthRouter {
    pub fn build() -> anyhow::Result<Self> {
        // filters out all ethernet interfaces that don't have mininet names
        let mn_intf: Vec<NetworkInterface> = datalink::interfaces()
            .into_iter()
            .filter(|intf| intf.name.contains("-eth"))
            .collect();

        let Ok(Ethernet(_i1_tx, mut i1_rx)) = datalink::channel(&mn_intf[0], Config::default())
        else {
            bail!(
                "Failed to parse ethernet channel on interface: {:#?}",
                mn_intf[0]
            );
        };

        let Ok(Ethernet(mut i2_tx, _i2_rx)) = datalink::channel(&mn_intf[1], Config::default())
        else {
            bail!(
                "Failed to parse ethernet channel on interface: {:#?}",
                mn_intf[1]
            );
        };

        while let Ok(i1_pkt) = i1_rx.next() {
            let Some(eth_pkt) = EthernetPacket::new(i1_pkt) else {
                continue;
            };

            i2_tx.build_and_send(1, eth_pkt.packet().len(), &mut |outbound| {
                let mut outbound = MutableEthernetPacket::new(outbound)
                    .expect("MutableEthernetPacket must construct successfully");
                outbound.clone_from(&eth_pkt);
            });
        }

        Ok(EthRouter {})
    }
}

fn main() -> anyhow::Result<()> {
    EthRouter::build()?;
    Ok(())
}
