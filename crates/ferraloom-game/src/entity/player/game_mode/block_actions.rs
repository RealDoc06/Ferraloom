//! ServerPlayerGameMode's block-action state machine. The caller borrows the
//! sole mining state separately; attaching it to ServerPlayer is a later hunk.
use super::{BlockBreakingState, ServerPlayer, accumulated_destroy_progress};
use crate::{
    block::BlockMethod,
    entity::{
        EntityRef,
        player::{GameType, inventory},
    },
    level::{Level, LevelError},
    loot::LootError,
};
use ferraloom_data::{BlockClass, Item, registries::data_component_type::Id as ComponentType};
use ferraloom_protocol::{
    chat::{Argument, Component, TextColor},
    component::DataComponent,
    play::{PlayerAction, clientbound as cb},
};
use ferraloom_util::{BlockPos, Direction};

/// The four ServerboundPlayerActionPacket actions dispatched to gameMode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestroyAction {
    Start,
    ChangeDirection,
    Abort,
    Stop,
}
impl DestroyAction {
    pub fn from_player_action(action: PlayerAction) -> Option<Self> {
        Some(match action {
            PlayerAction::StartDestroyBlock => Self::Start,
            PlayerAction::ChangeDestroyDirection => Self::ChangeDirection,
            PlayerAction::AbortDestroyBlock => Self::Abort,
            PlayerAction::StopDestroyBlock => Self::Stop,
            _ => return None,
        })
    }
}
/// Inputs of ServerPlayerGameMode#handleBlockBreakAction. The sequence belongs
/// to the listener's ACK, not to mining state or the debug-only native logger.
#[derive(Clone, Copy, Debug)]
pub struct BlockBreakAction {
    pub action: DestroyAction,
    pub pos: BlockPos,
    pub direction: Direction,
    pub max_y: i32,
}

/// Required queries retain their original cause; item/enchantment dependencies
/// remain explicit until their canonical implementations land.
#[derive(Debug)]
pub enum BlockBreakError {
    Level(Box<LevelError>),
    Predicate(Box<LootError>),
    BlockEntityData(ferraloom_protocol::Error),
    Inventory(Box<crate::inventory::MenuError>),
    ItemOnHitBlock(Item),
    ItemCanDestroyBlock(Item),
}
impl From<LevelError> for BlockBreakError {
    fn from(error: LevelError) -> Self {
        Self::Level(Box::new(error))
    }
}
impl From<LootError> for BlockBreakError {
    fn from(error: LootError) -> Self {
        Self::Predicate(Box::new(error))
    }
}
impl From<crate::inventory::MenuError> for BlockBreakError {
    fn from(error: crate::inventory::MenuError) -> Self {
        match error {
            crate::inventory::MenuError::Level(error) => error.into(),
            error => Self::Inventory(Box::new(error)),
        }
    }
}
impl std::fmt::Display for BlockBreakError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Level(error) => write!(f, "block action: {error:?}"),
            Self::Predicate(error) => std::fmt::Display::fmt(error, f),
            Self::BlockEntityData(error) => std::fmt::Display::fmt(error, f),
            Self::Inventory(error) => std::fmt::Display::fmt(error, f),
            Self::ItemOnHitBlock(item) => {
                write!(f, "unported EnchantmentHelper#onHitBlock: {}", item.name())
            }
            Self::ItemCanDestroyBlock(item) => {
                write!(f, "unported Item#canDestroyBlock: {}", item.name())
            }
        }
    }
}
impl std::error::Error for BlockBreakError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Predicate(error) => Some(error.as_ref()),
            Self::BlockEntityData(error) => Some(error),
            Self::Inventory(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}
type Result<T> = std::result::Result<T, BlockBreakError>;

