//! Clientbound bodies shared by configuration, login, and play.
use super::*;
packet!(/// Port of ClientboundKeepAlivePacket.
KeepAlive { id: i64 });
packet!(/// Port of ClientboundPingPacket.
Ping { id: i32 });
packet!(/// Port of ClientboundPongResponsePacket.
PongResponse { time: i64 });
packet!(/// Port of ClientboundCookieRequestPacket.
CookieRequest { key: Identifier });
packet!(/// Port of ClientboundStoreCookiePacket.
StoreCookie { key: Identifier, payload: ByteArray<5120> });
packet!(/// Port of ClientboundTransferPacket.
Transfer { host: String, port: VarInt });
packet!(/// Port of ClientboundResourcePackPopPacket.
ResourcePackPop { id: Option<Uuid> });
packet!(/// Port of ClientboundResourcePackPushPacket.
ResourcePackPush { id: Uuid, url: String, hash: Utf8<40>, required: bool, prompt: Option<Component> });
packet!(/// Port of ClientboundDisconnectPacket.
Disconnect { reason: Component });
packet!(/// Port of ClientboundPostEffectsPacket.
PostEffects { post_effects: Vec<Identifier> });
packet!(/// Port of ClientboundClearDialogPacket.
ClearDialog {});

wire_enum!(KnownLinkType { BugReport=0, CommunityGuidelines=1, Support=2, Status=3, Feedback=4, Community=5, Website=6, Forums=7, News=8, Announcements=9 });
packet!(/// Port of ServerLinks.UntrustedEntry.
ServerLink { link_type: Either<KnownLinkType, Component>, link: String });
packet!(/// Port of ClientboundServerLinksPacket.
ServerLinks { links: Vec<ServerLink> });

/// Port of Dialog's direct codec, which uses structured network NBT.
#[derive(Clone, Debug, PartialEq)]
pub struct DialogDefinition(pub ferraloom_nbt::Tag);
impl Wire for DialogDefinition {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.nbt(true).map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.nbt(&self.0)
    }
}
/// Port of ClientboundShowDialogPacket, including its context-free configuration form.
#[derive(Clone, Debug, PartialEq)]
pub struct ShowDialog {
    pub dialog: Holder<DialogDefinition>,
}
impl ShowDialog {
    pub fn decode_context_free(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            dialog: Holder::Direct(r.read()?),
        })
    }
    pub fn encode_context_free(&self, w: &mut Writer<'_>) -> Result<()> {
        match &self.dialog {
            Holder::Direct(dialog) => dialog.encode(w),
            Holder::Reference(_) => Err(Error::invalid(
                "dialog",
                "configuration requires a direct dialog",
            )),
        }
    }
}
impl Wire for ShowDialog {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            dialog: Holder::decode(r, &Identifier::parse("minecraft:dialog")?)?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.dialog
            .encode(w, &Identifier::parse("minecraft:dialog")?)
    }
}

packet!(/// Port of TagNetworkSerialization.NetworkPayload's tag entries.
TagEntry { name: Identifier, ids: Vec<VarInt> });
packet!(/// Port of ClientboundUpdateTagsPacket's registry entries.
RegistryTags { registry: Identifier, tags: Vec<TagEntry> });
packet!(/// Port of ClientboundUpdateTagsPacket. Entry order is retained for forwarding.
UpdateTags { tags: Vec<RegistryTags> });

packet!(/// Port of ClientboundCustomReportDetailsPacket's detail entry.
ReportDetail { key: Utf8<128>, value: Utf8<4096> });
/// Port of ClientboundCustomReportDetailsPacket, with the 32-entry limit.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomReportDetails {
    pub details: Vec<ReportDetail>,
}
impl Wire for CustomReportDetails {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            details: r.list(32, Wire::decode)?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.list(&self.details, 32, |w, v| v.encode(w))
    }
}

/// Port of ClientboundCustomPayloadPacket. Unknown channels have an unprefixed body.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomPayload {
    pub payload: super::CustomPayload,
}
impl Wire for CustomPayload {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            payload: super::CustomPayload::decode(r, 1048576)?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.payload.encode(w, 1048576)
    }
}
