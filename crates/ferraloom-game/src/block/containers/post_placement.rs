//! Native setPlacedBy callbacks. StackView is read only where vanilla reads the
//! ItemStack receiver; other callbacks intentionally leave it un-dereferenced.
use crate::{
    block::{
        BlockBehavior, BlockMethod, LevelAccessRef, LevelReaderRef, UpdateFlags,
        placement::StackView,
    },
    entity::{EntityRef, EntityState, player::ServerPlayer},
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockClass as C, BlockState, properties as p};
use ferraloom_protocol::{
    component::{ComponentTypeId, DataComponent},
    play::SignTextSlot,
};
use ferraloom_util::{BlockPos, Direction};

struct PostPlacement;
static POST_PLACEMENT: PostPlacement = PostPlacement;
pub(super) fn ported_behavior(
    block: Block,
    method: BlockMethod,
) -> Option<&'static dyn BlockBehavior> {
    let supported = if method == BlockMethod::SetPlacedBy {
        block.is_instance_of(C::AbstractBedBlock)
            || block.is_instance_of(C::SignBlock)
            || matches!(
                block.class(),
                C::CommandBlock
                    | C::CrafterBlock
                    | C::JukeboxBlock
                    | C::StructureBlock
                    | C::WitherSkullBlock
                    | C::WitherWallSkullBlock
            )
    } else {
        matches!(
            method,
            BlockMethod::UpdateShape | BlockMethod::UpdateFromNeighbourShapes
        ) && block.is_instance_of(C::AbstractBedBlock)
    };
    supported.then_some(&POST_PLACEMENT)
}
/// Port of SignBlock#setPlacedBy. Placement always edits FRONT, without the
/// useWithoutItem build/editor/command checks that belong to that other method.
fn sign(
    state: BlockState,
    pos: BlockPos,
    by: Option<EntityRef>,
    level: &mut Level,
) -> Result<(), LevelError> {
    let Some(player) = by else {
        return Ok(());
    };
    if !matches!(level.entity_state(player)?, EntityState::Player(_)) {
        return Ok(());
    }
    if !state.block().is_instance_of(C::SignBlock) {
        return Ok(());
    }
    let Some(handle) = level.block_entity_ref(pos)? else {
        return Ok(());
    };
    if !matches!(
        level.block_entity_state(handle)?,
        crate::block::entity::BlockEntityState::Sign(_)
    ) {
        return Ok(());
    }
    level.with_block_entity_callback(handle, |level| {
        let sign = level.block_entity::<super::sign::SignBlockEntity>(handle)?;
        let player_state = level.entity::<ServerPlayer>(player)?;
        if sign.is_waxed
            || !super::sign_block::has_editable_text(
                &sign.front_text,
                player_state.is_text_filtering_enabled(),
            )
        {
            return Ok(());
        }
        super::sign_block::open_text_edit(handle, player, SignTextSlot::Front, level)
    })
}
/// Port of JukeboxBlock#setPlacedBy. Presence of RecordItem, including an
/// incorrectly typed value, controls the flag; the embedded BE type is ignored.
fn jukebox(
    state: BlockState,
    pos: BlockPos,
    stack: StackView<'_>,
    level: &mut Level,
) -> Result<(), LevelError> {
    let has_record = {
        let reader = LevelReaderRef::Server(level);
        let receiver = stack.read(&reader)?;
        match receiver
            .component(ComponentTypeId::BLOCK_ENTITY_DATA)
            .map_err(|_| LevelError::Inventory("jukebox placement component"))?
        {
            Some(DataComponent::BlockEntityData(value)) => match &value.tag.0 {
                ferraloom_nbt::Tag::Compound(tag) => tag.get("RecordItem").is_some(),
                _ => false,
            },
            _ => false,
        }
    };
    if has_record {
        level.set_block(
            pos,
            state.with(p::HAS_RECORD, true).unwrap(),
            UpdateFlags::CLIENTS,
            crate::block::UPDATE_LIMIT,
        )?;
    }
    Ok(())
}
impl BlockBehavior for PostPlacement {
    /// Ports AbstractBedBlock, SignBlock, CommandBlock, CrafterBlock,
    /// JukeboxBlock, StructureBlock and both wither skull setPlacedBy overrides.
    fn set_placed_by(
        &self,
        state: BlockState,
        pos: BlockPos,
        by: Option<EntityRef>,
        stack: StackView<'_>,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let block = state.block();
        if block.is_instance_of(C::AbstractBedBlock) {
            let head = pos.relative(super::menu::facing(state), 1);
            level.set_block_and_update(head, state.with(p::PART, p::Part::Head).unwrap())?;
        } else if block.is_instance_of(C::SignBlock) {
            sign(state, pos, by, level)?;
        } else {
            match block.class() {
                C::CrafterBlock => {
                    if state.get(p::TRIGGERED) == Some(true) {
                        level.schedule_block_tick_after(pos, block, 4)?;
                    }
                }
                C::JukeboxBlock => jukebox(state, pos, stack, level)?,
                C::CommandBlock => super::command::set_placed_by(state, pos, stack, level)?,
                C::StructureBlock => super::structure::set_placed_by(pos, by, level)?,
                C::WitherSkullBlock | C::WitherWallSkullBlock => {
                    super::wither::check_spawn(pos, level)?
                }
                _ => unreachable!("registered setPlacedBy family"),
            }
        }
        Ok(())
    }
    /// Port of AbstractBedBlock#updateShape, used synchronously when head
    /// placement notifies the existing foot. Unrelated sides keep the state.
    fn update_shape(
        &self,
        state: BlockState,
        _pos: BlockPos,
        side: Direction,
        _neighbor_pos: BlockPos,
        neighbor: BlockState,
        _level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        let forward = super::menu::facing(state);
        let toward_partner = if state.get(p::PART) == Some(p::Part::Foot) {
            forward
        } else {
            forward.opposite()
        };
        if side != toward_partner {
            return Ok(state);
        }
        Ok(
            if neighbor.block() == state.block() && neighbor.get(p::PART) != state.get(p::PART) {
                state
                    .with(p::OCCUPIED, neighbor.get(p::OCCUPIED).unwrap())
                    .unwrap()
            } else {
                Block::AIR.default_state()
            },
        )
    }
}
