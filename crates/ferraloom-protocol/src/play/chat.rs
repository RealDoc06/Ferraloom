//! Port of Minecraft's signed chat network structures.
use super::*;

/// Port of MessageSignature: an RSA signature has exactly 256 bytes and no prefix.
#[derive(Clone, Debug, PartialEq)]
pub struct MessageSignature(pub [u8; 256]);
impl Wire for MessageSignature {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(
            r.take(256)?.try_into().map_err(|_| Error::UnexpectedEof)?,
        ))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.extend(&self.0);
        Ok(())
    }
}

/// Port of MessageSignature.Packed. The zero discriminator introduces a full signature.
#[derive(Clone, Debug, PartialEq)]
pub enum PackedMessageSignature {
    Full(MessageSignature),
    Cached(i32),
}
impl Wire for PackedMessageSignature {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.var_int()?.wrapping_sub(1);
        if id == -1 {
            Ok(Self::Full(r.read()?))
        } else {
            Ok(Self::Cached(id))
        }
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        match self {
            Self::Full(v) => {
                w.var_int(0);
                v.encode(w)
            }
            Self::Cached(id) => {
                w.var_int(id.wrapping_add(1));
                Ok(())
            }
        }
    }
}

/// Port of LastSeenMessages.Update (20 acknowledgement bits and a checksum byte).
#[derive(Clone, Debug, PartialEq)]
pub struct LastSeenMessagesUpdate {
    pub offset: VarInt,
    pub acknowledged: BitSet,
    pub checksum: i8,
}
impl Wire for LastSeenMessagesUpdate {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            offset: r.read()?,
            acknowledged: BitSet::decode_fixed(r, 20)?,
            checksum: r.i8()?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.offset.encode(w)?;
        self.acknowledged.encode_fixed(w, 20)?;
        w.i8(self.checksum);
        Ok(())
    }
}

packet!(/// Port of ArgumentSignatures.Entry.
ArgumentSignature {name:Utf8<16>,signature:MessageSignature});
/// Port of ArgumentSignatures (at most eight signed command arguments).
#[derive(Clone, Debug, PartialEq)]
pub struct ArgumentSignatures(pub Vec<ArgumentSignature>);
impl Wire for ArgumentSignatures {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.list(8, Wire::decode).map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.list(&self.0, 8, |w, v| v.encode(w))
    }
}
/// Port of LastSeenMessages.Packed (at most twenty entries).
#[derive(Clone, Debug, PartialEq)]
pub struct LastSeenMessagesPacked(pub Vec<PackedMessageSignature>);
impl Wire for LastSeenMessagesPacked {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.list(20, Wire::decode).map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.list(&self.0, 20, |w, v| v.encode(w))
    }
}
packet!(/// Port of SignedMessageBody.Packed.
SignedMessageBodyPacked {content:Utf8<256>,timestamp:i64,salt:i64,last_seen:LastSeenMessagesPacked});
packet!(/// Port of ProfilePublicKey.Data. Public key bytes are X.509 DER, not Minecraft NBT.
ProfilePublicKeyData {expires_at:i64,key:ByteArray<512>,key_signature:ByteArray<4096>});
packet!(/// Port of RemoteChatSession.Data.
RemoteChatSessionData {session_id:Uuid,profile_public_key:ProfilePublicKeyData});

/// Port of FilterMask#STREAM_CODEC.
#[derive(Clone, Debug, PartialEq)]
pub enum FilterMask {
    PassThrough,
    FullyFiltered,
    PartiallyFiltered(BitSet),
}
impl Wire for FilterMask {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        match r.var_int()? {
            1 => Ok(Self::FullyFiltered),
            2 => Ok(Self::PartiallyFiltered(r.read()?)),
            _ => Ok(Self::PassThrough),
        }
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        match self {
            Self::PassThrough => w.var_int(0),
            Self::FullyFiltered => w.var_int(1),
            Self::PartiallyFiltered(bits) => {
                w.var_int(2);
                bits.encode(w)?;
            }
        }
        Ok(())
    }
}

wire_enum!(ChatDecorationParameter {Sender=0,Target=1,Content=2},zero);
packet!(/// Port of ChatTypeDecoration.
ChatTypeDecoration {translation_key:String,parameters:Vec<ChatDecorationParameter>,style:crate::chat::Style});
packet!(/// Port of ChatType.DIRECT_STREAM_CODEC.
ChatType {chat:ChatTypeDecoration,narration:ChatTypeDecoration});
#[derive(Clone, Debug, PartialEq)]
pub struct ChatTypeHolder(pub crate::codec::Holder<ChatType>);
impl Wire for ChatTypeHolder {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        crate::codec::Holder::decode(r, &Identifier::parse("minecraft:chat_type")?).map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.0.encode(w, &Identifier::parse("minecraft:chat_type")?)
    }
}
packet!(/// Port of ChatType.Bound.
ChatTypeBound {chat_type:ChatTypeHolder,name:Component,target_name:Option<Component>});
