//! AnvilBlock's FallingBlock and Fallable overrides. Entity physics and the
//! degradation probability remain in FallingBlockEntity.
use crate::{
    block::{BlockBehavior, BlockMethod, foundational::FALLING_BLOCK},
    entity::{DamageSource, EntityData, EntityRef, item::falling_block::FallingBlockEntity},
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockClass, BlockState, properties as p};
use ferraloom_util::BlockPos;

pub struct AnvilBlock;
pub static ANVIL: AnvilBlock = AnvilBlock;

pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    block.class() == BlockClass::AnvilBlock
        && matches!(
            method,
            BlockMethod::OnPlace
                | BlockMethod::Tick
                | BlockMethod::OnLand
                | BlockMethod::OnBrokenAfterFall
                | BlockMethod::GetFallDamageSource
        )
}

/// Port of AnvilBlock#falling, on the original newly added entity.
pub fn falling(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let falling = level.entity_mut::<FallingBlockEntity>(entity)?;
    falling.hurt_entities = true;
    falling.fall_damage_per_distance = 2.0;
    falling.fall_damage_max = 40;
    Ok(())
}

/// Port of AnvilBlock#damage. The degradation roll belongs to the entity;
/// this transform preserves facing and returns None after the damaged anvil.
pub fn damage(state: BlockState) -> Option<BlockState> {
    let next = match state.block() {
        Block::ANVIL => Block::CHIPPED_ANVIL,
        Block::CHIPPED_ANVIL => Block::DAMAGED_ANVIL,
        _ => return None,
    };
    next.default_state().with(p::FACING, state.get(p::FACING)?)
}

impl BlockBehavior for AnvilBlock {
    /// Port of inherited FallingBlock#onPlace, with delay 2.
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        FALLING_BLOCK.on_place(state, pos, old, moved, level)
    }

    /// Port of FallingBlock#tick with AnvilBlock#falling, using the same
    /// native creation/replacement path as sand and colored falling blocks.
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if crate::entity::item::falling_block::is_free(level.get_block_state(pos.below(1))?, level)?
            && pos.y >= level.min_build_height()
        {
            let entity = crate::entity::item::falling_block::fall(level, pos, state)?;
            falling(entity, level)?;
        }
        Ok(())
    }

    /// Port of AnvilBlock#onLand. End the canonical entity read before the
    /// synchronous level event, including when the receiver was just removed.
    fn on_land(
        &self,
        _state: BlockState,
        pos: BlockPos,
        _replaced: BlockState,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let silent = *level
            .entity::<EntityData>(entity)?
            .entity_data
            .get(crate::entity::synched_data::SILENT);
        if !silent {
            level.level_event(None, 1031, pos, 0)?;
        }
        Ok(())
    }

    /// Port of AnvilBlock#onBrokenAfterFall.
    fn on_broken_after_fall(
        &self,
        pos: BlockPos,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let silent = *level
            .entity::<EntityData>(entity)?
            .entity_data
            .get(crate::entity::synched_data::SILENT);
        if !silent {
            level.level_event(None, 1029, pos, 0)?;
        }
        Ok(())
    }

    /// Port of AnvilBlock#getFallDamageSource.
    fn get_fall_damage_source(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<DamageSource, LevelError> {
        Ok(level.damage_sources().anvil(entity))
    }
}
