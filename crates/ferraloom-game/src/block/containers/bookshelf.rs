//! ChiseledBookShelfBlock and its ListBackedContainer's retained receiver hooks.
use crate::{
    block::{
        BlockBehavior, InteractionResult, UseContext,
        entity::{
            BlockEntityRef, BlockEntityState,
            chiseled_bookshelf::{MAX_BOOKS_IN_STORAGE, update_state},
        },
    },
    entity::player::{
        self, ServerPlayer,
        inventory::{count, set_count},
    },
    inventory::{ContainerHandle, ContainerSlot, StackOperation, StackValue},
    level::{Level, LevelError},
    stats::Stat,
};
use ferraloom_data::{
    BlockState, Item, properties as p,
    registries::{game_event::Id as GameEvent, sound_event::Id as SoundEvent},
    tags::ItemTags,
};
use ferraloom_protocol::play::{BlockHitResult, InteractionHand, SoundSource};
use ferraloom_util::{BlockPos, Direction, Vec3};

fn address(handle: BlockEntityRef, slot: usize) -> ContainerSlot {
    ContainerSlot {
        container: ContainerHandle::BlockEntity(handle),
        slot,
    }
}
fn item_error(error: crate::inventory::MenuError) -> LevelError {
    match error {
        crate::inventory::MenuError::Level(error) => error,
        _ => LevelError::Inventory("bookshelf stack receiver"),
    }
}
fn is_book(
    stack: &ferraloom_protocol::component::ItemStack,
    level: &Level,
) -> Result<bool, LevelError> {
    Ok(level
        .item_tags()?
        .contains_key(ItemTags::BOOKSHELF_BOOKS, stack.item()))
}

/// Port of ChiseledBookShelfBlockEntity#removeItem. Count is deliberately
/// ignored; return the old whole receiver, rooted in the caller's operation.
pub fn remove_item_value(
    handle: BlockEntityRef,
    slot: usize,
    _amount: i32,
    operation: &mut StackOperation<'_>,
) -> Result<StackValue, LevelError> {
    let receiver = operation.pin(address(handle, slot)).map_err(item_error)?;
    let was_empty = operation
        .level
        .stack(receiver)
        .map_err(item_error)?
        .is_empty();
    operation
        .level
        .replace_stack_object(address(handle, slot), StackValue::New(Default::default()))
        .map_err(item_error)?;
    if !was_empty {
        update_state(handle, slot, operation.level)?;
    }
    Ok(StackValue::Existing(receiver))
}

/// Port of ChiseledBookShelfBlockEntity#setItem. Accepted objects are assigned
/// without limiting their count. Empty values remove the original receiver.
pub fn set_item_value(
    handle: BlockEntityRef,
    slot: usize,
    value: StackValue,
    level: &mut Level,
) -> Result<(), LevelError> {
    if slot >= MAX_BOOKS_IN_STORAGE {
        return Err(LevelError::Inventory("bookshelf slot out of range"));
    }
    level.with_block_entity_callback(handle, |level| {
        let mut operation = StackOperation::new(level);
        if let StackValue::Existing(reference) = &value {
            operation.pin_existing(*reference).map_err(item_error)?;
        }
        let (accepted, empty) = {
            let stack = match &value {
                StackValue::New(stack) => stack,
                StackValue::Existing(reference) => {
                    operation.level.stack(*reference).map_err(item_error)?
                }
            };
            (is_book(stack, operation.level)?, stack.is_empty())
        };
        if accepted {
            operation
                .level
                .replace_stack_object(address(handle, slot), value)
                .map_err(item_error)?;
            update_state(handle, slot, operation.level)?;
        } else if empty {
            remove_item_value(handle, slot, 1, &mut operation)?;
        }
        Ok(())
    })
}

/// Port of SelectableSlotContainer#getHitSlot. Preserve the double subtraction,
/// float projection and pixel/section divisions at the boundary between cells.
pub fn get_hit_slot(hit: &BlockHitResult, facing: Direction) -> Option<usize> {
    if hit.direction != facing || !facing.axis().is_horizontal() {
        return None;
    }
    let neighbor = hit.block_pos.relative(facing, 1);
    let x = f64::from(hit.block_pos.x) + f64::from(hit.offset_x) - f64::from(neighbor.x);
    let y = f64::from(hit.block_pos.y) + f64::from(hit.offset_y) - f64::from(neighbor.y);
    let z = f64::from(hit.block_pos.z) + f64::from(hit.offset_z) - f64::from(neighbor.z);
    let horizontal = match facing {
        Direction::North => (1.0 - x) as f32,
        Direction::South => x as f32,
        Direction::West => z as f32,
        Direction::East => (1.0 - z) as f32,
        _ => unreachable!(),
    };
    let section = |coordinate: f32, sections: i32| {
        let targeted_pixel = coordinate * 16.0_f32;
        let section_size = 16.0_f32 / sections as f32;
        let value = targeted_pixel / section_size;
        let truncated = value as i32;
        let floor = if value < truncated as f32 {
            truncated.wrapping_sub(1)
        } else {
            truncated
        };
        floor.clamp(0, sections - 1) as usize
    };
    Some(section(horizontal, 3) + section(1.0_f32 - y as f32, 2) * 3)
}

