//! HopperBlockEntity transfer order, cooldowns and HopperBlock's power lock.
use super::{stack_error, state::facing, storage::HopperBlockEntity};
use crate::{
    block::{
        BlockBehavior, LevelReaderRef, UPDATE_LIMIT, UpdateFlags,
        containers::access::{self, ContainerRef},
        entity::{BlockEntityRef, BlockEntityState},
    },
    entity::{
        EntityRef, EntityState, RemovalReason,
        item::ItemEntity,
        player::inventory::{
            copy_with_count, count, is_same_item_same_components, max_stack_size, set_count,
        },
    },
    inventory::{ContainerHandle, ContainerSlot, StackOperation, StackRef, StackValue},
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
    container.is_empty(level)
}
fn set_changed(container: &ContainerRef, level: &mut Level) -> Result<(), LevelError> {
    container.set_changed(level)
}
fn is_full(
    container: &ContainerRef,
    face: Direction,
    level: &mut Level,
) -> Result<bool, LevelError> {
    let mut operation = StackOperation::new(level);
    for slot in slots(container, Some(face), operation.level)? {
        let (handle, local) = container.slot(slot, operation.level)?;
        let item = operation
            .pin(ContainerSlot {
                container: ContainerHandle::BlockEntity(handle),
                slot: local,
            })
            .map_err(stack_error)?;
        let stack = operation.level.stack(item).map_err(stack_error)?;
        if count(stack) < max_stack_size(stack) {
            return Ok(false);
        }
    }
    Ok(true)
}
/// Port of HopperBlockEntity#addItem. The destination generation(s) stay rooted
/// through synchronous setItem and comparator callbacks, including double chests.
pub fn add_item(
    from: Option<&ContainerRef>,
    into: &ContainerRef,
    item: ItemStack,
    direction: Option<Direction>,
    level: &mut Level,
) -> Result<ItemStack, LevelError> {
    match add_item_value(from, into, StackValue::New(item), direction, level)? {
        StackValue::New(item) => Ok(item),
        StackValue::Existing(_) => unreachable!("owned transfer stays an owned remainder"),
    }
}

