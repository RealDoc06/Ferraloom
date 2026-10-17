//! Port of ConfigurationProtocols. Shared bodies live in [`crate::common`].
use crate::codec::{state_packets, wire_struct};
use crate::common::{clientbound as cb, serverbound as sb};
use crate::{Identifier, Reader, Result, Wire, Writer};
use ferraloom_nbt::Tag;

wire_struct!(/// Port of Clientbound/ServerboundFinishConfigurationPacket.
FinishConfiguration {});
wire_struct!(/// Port of ClientboundResetChatPacket.
ResetChat {});
wire_struct!(/// Port of ServerboundAcceptCodeOfConductPacket.
AcceptCodeOfConduct {});
wire_struct!(/// Port of ClientboundCodeOfConductPacket.
CodeOfConduct { code_of_conduct:String });
wire_struct!(/// Port of KnownPack#STREAM_CODEC.
KnownPack { namespace:String, id:String, version:String });
wire_struct!(/// Port of RegistrySynchronization.PackedRegistryEntry.
PackedRegistryEntry { id:Identifier, data:Option<Tag> });
wire_struct!(/// Port of ClientboundRegistryDataPacket.
RegistryData { registry:Identifier, entries:Vec<PackedRegistryEntry> });
wire_struct!(/// Port of ClientboundUpdateEnabledFeaturesPacket.
UpdateEnabledFeatures { features:Vec<Identifier> });
wire_struct!(/// Port of ClientboundSelectKnownPacks (unbounded clientbound list).
ClientboundSelectKnownPacks { known_packs:Vec<KnownPack> });
/// Port of ServerboundSelectKnownPacks (bounded to 64 packs).
#[derive(Clone, Debug, PartialEq)]
pub struct ServerboundSelectKnownPacks {
    pub known_packs: Vec<KnownPack>,
}
impl Wire for ServerboundSelectKnownPacks {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            known_packs: r.list(64, Wire::decode)?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.list(&self.known_packs, 64, |w, v| v.encode(w))
    }
}
state_packets!(ServerboundPacket, ferraloom_data::packets::configuration::serverbound::Packet, {
    ClientInformation(sb::ClientInformation) => ClientInformation,
    CookieResponse(sb::CookieResponse) => CookieResponse,
    CustomPayload(sb::CustomPayload) => CustomPayload,
    FinishConfiguration(FinishConfiguration) => FinishConfiguration,
    KeepAlive(sb::KeepAlive) => KeepAlive, Pong(sb::Pong) => Pong,
    ResourcePack(sb::ResourcePack) => ResourcePack,
    SelectKnownPacks(ServerboundSelectKnownPacks) => SelectKnownPacks,
    CustomClickAction(sb::CustomClickAction) => CustomClickAction,
    AcceptCodeOfConduct(AcceptCodeOfConduct) => AcceptCodeOfConduct,
});
state_packets!(ClientboundPacket, ferraloom_data::packets::configuration::clientbound::Packet, {
    CookieRequest(cb::CookieRequest) => CookieRequest, CustomPayload(cb::CustomPayload) => CustomPayload,
    Disconnect(cb::Disconnect) => Disconnect, FinishConfiguration(FinishConfiguration) => FinishConfiguration,
    KeepAlive(cb::KeepAlive) => KeepAlive, Ping(cb::Ping) => Ping, ResetChat(ResetChat) => ResetChat,
    RegistryData(RegistryData) => RegistryData, ResourcePackPop(cb::ResourcePackPop) => ResourcePackPop,
    ResourcePackPush(cb::ResourcePackPush) => ResourcePackPush, PostEffects(cb::PostEffects) => PostEffects,
    StoreCookie(cb::StoreCookie) => StoreCookie, Transfer(cb::Transfer) => Transfer,
    UpdateEnabledFeatures(UpdateEnabledFeatures) => UpdateEnabledFeatures, UpdateTags(cb::UpdateTags) => UpdateTags,
    SelectKnownPacks(ClientboundSelectKnownPacks) => SelectKnownPacks, CustomReportDetails(cb::CustomReportDetails) => CustomReportDetails,
    ServerLinks(cb::ServerLinks) => ServerLinks, ClearDialog(cb::ClearDialog) => ClearDialog,
    ShowDialog(cb::ShowDialog) => ShowDialog [cb::ShowDialog::decode_context_free, cb::ShowDialog::encode_context_free],
    CodeOfConduct(CodeOfConduct) => CodeOfConduct,
});
