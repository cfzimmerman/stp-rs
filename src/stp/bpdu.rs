use bytemuck::{Pod, Zeroable};
use pnet::{
    packet::ethernet::{EthernetPacket, MutableEthernetPacket},
    util::MacAddr,
};
use std::mem;

/// A bridge protocol data unit packet. This is not full-spec. I'm
/// choosing a subset of fields and using aligned data types instead of
/// protocol field sizes for ease of implementation.
/// Assumes packets are unversioned and for spanning tree.
/// https://support.huawei.com/enterprise/en/doc/EDOC1000178168/e99e1364/bpdu-format
#[repr(C)]
#[derive(Pod, Zeroable, Copy, Clone, Debug)]
pub struct Bpdu {
    root_cost: u8,
    root_id: [u8; 6],
    bridge_id: [u8; 6],
}

/// A buffer used to construct Bpdu packets. All Bpdu
/// packets have the same size and are sent one at a time, so this is
/// just a nice way to reuse a single allocation for all packet construction.
pub struct BpduBuf(pub Vec<u8>);

impl Bpdu {
    /// A reserved MAC address often used for layer 2 protocols like STP
    /// https://notes.networklessons.com/stp-bpdu-destination-mac-address
    pub const BPDU_MAC: MacAddr = MacAddr(0x01, 0x80, 0xc2, 0x0, 0x0, 0x0);

    /// Builds a new bpdu type, casting Mac addresses into raw octets that
    /// satisfy bytemuck trait bounds.
    pub fn new(root_cost: u8, root_id: MacAddr, bridge_id: MacAddr) -> Self {
        Bpdu {
            root_cost,
            root_id: root_id.octets(),
            bridge_id: bridge_id.octets(),
        }
    }

    /// Returns a u8 buffer capable of holding exactly the size of a bpdu ethernet packet.
    pub fn make_buf() -> BpduBuf {
        BpduBuf(vec![
            0;
            EthernetPacket::minimum_packet_size()
                + mem::size_of::<Bpdu>()
        ])
    }

    #[inline]
    pub fn cost(&self) -> u8 {
        self.root_cost
    }

    #[inline]
    pub fn root_id(&self) -> MacAddr {
        self.root_id.into()
    }

    #[inline]
    pub fn bridge_id(&self) -> MacAddr {
        self.bridge_id.into()
    }

    /// Makes a bpdu ethernet packet in the given `bpdu_buf`.
    pub fn make_packet<'a>(
        &self,
        bpdu_buf: &'a mut BpduBuf,
        src_mac: MacAddr,
    ) -> EthernetPacket<'a> {
        let mut pkt = MutableEthernetPacket::new(&mut bpdu_buf.0)
            .expect("Bpdu packet size should be constant, and the buf should always accomodate what's needed");

        pkt.set_payload(bytemuck::bytes_of(self));
        pkt.set_source(src_mac);
        pkt.set_destination(Self::BPDU_MAC);
        pkt.consume_to_immutable()
    }
}
