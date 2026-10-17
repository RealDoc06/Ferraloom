//! Port of HandshakeProtocols. No clientbound packets exist in this state.
use crate::codec::state_packets;
use crate::{Error, Reader, Result, Wire, Writer};

/// Port of ClientIntent#byId (the IDs start at one).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientIntent {
    Status,
    Login,
    Transfer,
}
impl Wire for ClientIntent {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        match r.var_int()? {
            1 => Ok(Self::Status),
            2 => Ok(Self::Login),
            3 => Ok(Self::Transfer),
            id => Err(Error::invalid("connection intent", id.to_string())),
        }
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(match self {
            Self::Status => 1,
            Self::Login => 2,
            Self::Transfer => 3,
        });
        Ok(())
    }
}
/// Port of ClientIntentionPacket. Vanilla's reader limits the host to 255 UTF-16
/// units; its writer deliberately uses the default 32767 limit.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientIntention {
    pub protocol_version: i32,
    pub host_name: String,
    pub port: u16,
    pub intention: ClientIntent,
}
impl Wire for ClientIntention {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            protocol_version: r.var_int()?,
            host_name: r.string(255)?.into_owned(),
            port: r.u16()?,
            intention: r.read()?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(self.protocol_version);
        w.string(&self.host_name, 32767)?;
        w.u16(self.port);
        w.write(&self.intention)
    }
}
state_packets!(ServerboundPacket, ferraloom_data::packets::handshake::serverbound::Packet, { Intention(ClientIntention) => Intention });
#[derive(Clone, Debug, PartialEq)]
pub enum ClientboundPacket {}
impl Wire for ClientboundPacket {
    fn decode(_: &mut Reader<'_>) -> Result<Self> {
        Err(Error::invalid("handshake", "No clientbound packets"))
    }
    fn encode(&self, _: &mut Writer<'_>) -> Result<()> {
        match *self {}
    }
}
