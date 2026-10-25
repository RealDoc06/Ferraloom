//! Port of BlockItem state properties and typed block-entity data application.
use crate::{
    block::{LevelReaderRef, StackView},
    entity::EntityRef,
    inventory::StackRef,
    level::{Level, LevelError},
};
use ferraloom_data::{BlockState, Item};
use ferraloom_nbt::Tag;
use ferraloom_protocol::component::{BlockItemStateProperties, ComponentTypeId, DataComponent};
use ferraloom_util::BlockPos;

/// Port of BlockItemStateProperties#apply. Invalid values retain the incoming state.
pub fn apply_block_state(
    properties: &BlockItemStateProperties,
    mut state: BlockState,
) -> BlockState {
    // Native map decoding retains the last value for each key. Independent
    // state properties commute, so reverse traversal can discard earlier keys.
    let mut applied = std::collections::BTreeSet::new();
    for (name, value) in properties.0.iter().rev() {
        if !applied.insert(name) {
            continue;
        }
        if state.has_integer_property(name) {
            if let Some(value) = crate::predicates::java_integer(value)
                && let Ok(modified) = state.with_integer_property(name, value)
            {
                state = modified;
            }
        } else {
            let Some((property, _)) = state.properties().find(|(property, _)| *property == name)
            else {
                continue;
            };
            let Some(value) = state.property_values(property).and_then(|values| {
                values
                    .iter()
                    .copied()
                    .find(|candidate| *candidate == value.as_str())
            }) else {
                continue;
            };
            state = BlockState::from_properties(
                state.block(),
                state.properties().chain(std::iter::once((property, value))),
            )
            .expect("registered block state");
        }
    }
    state
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
    if kind.name() != custom.entity_type.0.as_str() {
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

pub(super) fn retained(stack: StackView<'_>) -> Result<StackRef, LevelError> {
    match stack {
        StackView::Retained(reference) => Ok(reference),
        StackView::Borrowed(_) => Err(LevelError::Inventory("mutable item context receiver")),
    }
}