impl BlockBreakingState {
    /// Port of ServerPlayerGameMode#handleBlockBreakAction, without the optional
    /// DEBUG_BLOCK_BREAK logger. The listener ACKs only after this returns.
    pub fn handle_block_break_action(
        &mut self,
        player: EntityRef,
        input: BlockBreakAction,
        level: &mut Level,
    ) -> Result<()> {
        let BlockBreakAction {
            action,
            pos,
            direction,
            max_y,
        } = input;
        if !level
            .entity::<ServerPlayer>(player)?
            .is_within_block_interaction_range(pos, 1.0)
        {
            if self.is_destroying_block && action == DestroyAction::Abort {
                self.abort_destroy_block(player, pos, level)?;
            }
            return Ok(());
        }
        if pos.y > max_y {
            return send_block_update(player, pos, level);
        }
        match action {
            DestroyAction::Start => {
                if level.is_under_spawn_protection(player, pos)? {
                    let mut content = Component::translatable(
                        "build.spawn_protection",
                        vec![Argument::String(format!("{}, {}, {}", pos.x, pos.y, pos.z))],
                    );
                    content.style.color = Some(TextColor::parse("red").expect("native color"));
                    level.emit(crate::level::effects::LevelEffect::PlayerPacket {
                        recipients: vec![player],
                        packet: Box::new(cb::Packet::SystemChat(Box::new(cb::SystemChat {
                            content,
                            overlay: true,
                        }))),
                    });
                    return Ok(());
                }
                if !level.may_interact(player, pos)? {
                    return send_block_update(player, pos, level);
                }
                if level.entity::<ServerPlayer>(player)?.instabuild() {
                    return destroy_and_ack(player, pos, level);
                }
                if block_action_restricted(player, pos, level)? {
                    return send_block_update(player, pos, level);
                }
                self.destroy_progress_start = self.game_ticks;
                let state = level.get_block_state(pos)?;
                let mut progress = 1.0;
                if !state.is_air() {
                    let stack = level.entity::<ServerPlayer>(player)?.get_main_hand_item();
                    if let Some(DataComponent::Enchantments(enchantments)) =
                        inventory::component(stack, ComponentType::ENCHANTMENTS)
                        && !enchantments.0.is_empty()
                    {
                        return Err(BlockBreakError::ItemOnHitBlock(stack.item()));
                    }
                    level.attack_block(state, pos, player)?;
                    progress = level.get_destroy_progress(state, pos, player)?;
                }
                if !state.is_air() && progress >= 1.0 {
                    destroy_and_ack(player, pos, level)?;
                } else {
                    if self.is_destroying_block {
                        send_block_update(player, self.destroy_pos, level)?;
                    }
                    self.is_destroying_block = true;
                    self.destroy_pos = pos;
                    self.destroy_direction = direction;
                    let stage = (progress * 10.0) as i32;
                    level.destroy_block_progress(
                        level.entity::<ServerPlayer>(player)?.network_id,
                        pos,
                        stage,
                    )?;
                    self.last_sent_state = stage;
                }
            }
            DestroyAction::ChangeDirection => self.destroy_direction = direction,
            DestroyAction::Stop => {
                if pos == self.destroy_pos {
                    let ticks = self.game_ticks.wrapping_sub(self.destroy_progress_start);
                    let state = level.get_block_state(pos)?;
                    if !state.is_air() {
                        let progress = accumulated_destroy_progress(
                            level.get_destroy_progress(state, pos, player)?,
                            ticks,
                        )
                        .0;
                        if progress >= 0.7 {
                            self.is_destroying_block = false;
                            level.destroy_block_progress(
                                level.entity::<ServerPlayer>(player)?.network_id,
                                pos,
                                -1,
                            )?;
                            return destroy_and_ack(player, pos, level);
                        }
                        if !self.has_delayed_destroy {
                            self.is_destroying_block = false;
                            self.has_delayed_destroy = true;
                            self.delayed_destroy_pos = pos;
                            self.delayed_tick_start = self.destroy_progress_start;
                        }
                    }
                }
            }
            DestroyAction::Abort => self.abort_destroy_block(player, pos, level)?,
        }
        Ok(())
    }

