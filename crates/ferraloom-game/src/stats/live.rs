use crate::{
    entity::{EntityRef, player::ServerPlayer},
    level::{Level, LevelError, effects::LevelEffect},
};
use ferraloom_protocol::play::{ClientCommandAction, clientbound as cb};

/// Port of ServerGamePacketListenerImpl's REQUEST_STATS adapter. The pure command
/// branch owns dirty-set behavior; the live adapter captures ordered output.
pub fn request_stats(level: &mut Level, player: EntityRef) -> Result<(), LevelError> {
    let packet = super::handle_client_command(
        &mut level.entity_mut::<ServerPlayer>(player)?.stats,
        ClientCommandAction::RequestStats,
    )
    .expect("REQUEST_STATS always produces its response");
    level.emit(LevelEffect::PlayerPacket {
        recipients: vec![player],
        packet: Box::new(cb::Packet::AwardStats(packet)),
    });
    Ok(())
}
