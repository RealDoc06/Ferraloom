//! Port of BlockItem's ordinary placement pipeline. Context and outer use receivers
//! retain their distinct native identities through every synchronous callback.
use super::required_behavior;
use crate::{
    block::{
        BlockPlaceContext, InteractionResult, LevelReaderRef, StackView, UPDATE_LIMIT, UpdateFlags,
        UseOnContext,
    },
    entity::EntityRef,
    inventory::{StackOperation, StackRef},
    level::{Level, LevelError},
};
use ferraloom_data::{BlockState, Item, ItemMethod, registries::game_event};
use ferraloom_nbt::Tag;
use ferraloom_protocol::{
    component::{BlockItemStateProperties, ComponentTypeId, DataComponent},
    play::SoundSource,
};
use ferraloom_util::{BlockPos, Vec3};

/// Port of BlockItem#useOn. The explicit super call runs Item's body even on a subclass.
pub fn use_on(
    item: Item,
    context: &UseOnContext<'_>,
    operation: &mut StackOperation<'_>,
) -> Result<Option<InteractionResult>, LevelError> {
    let placement =
        BlockPlaceContext::new(&LevelReaderRef::Server(operation.level), context.clone())?;
    let placed = required_behavior(item, ItemMethod::Place)?.place(item, placement, operation)?;
    if placed.consumes_action() {
        return Ok(Some(placed));
    }
    let Some(result) = super::behavior::base_use_on(item, context, operation)? else {
        return Ok(None);
    };
    if result.consumes_action() {
        return Ok(Some(result));
    }
    let consumable = crate::entity::player::inventory::component(
        context.item_in_hand(&LevelReaderRef::Server(operation.level))?,
        ComponentTypeId::CONSUMABLE,
    )
    .is_some();
    if consumable {
        // Item#use/Consumable is slice D; no successful consumption is invented.
        super::stack::unported_use_on(item, operation)
    } else {
        Ok(Some(placed))
    }
}

/// Port of BlockItem#place, preserving the local placed-state capture after callbacks.
pub fn place(
    item: Item,
    context: BlockPlaceContext<'_>,
    operation: &mut StackOperation<'_>,
) -> Result<InteractionResult, LevelError> {
    let block = required_behavior(item, ItemMethod::GetBlock)?.get_block(item, operation)?;
    if !block.is_enabled_in_vanilla()
        || !context.can_place(&LevelReaderRef::Server(operation.level))?
    {
        return Ok(InteractionResult::Fail);
    }
    let Some(context) = required_behavior(item, ItemMethod::UpdatePlacementContext)?
        .update_placement_context(item, context, operation)?
    else {
        return Ok(InteractionResult::Fail);
    };
    let Some(placement_state) = required_behavior(item, ItemMethod::GetPlacementState)?
        .get_placement_state(item, &context, operation)?
    else {
        return Ok(InteractionResult::Fail);
    };
    if !required_behavior(item, ItemMethod::PlaceBlock)?.place_block(
        item,
        &context,
        placement_state,
        operation,
    )? {
        return Ok(InteractionResult::Fail);
    }
    let pos = context.clicked_pos();
    let player = context.player();
    let stack = context.stack_view();
    let mut placed_state = operation.level.get_block_state(pos)?;
    if placed_state.block() == placement_state.block() {
        placed_state =
            update_block_state_from_tag(item, pos, stack, placed_state, operation.level)?;
        update_custom_block_entity_tag(item, player, pos, stack, operation.level)?;
        update_block_entity_components(item, pos, stack, operation.level)?;
        operation
            .level
            .set_placed_by(placed_state, pos, player, stack)?;
        if let Some(player) = player {
            crate::advancements::criteria::placed_block(
                operation.level,
                player,
                pos,
                retained(stack)?,
            )?;
        }
    }
    let sound_type = placed_state.data().sound;
    let sound = required_behavior(item, ItemMethod::GetPlaceSound)?.get_place_sound(
        item,
        placed_state,
        operation,
    )?;
    let center = Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    );
    operation.level.play_sound(
        player,
        center,
        sound,
        SoundSource::Blocks,
        (sound_type.volume + 1.0) / 2.0,
        sound_type.pitch * 0.8,
    )?;
    operation.level.game_event(
        game_event::Id::BLOCK_PLACE,
        center,
        player,
        Some(placed_state),
    );
    super::stack::consume(retained(stack)?, 1, player, operation)?;
    Ok(InteractionResult::SUCCESS)
}

