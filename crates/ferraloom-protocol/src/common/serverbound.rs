//! Serverbound bodies shared by configuration, login, and play.
use super::*;
packet!(/// Port of ServerboundKeepAlivePacket.
KeepAlive { id:i64 });
packet!(/// Port of ServerboundPongPacket.
Pong { id:i32 });
packet!(/// Port of ServerboundPingRequestPacket.
PingRequest { time:i64 });
packet!(/// Port of ServerboundCookieResponsePacket.
CookieResponse {key:Identifier,payload:Option<ByteArray<5120>>});
wire_enum!(ResourcePackAction { SuccessfullyLoaded=0,Declined=1,FailedDownload=2,Accepted=3,Downloaded=4,InvalidUrl=5,FailedReload=6,Discarded=7 });
packet!(/// Port of ServerboundResourcePackPacket.
ResourcePack {id:Uuid,action:ResourcePackAction});
wire_enum!(ChatVisibility {Full=0,System=1,Hidden=2});
wire_enum!(HumanoidArm {Left=0,Right=1});
wire_enum!(ParticleStatus {All=0,Decreased=1,Minimal=2});
packet!(/// Port of ClientInformation.
ClientInformation {language:Utf8<16>,view_distance:i8,chat_visibility:ChatVisibility,chat_colors:bool,model_customisation:u8,main_hand:HumanoidArm,text_filtering_enabled:bool,allows_listing:bool,particle_status:ParticleStatus});

/// Port of ServerboundCustomPayloadPacket.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomPayload {
    pub payload: super::CustomPayload,
}
impl Wire for CustomPayload {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            payload: super::CustomPayload::decode(r, 32767)?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.payload.encode(w, 32767)
    }
}

/// Port of ServerboundCustomClickActionPacket's length-prefixed optional NBT.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomClickAction {
    pub id: Identifier,
    pub payload: Option<ferraloom_nbt::Tag>,
}
impl Wire for CustomClickAction {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.read()?;
        let bytes = r.byte_array(65536)?;
        let mut body = Reader::new(bytes);
        let payload = body.optional_nbt(false)?;
        body.finish()?;
        Ok(Self { id, payload })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.id.encode(w)?;
        let mut body = Writer::new();
        body.optional_nbt(self.payload.as_ref())?;
        w.byte_array(body.as_slice(), 65536)
    }
}