pub(super) struct BookShelfBlock;
pub(super) static BOOKSHELF_BLOCK: BookShelfBlock = BookShelfBlock;
impl BlockBehavior for BookShelfBlock {
    fn use_item_on(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        let Some(handle) = level.block_entity_ref(pos)? else {
            return Ok(InteractionResult::Pass);
        };
        if !matches!(
            level.block_entity_state(handle)?,
            BlockEntityState::ChiseledBookShelf(_)
        ) {
            return Ok(InteractionResult::Pass);
        }
        level.with_block_entity_callback(handle, |level| {
            let selected = match context.hand {
                InteractionHand::MainHand => level
                    .entity::<ServerPlayer>(context.player)?
                    .inventory
                    .selected_slot(),
                InteractionHand::OffHand => 40,
            };
            let mut operation = StackOperation::new(level);
            let held = operation
                .pin(ContainerSlot {
                    container: ContainerHandle::PlayerInventory(context.player),
                    slot: selected,
                })
                .map_err(item_error)?;
            if !is_book(
                operation.level.stack(held).map_err(item_error)?,
                operation.level,
            )? {
                return Ok(InteractionResult::TryWithEmptyHand);
            }
            let Some(slot) = get_hit_slot(&context.hit, super::menu::facing(state)) else {
                return Ok(InteractionResult::Pass);
            };
            if occupied(state, slot) {
                return Ok(InteractionResult::TryWithEmptyHand);
            }
            let used = operation.level.stack(held).map_err(item_error)?.item();
            player::award_stat(context.player, Stat::Used(used), 1, operation.level)?;
            let enchanted =
                operation.level.stack(held).map_err(item_error)?.item() == Item::ENCHANTED_BOOK;
            // ItemStack#consumeAndReturn copies before querying the owner's
            // infinite-materials ability, then consumes the original receiver.
            let copy = operation
                .level
                .stack(held)
                .map_err(item_error)?
                .copy_with_count(1);
            if !operation
                .level
                .entity::<ServerPlayer>(context.player)?
                .instabuild()
            {
                let stack = operation.level.stack_mut(held).map_err(item_error)?;
                let remaining = count(stack).wrapping_sub(1);
                set_count(stack, remaining);
            }
            set_item_value(handle, slot, StackValue::New(copy), operation.level)?;
            play_sound(
                operation.level,
                pos,
                if enchanted {
                    SoundEvent::BLOCK_CHISELED_BOOKSHELF_INSERT_ENCHANTED
                } else {
                    SoundEvent::BLOCK_CHISELED_BOOKSHELF_INSERT
                },
            )?;
            Ok(InteractionResult::SUCCESS)
        })
    }
    fn use_without_item(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        let Some(handle) = level.block_entity_ref(pos)? else {
            return Ok(InteractionResult::Pass);
        };
        if !matches!(
            level.block_entity_state(handle)?,
            BlockEntityState::ChiseledBookShelf(_)
        ) {
            return Ok(InteractionResult::Pass);
        }
        let Some(slot) = get_hit_slot(&context.hit, super::menu::facing(state)) else {
            return Ok(InteractionResult::Pass);
        };
        if !occupied(state, slot) {
            return Ok(InteractionResult::CONSUME);
        }
        level.with_block_entity_callback(handle, |level| {
            let mut operation = StackOperation::new(level);
            let StackValue::Existing(retrieved) =
                remove_item_value(handle, slot, 1, &mut operation)?
            else {
                unreachable!()
            };
            let enchanted = operation.level.stack(retrieved).map_err(item_error)?.item()
                == Item::ENCHANTED_BOOK;
            play_sound(
                operation.level,
                pos,
                if enchanted {
                    SoundEvent::BLOCK_CHISELED_BOOKSHELF_PICKUP_ENCHANTED
                } else {
                    SoundEvent::BLOCK_CHISELED_BOOKSHELF_PICKUP
                },
            )?;
            if !crate::inventory::add_item_stack(context.player, retrieved, operation.level)
                .map_err(item_error)?
            {
                player::drop_item_stack(
                    context.player,
                    StackValue::Existing(retrieved),
                    false,
                    operation.level,
                )?;
            }
            operation.level.game_event(
                GameEvent::BLOCK_CHANGE,
                center(pos),
                Some(context.player),
                None,
            )?;
            Ok(InteractionResult::SUCCESS)
        })
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
        Ok(match level.block_entity_state(handle)? {
            BlockEntityState::ChiseledBookShelf(entity) => {
                entity.last_interacted_slot().wrapping_add(1)
            }
            _ => 0,
        })
    }
    fn affect_neighbors_after_removal(
        &self,
        state: BlockState,
        pos: BlockPos,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.update_neighbour_for_output_signal(pos, state.block())
    }
}
fn center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}
fn play_sound(level: &mut Level, pos: BlockPos, sound: SoundEvent) -> Result<(), LevelError> {
    level.play_sound(None, center(pos), sound, SoundSource::Blocks, 1.0, 1.0)
}
fn occupied(state: BlockState, slot: usize) -> bool {
    state
        .get(
            [
                p::SLOT_0_OCCUPIED,
                p::SLOT_1_OCCUPIED,
                p::SLOT_2_OCCUPIED,
                p::SLOT_3_OCCUPIED,
                p::SLOT_4_OCCUPIED,
                p::SLOT_5_OCCUPIED,
            ][slot],
        )
        .expect("bookshelf occupied property")
}

#[cfg(test)]
mod tests;
