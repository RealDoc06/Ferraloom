//! HopperBlockEntity transfer order, cooldowns and HopperBlock's power lock.
use super::{state::facing, storage::HopperBlockEntity};
use crate::{
    block::{
        BlockBehavior, LevelReaderRef, UPDATE_LIMIT, UpdateFlags,
        containers::{
            access::{self, ContainerRef},
            loot::unpack_loot_table,
            storage::BaseContainerBlockEntity,
        },
        entity::{BlockEntityRef, BlockEntityState},
    },
    entity::{
        EntityRef, EntityState, RemovalReason,
        item::ItemEntity,
        player::inventory::{count, is_same_item_same_components, max_stack_size, set_count},
    },
    inventory::Container,
    level::{Level, LevelError, redstone::Orientation},
};
use ferraloom_data::{Block, BlockState, properties as p, tags::BlockTags};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{Aabb, BlockPos, Direction};

/// All currently canonical entity subclasses are non-containers. Exhaustiveness
/// makes adding a vehicle require this query's real Container projection first.
fn is_entity_container(state: &EntityState) -> bool {
    match state {
        EntityState::Cow(_)
        | EntityState::Player(_)
        | EntityState::PrimedTnt(_)
        | EntityState::Item(_)
        | EntityState::FallingBlock(_)
        | EntityState::ExperienceOrb(_) => false,
    }
}
pub(super) fn container_at(
    level: &mut Level,
    pos: BlockPos,
) -> Result<Option<ContainerRef>, LevelError> {
    if let Some(container) = access::block_container_at(pos, level)? {
        return Ok(Some(container));
    }
    let candidates = level.entities_in(
        Aabb::new(
            f64::from(pos.x),
            f64::from(pos.y),
            f64::from(pos.z),
            f64::from(pos.x + 1),
            f64::from(pos.y + 1),
            f64::from(pos.z + 1),
        ),
        |_, state| is_entity_container(state),
    );
    if let Some(entity) = candidates.first() {
        return Err(LevelError::UnsupportedEntity(
            level.entity_state(*entity)?.entity_type(),
        ));
    }
    // getEntityContainer draws only when its filtered list is nonempty.
    Ok(None)
}
fn slots(
    container: &ContainerRef,
    face: Option<Direction>,
    level: &Level,
) -> Result<Vec<usize>, LevelError> {
    if let (ContainerRef::Single(handle), Some(face)) = (container, face) {
        Ok(access::slots_for_face(
            level.block_entity_state(*handle)?,
            face,
        ))
    } else {
        Ok((0..container.size(level)?).collect())
    }
}
fn is_empty(container: &ContainerRef, level: &mut Level) -> Result<bool, LevelError> {
    for &handle in container.handles() {
        unpack_loot_table(handle, None, level)?;
        if !level
            .block_entity::<BaseContainerBlockEntity>(handle)?
            .is_empty()
        {
            return Ok(false);
        }
    }
    Ok(true)
}
fn set_changed(container: &ContainerRef, level: &mut Level) -> Result<(), LevelError> {
    for &handle in container.handles() {
        level
            .block_entity_mut::<BaseContainerBlockEntity>(handle)?
            .set_changed();
        handle.set_changed(level)?;
    }
    Ok(())
}
fn is_full(
    container: &ContainerRef,
    face: Direction,
    level: &mut Level,
) -> Result<bool, LevelError> {
    for slot in slots(container, Some(face), level)? {
        let item = container.get_item(slot, level)?;
        if count(&item) < max_stack_size(&item) {
            return Ok(false);
        }
    }
    Ok(true)
}
/// Port of HopperBlockEntity#addItem. The destination generation(s) stay rooted
/// through synchronous setItem and comparator callbacks, including double chests.
/// Original ItemStack identity across reentrant slot replacement remains pending
/// the canonical StackRef pin/read/mutate API; block-entity roots do not pin stacks.
pub fn add_item(
    from: Option<&ContainerRef>,
    into: &ContainerRef,
    mut item: ItemStack,
    direction: Option<Direction>,
    level: &mut Level,
) -> Result<ItemStack, LevelError> {
    into.with_callback(level, |level| {
        let transfer = |level: &mut Level| {
            for slot in slots(into, direction, level)? {
                if item.is_empty() {
                    break;
                }
                let current = into.get_item(slot, level)?;
                let (handle, local) = into.slot(slot, level)?;
                let state = level.block_entity_state(handle)?;
                if !access::can_place_item(state, local, &item)
                    || !access::can_place_item_through_face(state, local, &item, direction)
                {
                    continue;
                }
                let was_empty = is_empty(into, level)?;
                let mut success = false;
                if current.is_empty() {
                    access::set_item(handle, local, std::mem::take(&mut item), level)?;
                    success = true;
                } else if count(&current) <= max_stack_size(&current)
                    && is_same_item_same_components(&current, &item)
                {
                    let amount = count(&item).min(max_stack_size(&item) - count(&current));
                    let remaining = count(&item).wrapping_sub(amount);
                    set_count(&mut item, remaining);
                    let current = &mut level
                        .block_entity_mut::<BaseContainerBlockEntity>(handle)?
                        .items[local];
                    let total = count(current).wrapping_add(amount);
                    set_count(current, total);
                    success = amount > 0;
                }
                if success {
                    if was_empty {
                        let source_time = match from {
                            Some(ContainerRef::Single(source)) => match level
                                .block_entity_state(*source)?
                            {
                                BlockEntityState::Hopper(hopper) => Some(hopper.ticked_game_time),
                                _ => None,
                            },
                            _ => None,
                        };
                        if let ContainerRef::Single(destination) = into {
                            if let BlockEntityState::Hopper(hopper) =
                                level.block_entity_state_mut(*destination)?
                            {
                                if !hopper.is_on_custom_cooldown() {
                                    let skip = i32::from(
                                        source_time
                                            .is_some_and(|time| hopper.ticked_game_time >= time),
                                    );
                                    hopper.cooldown_time = 8 - skip;
                                }
                            }
                        }
                    }
                    set_changed(into, level)?;
                }
            }
            Ok(item)
        };
        match from {
            Some(source) => source.with_callback(level, transfer),
            None => transfer(level),
        }
    })
}
fn restore_failed_take(
    source: &ContainerRef,
    slot: usize,
    original: ItemStack,
    level: &mut Level,
) -> Result<(), LevelError> {
    // Pending StackRef: native restores the original getItem receiver even if a
    // transfer callback replaced this slot. A fresh slot read loses that identity.
    let (handle, local) = source.slot(slot, level)?;
    if count(&original) == 1 {
        access::set_item(handle, local, original, level)?;
    } else {
        let current = &mut level
            .block_entity_mut::<BaseContainerBlockEntity>(handle)?
            .items[local];
        let size = count(&original);
        set_count(current, size);
    }
    Ok(())
}
fn eject_items(level: &mut Level, hopper: BlockEntityRef) -> Result<bool, LevelError> {
    let direction = facing(
        level
            .block_entity::<HopperBlockEntity>(hopper)?
            .parent
            .parent
            .parent
            .block_state,
    );
    let Some(into) = container_at(level, hopper.pos.relative(direction, 1))? else {
        return Ok(false);
    };
    let source = ContainerRef::Single(hopper);
    into.with_callback(level, |level| {
        if is_full(&into, direction.opposite(), level)? {
            return Ok(false);
        }
        for slot in 0..source.size(level)? {
            let original = source.get_item(slot, level)?;
            if original.is_empty() {
                continue;
            }
            let removed = access::remove_item(hopper, slot, 1, level)?;
            if add_item(
                Some(&source),
                &into,
                removed,
                Some(direction.opposite()),
                level,
            )?
            .is_empty()
            {
                set_changed(&into, level)?;
                return Ok(true);
            }
            restore_failed_take(&source, slot, original, level)?;
        }
        Ok(false)
    })
}
fn can_take(
    source: &ContainerRef,
    slot: usize,
    item: &ItemStack,
    face: Direction,
    level: &Level,
) -> Result<bool, LevelError> {
    let (handle, local) = source.slot(slot, level)?;
    let state = level.block_entity_state(handle)?;
    if matches!(state, BlockEntityState::Furnace(_)) && face == Direction::Down && local == 1 {
        let resources = level.container_resources()?;
        let tags = resources
            .facts
            .item_tags()
            .map_err(|_| LevelError::Inventory("hopper live item tags"))?;
        return Ok(access::can_take_item_through_face(
            state, local, item, face, tags,
        ));
    }
    Ok(true)
}
fn take_item_entity(
    level: &mut Level,
    hopper: BlockEntityRef,
    entity: EntityRef,
) -> Result<bool, LevelError> {
    let item = level.entity::<ItemEntity>(entity)?.item().clone();
    let remaining = add_item(None, &ContainerRef::Single(hopper), item, None, level)?;
    let complete = remaining.is_empty();
    level.entity_mut::<ItemEntity>(entity)?.set_item(remaining);
    if complete {
        level.remove_entity_with_reason(entity, RemovalReason::Discarded)?;
    }
    Ok(complete)
}
fn suck_in_items(level: &mut Level, hopper: BlockEntityRef) -> Result<bool, LevelError> {
    let above = hopper.pos.above(1);
    let state = level.get_block_state(above)?;
    if let Some(source) = container_at(level, above)? {
        return source.with_callback(level, |level| {
            for slot in slots(&source, Some(Direction::Down), level)? {
                let original = source.get_item(slot, level)?;
                if original.is_empty()
                    || !can_take(&source, slot, &original, Direction::Down, level)?
                {
                    continue;
                }
                let (handle, local) = source.slot(slot, level)?;
                let removed = access::remove_item(handle, local, 1, level)?;
                if add_item(
                    Some(&source),
                    &ContainerRef::Single(hopper),
                    removed,
                    None,
                    level,
                )?
                .is_empty()
                {
                    set_changed(&source, level)?;
                    return Ok(true);
                }
                restore_failed_take(&source, slot, original, level)?;
            }
            Ok(false)
        });
    }
    if LevelReaderRef::Server(level).is_collision_shape_full_block(state, above)?
        && !level
            .block_tags()?
            .contains_key(BlockTags::DOES_NOT_BLOCK_HOPPERS, state.block())
    {
        return Ok(false);
    }
    let p = hopper.pos;
    let items = level.entities_in(
        Aabb::new(
            f64::from(p.x),
            f64::from(p.y) + 11.0 / 16.0,
            f64::from(p.z),
            f64::from(p.x + 1),
            f64::from(p.y + 2),
            f64::from(p.z + 1),
        ),
        |_, state| matches!(state, EntityState::Item(item) if !item.parent.removed),
    );
    for item in items {
        if take_item_entity(level, hopper, item)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn try_move_items(
    level: &mut Level,
    hopper: BlockEntityRef,
    state: BlockState,
    action: impl FnOnce(&mut Level) -> Result<bool, LevelError>,
) -> Result<bool, LevelError> {
    if level
        .block_entity::<HopperBlockEntity>(hopper)?
        .is_on_cooldown()
        || !state.get(p::ENABLED).unwrap()
    {
        return Ok(false);
    }
    let mut changed = false;
    if !is_empty(&ContainerRef::Single(hopper), level)? {
        changed = eject_items(level, hopper)?;
    }
    let full = level
        .block_entity::<HopperBlockEntity>(hopper)?
        .parent
        .parent
        .items
        .iter()
        .all(|item| !item.is_empty() && count(item) == max_stack_size(item));
    if !full {
        changed |= action(level)?;
    }
    if changed {
        level
            .block_entity_mut::<HopperBlockEntity>(hopper)?
            .cooldown_time = 8;
        hopper.set_changed(level)?;
    }
    Ok(changed)
}
/// Port of HopperBlockEntity#pushItemsTick. Cooldown decrements even while locked.
pub fn push_items_tick(level: &mut Level, hopper: BlockEntityRef) -> Result<(), LevelError> {
    level.with_block_entity_callback(hopper, |level| {
        let time = level.game_time();
        let data = level.block_entity_mut::<HopperBlockEntity>(hopper)?;
        data.cooldown_time = data.cooldown_time.wrapping_sub(1);
        data.ticked_game_time = time;
        if !data.is_on_cooldown() {
            data.cooldown_time = 0;
            let state = data.parent.parent.parent.block_state;
            try_move_items(level, hopper, state, |level| suck_in_items(level, hopper))?;
        }
        Ok(())
    })
}
pub struct HopperBlock;
pub static HOPPER: HopperBlock = HopperBlock;
fn check_powered(level: &mut Level, pos: BlockPos, state: BlockState) -> Result<(), LevelError> {
    let enabled = !level.has_neighbor_signal(pos)?;
    if enabled != state.get(p::ENABLED).unwrap() {
        level.set_block(
            pos,
            state.with(p::ENABLED, enabled).unwrap(),
            UpdateFlags::CLIENTS,
            UPDATE_LIMIT,
        )?;
    }
    Ok(())
}
impl BlockBehavior for HopperBlock {
    fn has_analog_output_signal(&self, _state: BlockState) -> bool {
        true
    }
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if old.block() != state.block() {
            check_powered(level, pos, state)?;
        }
        Ok(())
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        _source: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        check_powered(level, pos, state)
    }
    fn analog_output_signal(
        &self,
        _state: BlockState,
        pos: BlockPos,
        _direction: Direction,
        level: &mut Level,
    ) -> Result<i32, LevelError> {
        let Some(handle) = level.block_entity_ref(pos)? else {
            return Ok(0);
        };
        if level.block_entity_state(handle)?.container().is_none() {
            return Ok(0);
        }
        access::analog_output(&ContainerRef::Single(handle), level)
    }
    fn affect_neighbors_after_removal(
        &self,
        _state: BlockState,
        pos: BlockPos,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.update_neighbour_for_output_signal(pos, Block::HOPPER)
    }
    fn entity_inside(
        &self,
        state: BlockState,
        pos: BlockPos,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let EntityState::Item(item) = level.entity_state(entity)? else {
            return Ok(());
        };
        if item.item().is_empty() {
            return Ok(());
        }
        let box_ = item.parent.bounding_box.move_by(
            -f64::from(pos.x),
            -f64::from(pos.y),
            -f64::from(pos.z),
        );
        if !box_.intersects(Aabb::new(0.0, 11.0 / 16.0, 0.0, 1.0, 2.0, 1.0)) {
            return Ok(());
        }
        let Some(hopper) = level.block_entity_ref(pos)? else {
            return Ok(());
        };
        if !matches!(
            level.block_entity_state(hopper)?,
            BlockEntityState::Hopper(_)
        ) {
            return Ok(());
        }
        level.with_block_entity_callback(hopper, |level| {
            try_move_items(level, hopper, state, |level| {
                take_item_entity(level, hopper, entity)
            })
        })?;
        Ok(())
    }
}
