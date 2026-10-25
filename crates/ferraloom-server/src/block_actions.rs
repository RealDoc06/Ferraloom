//! ServerGamePacketListenerImpl's four mining actions over the canonical player.
use super::*;
use ferraloom_game::entity::player::game_mode::{self, BlockBreakAction, DestroyAction};

impl MinecraftServer {
    /// Port of ServerGamePacketListenerImpl#handlePlayerAction's mining cases.
    /// Mining state stays on ServerPlayer throughout callbacks and tick dispatch.
    pub fn handle_player_block_action(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        packet: sb::PlayerActionPacket,
    ) -> Result<()> {
        if !self
            .level_for_entity(entity)
            .map_err(game_error)?
            .entity::<ServerPlayer>(entity)
            .map_err(game_error)?
            .has_client_loaded()
        {
            return Ok(());
        }
        self.connections
            .get_mut(&connection)
            .context("missing player connection")?
            .last_action = Instant::now();
        let action = DestroyAction::from_player_action(packet.action)
            .context("not a block-break player action")?;
        let dimension = self
            .level_for_entity(entity)
            .map_err(game_error)?
            .dimension_key()
            .map_err(game_error)?
            .clone();
        let input = BlockBreakAction {
            action,
            pos: packet.pos,
            direction: packet.direction.0,
            max_y: self.live_level_in(&dimension)?.level.max_build_height(),
        };
        game_mode::handle_block_break_action(
            entity,
            input,
            &mut self.live_level_in_mut(&dimension)?.level,
        )?;
        // Native mining packets precede ACK validation, including the clear
        // already sent when a negative sequence subsequently throws.
        self.apply_level_effects_in(&dimension)?;
        self.connections
            .get_mut(&connection)
            .unwrap()
            .ack_block_changes_up_to(packet.sequence.0)?;
        Ok(())
    }
}
