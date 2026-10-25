//! ServerGamePacketListenerImpl's four mining actions. The canonical player's
//! mining-state field and production routing attach in slice D.
use super::*;
use ferraloom_game::entity::player::game_mode::{
    BlockBreakAction, BlockBreakingState, DestroyAction,
};

impl MinecraftServer {
    /// Port of ServerGamePacketListenerImpl#handlePlayerAction's mining cases.
    /// The explicit state borrow becomes the player's sole field at integration;
    /// this adapter neither allocates a listener state nor takes/restores one.
    pub fn handle_player_block_action(
        &mut self,
        connection: ConnectionId,
        entity: EntityRef,
        state: &mut BlockBreakingState,
        packet: sb::PlayerActionPacket,
    ) -> Result<()> {
        if !self
            .level
            .level
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
        let input = BlockBreakAction {
            action,
            pos: packet.pos,
            direction: packet.direction.0,
            max_y: self.level.level.max_build_height(),
        };
        state.handle_block_break_action(entity, input, &mut self.level.level)?;
        // Native mining packets precede ACK validation, including the clear
        // already sent when a negative sequence subsequently throws.
        self.apply_level_effects()?;
        self.connections
            .get_mut(&connection)
            .unwrap()
            .ack_block_changes_up_to(packet.sequence.0)?;
        Ok(())
    }
}
