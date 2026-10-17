//! Port of LoginProtocols, including 26.3's login-finished session UUID.
use crate::chat::{Component, ProfileProperty};
use crate::codec::{VarInt, state_packets, wire_struct};
use crate::common::{clientbound as cb, serverbound as sb};
use crate::{Error, Identifier, Reader, Result, Uuid, Wire, Writer};

/// Port of ByteBufCodecs.GAME_PROFILE.
#[derive(Clone, Debug, PartialEq)]
pub struct GameProfile {
    pub id: Uuid,
    pub name: String,
    pub properties: Vec<ProfileProperty>,
}
impl Wire for GameProfile {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            id: r.read()?,
            name: r.string(16)?.into_owned(),
            properties: r.list(16, |r| {
                Ok(ProfileProperty {
                    name: r.string(64)?.into_owned(),
                    value: r.string(32767)?.into_owned(),
                    signature: r.optional(|r| Ok(r.string(1024)?.into_owned()))?,
                })
            })?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.write(&self.id)?;
        w.string(&self.name, 16)?;
        w.list(&self.properties, 16, |w, p| {
            w.string(&p.name, 64)?;
            w.string(&p.value, 32767)?;
            w.optional(&p.signature, |w, v| w.string(v, 1024))
        })
    }
}
/// Port of ServerboundHelloPacket.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerboundHello {
    pub name: String,
    pub profile_id: Uuid,
}
impl Wire for ServerboundHello {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            name: r.string(16)?.into_owned(),
            profile_id: r.read()?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.string(&self.name, 16)?;
        w.write(&self.profile_id)
    }
}
/// Port of ServerboundKeyPacket. RSA payloads are opaque until the login handler.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerboundKey {
    pub key_bytes: Vec<u8>,
    pub encrypted_challenge: Vec<u8>,
}
impl Wire for ServerboundKey {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            key_bytes: r.byte_array(r.remaining().len())?.to_vec(),
            encrypted_challenge: r.byte_array(r.remaining().len())?.to_vec(),
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.byte_array(&self.key_bytes, i32::MAX as usize)?;
        w.byte_array(&self.encrypted_challenge, i32::MAX as usize)
    }
}
/// Port of ClientboundHelloPacket; read's 20-unit server ID limit is asymmetric.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientboundHello {
    pub server_id: String,
    pub public_key: Vec<u8>,
    pub challenge: Vec<u8>,
    pub should_authenticate: bool,
}
impl Wire for ClientboundHello {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            server_id: r.string(20)?.into_owned(),
            public_key: r.byte_array(r.remaining().len())?.to_vec(),
            challenge: r.byte_array(r.remaining().len())?.to_vec(),
            should_authenticate: r.bool()?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.string(&self.server_id, 32767)?;
        w.byte_array(&self.public_key, i32::MAX as usize)?;
        w.byte_array(&self.challenge, i32::MAX as usize)?;
        w.bool(self.should_authenticate);
        Ok(())
    }
}
wire_struct!(/// Port of ClientboundLoginCompressionPacket.
LoginCompression { threshold: VarInt });
wire_struct!(/// Port of ClientboundLoginFinishedPacket.
LoginFinished { game_profile: GameProfile, session_id: Uuid });
wire_struct!(/// Port of ServerboundLoginAcknowledgedPacket.
LoginAcknowledged {});

/// Port of ClientboundCustomQueryPacket. Unknown data is retained for forwarding;
/// vanilla's built-in DiscardedQueryPayload consumes it without interpretation.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomQuery {
    pub transaction_id: i32,
    pub id: Identifier,
    pub data: Vec<u8>,
}
impl Wire for CustomQuery {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let transaction_id = r.var_int()?;
        let id = r.read()?;
        if r.remaining().len() > 1048576 {
            return Err(Error::Length {
                length: r.remaining().len() as i64,
                max: 1048576,
            });
        }
        Ok(Self {
            transaction_id,
            id,
            data: r.rest().to_vec(),
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(self.transaction_id);
        w.write(&self.id)?;
        w.extend(&self.data);
        Ok(())
    }
}
/// Port of ServerboundCustomQueryAnswerPacket#read. Vanilla reads the transaction
/// ID and discards all remaining bytes, including writeNullable's discriminator.
/// Retaining that exact suffix also represents the empty suffix vanilla accepts.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomQueryAnswer {
    pub transaction_id: i32,
    pub wire_payload: Vec<u8>,
}
impl CustomQueryAnswer {
    pub fn new(transaction_id: i32, payload: Option<&[u8]>) -> Self {
        let mut wire_payload = vec![u8::from(payload.is_some())];
        if let Some(bytes) = payload {
            wire_payload.extend_from_slice(bytes);
        }
        Self {
            transaction_id,
            wire_payload,
        }
    }
}
impl Wire for CustomQueryAnswer {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let transaction_id = r.var_int()?;
        if r.remaining().len() > 1048576 {
            return Err(Error::Length {
                length: r.remaining().len() as i64,
                max: 1048576,
            });
        }
        Ok(Self {
            transaction_id,
            wire_payload: r.rest().to_vec(),
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(self.transaction_id);
        w.extend(&self.wire_payload);
        Ok(())
    }
}
/// Port of ClientboundLoginDisconnectPacket: JSON, not network NBT.
#[derive(Clone, Debug, PartialEq)]
pub struct LoginDisconnect {
    pub reason: Component,
}
impl Wire for LoginDisconnect {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            reason: Component::from_json_lenient(&r.string(262144)?)?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.string(&self.reason.to_json()?, 262144)
    }
}
state_packets!(ServerboundPacket, ferraloom_data::packets::login::serverbound::Packet, {
    Hello(ServerboundHello) => Hello, Key(ServerboundKey) => Key, CustomQueryAnswer(CustomQueryAnswer) => CustomQueryAnswer,
    LoginAcknowledged(LoginAcknowledged) => LoginAcknowledged, CookieResponse(sb::CookieResponse) => CookieResponse,
});
state_packets!(ClientboundPacket, ferraloom_data::packets::login::clientbound::Packet, {
    Disconnect(LoginDisconnect) => LoginDisconnect, Hello(ClientboundHello) => Hello, LoginFinished(LoginFinished) => LoginFinished,
    Compression(LoginCompression) => LoginCompression, CustomQuery(CustomQuery) => CustomQuery, CookieRequest(cb::CookieRequest) => CookieRequest,
});
