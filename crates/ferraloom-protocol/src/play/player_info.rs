//! Port of ClientboundPlayerInfoUpdatePacket's action-selected entry format.
use super::*;
packet!(/// Port of GameProfile property network entries.
ProfileProperty {name:Utf8<64>,value:String,signature:Option<Utf8<1024>>});
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileProperties(pub Vec<ProfileProperty>);
impl Wire for ProfileProperties {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.list(16, Wire::decode).map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.list(&self.0, 16, |w, v| v.encode(w))
    }
}
wire_union!(PlayerInfoAction:var_int {
    AddPlayer=0 {name:Utf8<16>,properties:ProfileProperties},
    InitializeChat=1 {session:Option<RemoteChatSessionData>},
    UpdateGameMode=2 {game_mode:GameType},
    UpdateListed=3 {listed:bool},
    UpdateLatency=4 {latency:VarInt},
    UpdateDisplayName=5 {display_name:Option<Box<Component>> => optional_trusted_boxed_component},
    UpdateListOrder=6 {list_order:VarInt},
    UpdateHat=7 {show_hat:bool}
});
/// Values occur in ascending action order; only actions selected by the packet mask are present.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerInfoEntry {
    pub profile_id: Uuid,
    pub actions: Vec<PlayerInfoAction>,
}