    /// Port of ServerPlayerGameMode#abortDestroyBlock. A mismatched abort sends
    /// both clears; it leaves the delayed-destruction flag untouched.
    fn abort_destroy_block(
        &mut self,
        player: EntityRef,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<()> {
        self.is_destroying_block = false;
        let id = level.entity::<ServerPlayer>(player)?.network_id;
        if self.destroy_pos != pos {
            eprintln!(
                "Mismatch in destroy block pos: {:?} {:?}",
                self.destroy_pos, pos
            );
            level.destroy_block_progress(id, self.destroy_pos, -1)?;
        }
        level.destroy_block_progress(id, pos, -1)?;
        Ok(())
    }

    /// Port of ServerPlayerGameMode#tick. Mutations stay on this exact state;
    /// no second player map or take/restore is used while the field is pending.
    pub fn tick(&mut self, player: EntityRef, level: &mut Level) -> Result<()> {
        self.game_ticks = self.game_ticks.wrapping_add(1);
        if self.has_delayed_destroy {
            let state = level.get_block_state(self.delayed_destroy_pos)?;
            if state.is_air() {
                self.has_delayed_destroy = false;
            } else if self.increment_destroy_progress(
                player,
                state,
                self.delayed_destroy_pos,
                self.delayed_tick_start,
                level,
            )? >= 1.0
            {
                self.has_delayed_destroy = false;
                destroy_block(player, self.delayed_destroy_pos, level)?;
            }
        } else if self.is_destroying_block {
            let state = level.get_block_state(self.destroy_pos)?;
            let id = level.entity::<ServerPlayer>(player)?.network_id;
            if state.is_air() {
                level.destroy_block_progress(id, self.destroy_pos, -1)?;
                self.last_sent_state = -1;
                self.is_destroying_block = false;
            } else {
                let ticks = self.game_ticks.wrapping_sub(self.destroy_progress_start);
                let event = if ticks % 4 == 0 { 2020 } else { 2019 };
                self.increment_destroy_progress(player, state, self.destroy_pos, ticks, level)?;
                level.level_event(None, event, self.destroy_pos, self.destroy_direction as i32)?;
            }
        }
        Ok(())
    }
    fn increment_destroy_progress(
        &mut self,
        player: EntityRef,
        state: ferraloom_data::BlockState,
        pos: BlockPos,
        ticks: i32,
        level: &mut Level,
    ) -> Result<f32> {
        let (progress, stage) =
            accumulated_destroy_progress(level.get_destroy_progress(state, pos, player)?, ticks);
        if stage != self.last_sent_state {
            level.destroy_block_progress(
                level.entity::<ServerPlayer>(player)?.network_id,
                pos,
                stage,
            )?;
            self.last_sent_state = stage;
        }
        Ok(progress)
    }
}

/// Port of Player#blockActionRestricted and AdventureModePredicate#test with
/// BlockInWorld(loadChunks=false). Only the native state and NBT overload is
/// used here; component matchers belong to the other BlockPredicate overload.
pub fn block_action_restricted(
    player: EntityRef,
    pos: BlockPos,
    level: &mut Level,
) -> Result<bool> {
    let entity = level.entity::<ServerPlayer>(player)?;
    if !matches!(entity.game_mode, GameType::Adventure | GameType::Spectator) {
        return Ok(false);
    }
    if entity.game_mode == GameType::Spectator {
        return Ok(true);
    }
    if entity.may_build() {
        return Ok(false);
    }
    let stack = entity.get_main_hand_item();
    if stack.is_empty() {
        return Ok(true);
    }
    let Some(DataComponent::CanBreak(predicate)) =
        inventory::component(stack, ComponentType::CAN_BREAK)
    else {
        return Ok(true);
    };
    let predicates = predicate.predicates.clone();
    let mut state = None;
    let mut entity = None;
    let mut read_entity = false;
    let result = (|| {
        for predicate in predicates {
            // BlockInWorld(false) never acquires a missing chunk just to read
            // state. A null state is usable by a predicate with no state tests;
            // a state-dependent predicate keeps the required-query failure.
            if state.is_none() && level.has_chunk_at(pos) {
                state = Some(level.get_block_state(pos)?);
            }
            let predicate = crate::predicates::BlockPredicate(predicate);
            if let Some(state) = state {
                if !predicate.matches_state(state, level)? {
                    continue;
                }
            } else if predicate.0.blocks.is_some() || predicate.0.properties.is_some() {
                return Err(
                    LevelError::MissingChunk(ferraloom_util::ChunkPos::containing(pos)).into(),
                );
            }
            if let Some(test) = &predicate.0.nbt {
                if !read_entity {
                    let captured = level.get_or_promote_block_entity(pos)?;
                    if let Some(entity) = captured {
                        level.retain_block_entity(entity)?;
                    }
                    entity = captured;
                    read_entity = true;
                }
                // BlockInWorld caches the receiver, not its serialized NBT.
                let nbt = entity
                    .map(|entity| {
                        level
                            .block_entity_state(entity)?
                            .save_with_full_metadata()
                            .map(ferraloom_nbt::Tag::Compound)
                            .map_err(BlockBreakError::BlockEntityData)
                    })
                    .transpose()?;
                if !crate::predicates::NbtPredicate(test.0.0.clone()).matches(nbt.as_ref()) {
                    continue;
                }
            }
            return Ok(false);
        }
        Ok(true)
    })();
    if let Some(entity) = entity {
        level.release_block_entity(entity);
    }
    result
}

fn send_block_update(player: EntityRef, pos: BlockPos, level: &mut Level) -> Result<()> {
    let state = level.get_block_state(pos)?;
    level.send_player_block_update(player, pos, state);
    Ok(())
}
/// Port of ServerPlayerGameMode#destroyAndAck: this is the correction packet,
/// not the listener's sequence ACK (which follows the complete action).
fn destroy_and_ack(player: EntityRef, pos: BlockPos, level: &mut Level) -> Result<()> {
    if !destroy_block(player, pos, level)? {
        send_block_update(player, pos, level)?;
    }
    Ok(())
}

/// Port of ServerPlayerGameMode#destroyBlock. The original block dispatch and
/// block-entity generation survive removal; destroyedWith precedes tool wear.
pub fn destroy_block(player: EntityRef, pos: BlockPos, level: &mut Level) -> Result<bool> {
    let state = level.get_block_state(pos)?;
    let entity = level.entity::<ServerPlayer>(player)?;
    let stack = entity.get_main_hand_item();
    if stack.item() == Item::DEBUG_STICK {
        return Err(BlockBreakError::ItemCanDestroyBlock(stack.item()));
    }
    if let Some(DataComponent::Tool(tool)) = inventory::component(stack, ComponentType::TOOL)
        && !tool.can_destroy_blocks_in_creative
        && entity.instabuild()
    {
        return Ok(false);
    }
    let block_entity = level.get_or_promote_block_entity(pos)?;
    if let Some(entity) = block_entity {
        level.retain_block_entity(entity)?;
    }
    let result = (|| {
        if state.block().is_instance_of(BlockClass::GameMasterBlock)
            && !level.can_use_game_master_blocks(player)?
        {
            level.send_block_updated(pos, state, state);
            return Ok(false);
        }
        if block_action_restricted(player, pos, level)? {
            return Ok(false);
        }
        let adjusted = level.player_will_destroy(state, pos, player)?;
        let changed = level.remove_block(pos, false)?;
        if changed {
            level.optional_block_call(state.block(), BlockMethod::Destroy, |behavior, level| {
                behavior.destroy(adjusted, pos, level)
            })?;
        }
        if level.entity::<ServerPlayer>(player)?.prevents_block_drops() {
            return Ok(true);
        }
        let selected = level
            .entity::<ServerPlayer>(player)?
            .inventory
            .selected_slot();
        let mut operation = crate::inventory::StackOperation::new(level);
        let tool = operation.pin(crate::inventory::ContainerSlot {
            container: crate::inventory::ContainerHandle::PlayerInventory(player),
            slot: selected,
        })?;
        let receiver = operation.level.stack(tool)?;
        let destroyed_with = receiver.copy_with_count(inventory::count(receiver));
        let correct = operation
            .level
            .entity::<ServerPlayer>(player)?
            .has_correct_tool_for_drops(adjusted, operation.level)?;
        super::mining_item::mine_block(player, tool, adjusted, operation.level)?;
        if changed && correct {
            operation
                .level
                .required_block_behavior(state.block(), BlockMethod::PlayerDestroy)?
                .player_destroy(
                    crate::block::breaking::PlayerDestroyContext {
                        block: state.block(),
                        state: adjusted,
                        pos,
                        player,
                        block_entity,
                        destroyed_with: &destroyed_with,
                    },
                    operation.level,
                )?;
        }
        Ok(true)
    })();
    if let Some(entity) = block_entity {
        level.release_block_entity(entity);
    }
    result
}