/// The same native transfer for a fresh split or an original removeItem receiver.
/// The caller keeps Existing rooted through its post-transfer restore callbacks.
pub fn add_item_value(
    from: Option<&ContainerRef>,
    into: &ContainerRef,
    mut item: StackValue,
    direction: Option<Direction>,
    level: &mut Level,
) -> Result<StackValue, LevelError> {
    let mut operation = StackOperation::new(level);
    if let StackValue::Existing(reference) = &item {
        operation.pin_existing(*reference).map_err(stack_error)?;
    }
    into.with_callback(operation.level, |level| {
        let transfer = |level: &mut Level| {
            for slot in slots(into, direction, level)? {
                if item.as_stack(level).map_err(stack_error)?.is_empty() {
                    break;
                }
                item = try_move_in_item_value(from, into, item, slot, direction, level)?;
            }
            Ok(item)
        };
        match from {
            Some(source) => source.with_callback(level, transfer),
            None => transfer(level),
        }
    })
}
/// Port of HopperBlockEntity#tryMoveInItem. getItem's receiver remains rooted
/// across isEmpty, lazy loot, setItem and comparator callbacks.
#[cfg(test)]
pub(super) fn try_move_in_item(
    from: Option<&ContainerRef>,
    into: &ContainerRef,
    item: ItemStack,
    slot: usize,
    direction: Option<Direction>,
    level: &mut Level,
) -> Result<ItemStack, LevelError> {
    match try_move_in_item_value(from, into, StackValue::New(item), slot, direction, level)? {
        StackValue::New(item) => Ok(item),
        StackValue::Existing(_) => unreachable!("owned transfer stays an owned remainder"),
    }
}
fn try_move_in_item_value(
    from: Option<&ContainerRef>,
    into: &ContainerRef,
    mut item: StackValue,
    slot: usize,
    direction: Option<Direction>,
    level: &mut Level,
) -> Result<StackValue, LevelError> {
    let mut operation = StackOperation::new(level);
    let (handle, local) = into.slot(slot, operation.level)?;
    let current = operation
        .pin(ContainerSlot {
            container: ContainerHandle::BlockEntity(handle),
            slot: local,
        })
        .map_err(stack_error)?;
    let state = operation.level.block_entity_state(handle)?;
    let incoming = item.as_stack(operation.level).map_err(stack_error)?;
    if !access::can_place_item(state, local, incoming, operation.level)?
        || !access::can_place_item_through_face(state, local, incoming, direction)
    {
        return Ok(item);
    }
    let was_empty = is_empty(into, operation.level)?;
    let held = operation.level.stack(current).map_err(stack_error)?;
    let incoming = item.as_stack(operation.level).map_err(stack_error)?;
    let mut success = false;
    if held.is_empty() {
        access::set_item_value(handle, local, item, operation.level)?;
        item = StackValue::New(ItemStack::default());
        success = true;
    } else if count(held) <= max_stack_size(held) && is_same_item_same_components(held, incoming) {
        let amount = count(incoming).min(max_stack_size(incoming) - count(held));
        let remaining = count(incoming).wrapping_sub(amount);
        let receiver = match &mut item {
            StackValue::New(stack) => stack,
            StackValue::Existing(reference) => {
                operation.level.stack_mut(*reference).map_err(stack_error)?
            }
        };
        set_count(receiver, remaining);
        let current = operation.level.stack_mut(current).map_err(stack_error)?;
        let total = count(current).wrapping_add(amount);
        set_count(current, total);
        success = amount > 0;
    }
    if success {
        if was_empty {
            let source_time = match from {
                Some(ContainerRef::Single(source)) => {
                    match operation.level.block_entity_state(*source)? {
                        BlockEntityState::Hopper(hopper) => Some(hopper.ticked_game_time),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let ContainerRef::Single(destination) = into
                && let BlockEntityState::Hopper(hopper) =
                    operation.level.block_entity_state_mut(*destination)?
                && !hopper.is_on_custom_cooldown()
            {
                let skip =
                    i32::from(source_time.is_some_and(|time| hopper.ticked_game_time >= time));
                hopper.cooldown_time = 8 - skip;
            }
        }
        set_changed(into, operation.level)?;
    }
    Ok(item)
}

fn restore_failed_take(
    handle: BlockEntityRef,
    slot: usize,
    original: StackRef,
    original_count: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    set_count(
        level.stack_mut(original).map_err(stack_error)?,
        original_count,
    );
    if original_count == 1 {
        access::set_item_value(handle, slot, StackValue::Existing(original), level)?;
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
        let mut operation = StackOperation::new(level);
        for slot in 0..source.size(operation.level)? {
            let original = operation
                .pin(ContainerSlot {
                    container: ContainerHandle::BlockEntity(hopper),
                    slot,
                })
                .map_err(stack_error)?;
            let stack = operation.level.stack(original).map_err(stack_error)?;
            if stack.is_empty() {
                continue;
            }
            let original_count = count(stack);
            let removed = access::remove_item(hopper, slot, 1, operation.level)?;
            if add_item(
                Some(&source),
                &into,
                removed,
                Some(direction.opposite()),
                operation.level,
            )?
            .is_empty()
            {
                set_changed(&into, operation.level)?;
                return Ok(true);
            }
            restore_failed_take(hopper, slot, original, original_count, operation.level)?;
        }
        Ok(false)
    })
}

fn can_take(
    source: &ContainerRef,
    slot: usize,
    item: StackRef,
    into: &ContainerRef,
    face: Direction,
    level: &mut Level,
) -> Result<bool, LevelError> {
    let (handle, local) = source.slot(slot, level)?;
    if !access::can_take_item(handle, local, item, into, level)? {
        return Ok(false);
    }
    let state = level.block_entity_state(handle)?;
    if matches!(state, BlockEntityState::Furnace(_)) && face == Direction::Down && local == 1 {
        let tags = level.item_tags()?;
        return Ok(access::can_take_item_through_face(
            state,
            local,
            level.stack(item).map_err(stack_error)?,
            face,
            tags,
        ));
    }
    Ok(true)
}
pub(super) fn take_item_entity(
    level: &mut Level,
    hopper: BlockEntityRef,
    entity: EntityRef,
) -> Result<bool, LevelError> {
    let mut operation = StackOperation::new(level);
    let original = operation
        .pin(ContainerSlot {
            container: ContainerHandle::Entity(entity),
            slot: 0,
        })
        .map_err(stack_error)?;
    // Native makes a new object before invoking destination container callbacks.
    let stack = operation.level.stack(original).map_err(stack_error)?;
    let item = copy_with_count(stack, count(stack));
    let remaining = add_item(
        None,
        &ContainerRef::Single(hopper),
        item,
        None,
        operation.level,
    )?;
    let complete = remaining.is_empty();
    operation
        .level
        .entity_mut::<ItemEntity>(entity)?
        .set_item(if complete {
            ItemStack::default()
        } else {
            remaining
        });
    if complete {
        operation
            .level
            .remove_entity_with_reason(entity, RemovalReason::Discarded)?;
    }
    Ok(complete)
}

pub(super) fn suck_in_items(level: &mut Level, hopper: BlockEntityRef) -> Result<bool, LevelError> {
    let above = hopper.pos.above(1);
    let state = level.get_block_state(above)?;
    if let Some(source) = container_at(level, above)? {
        return source.with_callback(level, |level| {
            let destination = ContainerRef::Single(hopper);
            let mut operation = StackOperation::new(level);
            for slot in slots(&source, Some(Direction::Down), operation.level)? {
                let (handle, local) = source.slot(slot, operation.level)?;
                let original = operation
                    .pin(ContainerSlot {
                        container: ContainerHandle::BlockEntity(handle),
                        slot: local,
                    })
                    .map_err(stack_error)?;
                if operation
                    .level
                    .stack(original)
                    .map_err(stack_error)?
                    .is_empty()
                    || !can_take(
                        &source,
                        slot,
                        original,
                        &destination,
                        Direction::Down,
                        operation.level,
                    )?
                {
                    continue;
                }
                let original_count = count(operation.level.stack(original).map_err(stack_error)?);
                let removed = access::remove_item_value(handle, local, 1, &mut operation)?;
                if add_item_value(Some(&source), &destination, removed, None, operation.level)?
                    .as_stack(operation.level)
                    .map_err(stack_error)?
                    .is_empty()
                {
                    set_changed(&source, operation.level)?;
                    return Ok(true);
                }
                restore_failed_take(handle, local, original, original_count, operation.level)?;
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

#[cfg(test)]
#[path = "bookshelf_transfer_tests.rs"]
mod bookshelf_transfer_tests;