/// Port of BlockItemStateProperties#apply. Invalid values retain the incoming state.
pub fn apply_block_state(
    properties: &BlockItemStateProperties,
    mut state: BlockState,
) -> BlockState {
    for (name, value) in &properties.0 {
        if state.has_integer_property(name) {
            if let Some(value) = crate::predicates::block::java_integer(value)
                && let Ok(modified) = state.with_integer_property(name, value)
            {
                state = modified;
            }
        } else {
            state = BlockState::from_properties(
                state.block(),
                state
                    .properties()
                    .chain(std::iter::once((name.as_str(), value.as_str()))),
            )
            .expect("registered block state");
        }
    }
    state
}

/// Port of BlockItem#updateBlockStateFromTag, including UPDATE_CLIENTS = 2.
fn update_block_state_from_tag(
    item: Item,
    pos: BlockPos,
    stack: StackView<'_>,
    state: BlockState,
    level: &mut Level,
) -> Result<BlockState, LevelError> {
    let modified = match stack
        .read(&LevelReaderRef::Server(level))?
        .component(ComponentTypeId::BLOCK_STATE)
        .map_err(|error| super::stack::item_data_error(item, error))?
    {
        Some(DataComponent::BlockState(properties)) => apply_block_state(properties, state),
        _ => state,
    };
    if modified != state {
        level.set_block(pos, modified, UpdateFlags::CLIENTS, UPDATE_LIMIT)?;
    }
    Ok(modified)
}

/// Port of BlockItem#updateCustomBlockEntityTag and TypedEntityData#loadInto.
pub fn update_custom_block_entity_tag(
    item: Item,
    player: Option<EntityRef>,
    pos: BlockPos,
    stack: StackView<'_>,
    level: &mut Level,
) -> Result<bool, LevelError> {
    let reader = LevelReaderRef::Server(level);
    let Some(DataComponent::BlockEntityData(custom)) = stack
        .read(&reader)?
        .component(ComponentTypeId::BLOCK_ENTITY_DATA)
        .map_err(|error| super::stack::item_data_error(item, error))?
    else {
        return Ok(false);
    };
    let Some(entity) = level.block_entity_ref(pos)? else {
        return Ok(false);
    };
    let kind = level.block_entity_state(entity)?.data().kind;
    if kind != custom.entity_type {
        return Ok(false);
    }
    if kind.only_op_can_set_nbt()
        && !match player {
            Some(player) => level.can_use_game_master_blocks(player)?,
            None => false,
        }
    {
        return Ok(false);
    }
    let Tag::Compound(custom) = &custom.tag.0 else {
        return Err(LevelError::Inventory(
            "block entity item data is not a compound",
        ));
    };
    // TypedEntityData strips its discriminator on construction; it is never
    // part of the loadCustomOnly payload, including on a direct wire input.
    let mut custom = custom.clone();
    custom.remove("id");
    level.with_block_entity_callback(entity, |level| {
        let old = level
            .block_entity_state(entity)?
            .save_custom_only()
            .map_err(|error| super::stack::item_data_error(item, error))?;
        let mut merged = old.clone();
        merged.merge(&custom);
        if merged == old {
            return Ok(false);
        }
        // Keep TypedEntityData's receiver rooted after the nested load callback
        // returns, through the subsequent setChanged on that same generation.
        entity.load_custom_only(&merged, level)?;
        entity.set_changed(level)?;
        Ok(true)
    })
}

/// Port of BlockItem#updateBlockEntityComponents. Only immutable component values
/// are captured across the short exclusive block-entity borrow, never a stack copy.
fn update_block_entity_components(
    item: Item,
    pos: BlockPos,
    stack: StackView<'_>,
    level: &mut Level,
) -> Result<(), LevelError> {
    let Some(entity) = level.block_entity_ref(pos)? else {
        return Ok(());
    };
    let reader = LevelReaderRef::Server(level);
    let stack = stack.read(&reader)?;
    let prototype = stack
        .prototype()
        .map_err(|error| super::stack::item_data_error(item, error))?;
    let patch = stack.components_patch().clone();
    level
        .block_entity_state_mut(entity)?
        .apply_components(prototype, &patch);
    entity.set_changed(level)
}

fn retained(stack: StackView<'_>) -> Result<StackRef, LevelError> {
    match stack {
        StackView::Retained(reference) => Ok(reference),
        StackView::Borrowed(_) => Err(LevelError::Inventory("mutable item context receiver")),
    }
}
