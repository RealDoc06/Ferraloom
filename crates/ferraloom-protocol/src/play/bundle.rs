//! Port of BundlerInfo and PacketBundlePacker for the play clientbound stream.
use super::{Error, Result, clientbound};

#[derive(Clone, Debug, PartialEq)]
pub enum ClientboundMessage {
    Packet(Box<clientbound::Packet>),
    Bundle(Vec<clientbound::Packet>),
}
/// Incremental bundling runs after packet decoding and before client handling.
#[derive(Default)]
pub struct BundlePacker {
    current: Option<Vec<clientbound::Packet>>,
}
impl BundlePacker {
    pub const LIMIT: usize = 4096;
    pub fn push(&mut self, packet: clientbound::Packet) -> Result<Option<ClientboundMessage>> {
        if let Some(packets) = &mut self.current {
            if matches!(packet, clientbound::Packet::StartConfiguration(_)) {
                return Err(Error::invalid(
                    "bundle",
                    "Terminal message received in bundle",
                ));
            }
            if matches!(packet, clientbound::Packet::BundleDelimiter(_)) {
                return Ok(Some(ClientboundMessage::Bundle(
                    self.current.take().expect("open bundle"),
                )));
            }
            if packets.len() >= Self::LIMIT {
                return Err(Error::invalid("bundle", "Too many packets in a bundle"));
            }
            packets.push(packet);
            Ok(None)
        } else if matches!(packet, clientbound::Packet::BundleDelimiter(_)) {
            self.current = Some(Vec::new());
            Ok(None)
        } else {
            Ok(Some(ClientboundMessage::Packet(Box::new(packet))))
        }
    }
    pub fn is_bundling(&self) -> bool {
        self.current.is_some()
    }
}
impl ClientboundMessage {
    /// Port of BundlerInfo#unbundlePacket. Framing remains the transport's job.
    pub fn into_packets(self) -> Vec<clientbound::Packet> {
        match self {
            Self::Packet(packet) => vec![*packet],
            Self::Bundle(packets) => {
                let mut result = Vec::with_capacity(packets.len() + 2);
                result.push(clientbound::Packet::BundleDelimiter(
                    clientbound::BundleDelimiter {},
                ));
                result.extend(packets);
                result.push(clientbound::Packet::BundleDelimiter(
                    clientbound::BundleDelimiter {},
                ));
                result
            }
        }
    }
}
