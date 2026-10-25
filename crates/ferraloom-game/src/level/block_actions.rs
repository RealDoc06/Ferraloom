//! Immediate block-action packets with their native call-site audience.
use super::{Level, LevelError, effects::LevelEffect};
use crate::entity::{EntityRef, EntityState};
use ferraloom_data::BlockState;
use ferraloom_protocol::{
    codec::VarInt,
    play::{BlockStateId, clientbound as cb},
};
use ferraloom_util::BlockPos;

impl Level {
    /// Port of ServerLevel#destroyBlockProgress. Include spectators and exclude
    /// only the source network ID. The radius is strict, from the integer pos.
    pub fn destroy_block_progress(
        &mut self,
        source_id: i32,
        pos: BlockPos,
        progress: i32,
    ) -> Result<(), LevelError> {
        let mut recipients = Vec::new();
        for entity in self.all_entities() {
            if let EntityState::Player(player) = self.entity_state(entity)?
                && player.network_id != source_id
            {
                let dx = f64::from(pos.x) - player.position.x;
                let dy = f64::from(pos.y) - player.position.y;
                let dz = f64::from(pos.z) - player.position.z;
                if dx * dx + dy * dy + dz * dz < 1024.0 {
                    recipients.push(entity);
                }
            }
        }
        self.emit(LevelEffect::PlayerPacket {
            recipients,
            packet: Box::new(cb::Packet::BlockDestruction(cb::BlockDestruction {
                id: VarInt(source_id),
                pos,
                progress: progress as u8,
            })),
        });
        Ok(())
    }

    /// Port of the direct ClientboundBlockUpdatePacket corrections in
    /// ServerPlayerGameMode; separate from holder block-change broadcasts.
    pub fn send_player_block_update(
        &mut self,
        player: EntityRef,
        pos: BlockPos,
        state: BlockState,
    ) {
        self.emit(LevelEffect::PlayerPacket {
            recipients: vec![player],
            packet: Box::new(cb::Packet::BlockUpdate(cb::BlockUpdate {
                pos,
                block_state: BlockStateId(i32::from(state.id())),
            })),
        });
    }
}
