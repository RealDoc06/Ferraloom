//! DispenserBlock, DropperBlock and common dispense behaviors. Unported native
//! registry classes remain explicit errors until their concrete hooks are installed.
use super::{hopper, stack_error, state::facing, storage::DispenserBlockEntity};
use crate::{
    block::{
        BlockBehavior, UPDATE_LIMIT, UpdateFlags,
        containers::{
            access::{self, ContainerRef},
            loot::unpack_loot_table,
        },
        entity::{BlockEntityRef, BlockEntityType},
    },
    entity::{
        EntityRef, EntityState,
        item::ItemEntity,
        player::inventory::{component, copy_with_count, count, set_count, split},
    },
    inventory::{ContainerHandle, ContainerSlot, StackOperation, StackRef, StackValue},
    level::{Level, LevelError, effects::LevelEffect, redstone::Orientation},
};
use ferraloom_data::{
    Block, BlockState, Item,
    dispenser_properties::{
        DispenseBehaviorKind, ProjectileDispenseConfig, ProjectileDispensePosition,
    },
    properties as p,
    registries::game_event::Id as GameEvent,
    tags::ItemTags,
};
use ferraloom_protocol::component::{ComponentTypeId, ItemStack};
use ferraloom_util::{Axis, BlockPos, Direction, Vec3, random::RandomSource};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct DispenseSource {
    pub pos: BlockPos,
    pub state: BlockState,
    pub entity: BlockEntityRef,
    pub slot: usize,
    /// The getItem receiver pinned by the enclosing dispense operation.
    pub stack: StackRef,
}
/// A native behavior owns its sound/animation and operates on the canonical
/// receiver before each world callback. Existing results preserve that receiver
/// through native setItem callbacks; New is reserved for native fresh objects.
pub type DispenseHook = fn(DispenseSource, &mut Level) -> Result<StackValue, LevelError>;
#[derive(Clone, Debug)]
pub struct ProjectileDispense {
    pub position: Vec3,
    pub direction: Direction,
    /// Rooted by the enclosing dispense operation through factory and shoot calls.
    pub stack: StackRef,
    pub config: ProjectileDispenseConfig,
}
/// Entity implementations perform ProjectileItem#asProjectile and
/// Projectile#spawnProjectileUsingShoot, including subclass shoot overrides.
pub type ProjectileSpawnHook = fn(ProjectileDispense, &mut Level) -> Result<EntityRef, LevelError>;
#[derive(Default)]
pub struct DispenseRegistry {
    item_hooks: BTreeMap<Item, DispenseHook>,
    projectile_spawn: Option<ProjectileSpawnHook>,
}
pub fn register_behavior(level: &mut Level, item: Item, hook: DispenseHook) {
    level
        .region_mut()
        .redstone
        .dispense
        .item_hooks
        .insert(item, hook);
}
pub fn set_projectile_spawn_hook(level: &mut Level, hook: ProjectileSpawnHook) {
    level.region_mut().redstone.dispense.projectile_spawn = Some(hook);
}
fn center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}
pub fn projectile_position(
    pos: BlockPos,
    direction: Direction,
    kind: ProjectileDispensePosition,
) -> Vec3 {
    let step = direction.step();
    let (scale, offset) = match kind {
        ProjectileDispensePosition::Default => (0.7, 0.1),
        ProjectileDispensePosition::BlockFace => (1.0, 0.0),
        ProjectileDispensePosition::Firework => (0.5000099999997474, 0.0),
    };
    let c = center(pos);
    Vec3::new(
        c.x + f64::from(step.x) * scale,
        c.y + (f64::from(step.y) * scale + offset),
        c.z + f64::from(step.z) * scale,
    )
}
fn effect(level: &mut Level, pos: BlockPos, id: i32, data: i32) {
    level.emit(LevelEffect::LevelEvent { pos, id, data });
}
/// Port of DefaultDispenseItemBehavior#spawnItem. Item constructor RNG runs
/// before the level's power and six triangular-deviation draws.
pub fn spawn_item(
    level: &mut Level,
    stack: ItemStack,
    accuracy: i32,
    direction: Direction,
    position: Vec3,
) -> Result<EntityRef, LevelError> {
    let mut position = position;
    position.y -= if direction.axis() == Axis::Y {
        0.125
    } else {
        0.15625
    };
    let mut item = ItemEntity::new_at(level, position, stack)?;
    let random = level.random();
    let power = random.next_double() * 0.1 + 0.2;
    let spread = 0.0172275 * f64::from(accuracy);
    let step = direction.step();
    item.parent.velocity = Vec3::new(
        random.triangle(f64::from(step.x) * power, spread),
        random.triangle(0.2, spread),
        random.triangle(f64::from(step.z) * power, spread),
    );
    let handle = level.next_entity_ref();
    level.add_entity(handle, EntityState::Item(Box::new(item)))?;
    Ok(handle)
}
/// Port of DefaultDispenseItemBehavior#dispense/execute. Splitting changes the
/// original input receiver before addFreshEntity, including after slot replacement.
pub fn default_dispense(
    source: DispenseSource,
    level: &mut Level,
) -> Result<StackValue, LevelError> {
    let mut operation = StackOperation::new(level);
    operation.pin_existing(source.stack).map_err(stack_error)?;
    let piece = split(
        operation
            .level
            .stack_mut(source.stack)
            .map_err(stack_error)?,
        1,
    );
    let direction = facing(source.state);
    let step = direction.step();
    let c = center(source.pos);
    let position = Vec3::new(
        c.x + 0.7 * f64::from(step.x),
        c.y + 0.7 * f64::from(step.y),
        c.z + 0.7 * f64::from(step.z),
    );
    spawn_item(operation.level, piece, 6, direction, position)?;
    effect(operation.level, source.pos, 1000, 0);
    effect(operation.level, source.pos, 2000, direction as i32);
    Ok(StackValue::Existing(source.stack))
}
fn projectile_dispense(
    source: DispenseSource,
    level: &mut Level,
) -> Result<StackValue, LevelError> {
    let mut operation = StackOperation::new(level);
    operation.pin_existing(source.stack).map_err(stack_error)?;
    let item = operation
        .level
        .stack(source.stack)
        .map_err(stack_error)?
        .item();
    let config = item
        .dispenser_properties()
        .projectile
        .expect("registered projectile behavior");
    let hook = operation
        .level
        .region()
        .redstone
        .dispense
        .projectile_spawn
        .ok_or(LevelError::Inventory("projectile entity dispense hook"))?;
    let direction = facing(source.state);
    hook(
        ProjectileDispense {
            position: projectile_position(source.pos, direction, config.position),
            direction,
            stack: source.stack,
            config,
        },
        operation.level,
    )?;
    let stack = operation
        .level
        .stack_mut(source.stack)
        .map_err(stack_error)?;
    let remaining = count(stack).wrapping_sub(1);
    set_count(stack, remaining);
    effect(
        operation.level,
        source.pos,
        config.override_dispense_event.unwrap_or(1002),
        0,
    );
    effect(operation.level, source.pos, 2000, direction as i32);
    Ok(StackValue::Existing(source.stack))
}

/// Port of DropperBlock#dispenseFrom's post-transfer copy. The argument is the
/// original getItem receiver, read after every destination callback finishes.
pub(super) fn copy_after_transfer(
    original: StackRef,
    transferred: bool,
    level: &Level,
) -> Result<ItemStack, LevelError> {
    let stack = level.stack(original).map_err(stack_error)?;
    let mut result = copy_with_count(stack, count(stack));
    if transferred {
        let remaining = count(&result).wrapping_sub(1);
        set_count(&mut result, remaining);
    }
    Ok(result)
}

fn method(level: &Level, stack: &ItemStack) -> Result<Option<DispenseHook>, LevelError> {
    let item = stack.item();
    if !crate::loot::LootFacts::item_enabled(level, stack)
        .map_err(|_| LevelError::Inventory("dispenser enabled item features"))?
    {
        return Ok(Some(default_dispense));
    }
    if let Some(&hook) = level.region().redstone.dispense.item_hooks.get(&item) {
        return Ok(Some(hook));
    }
    let facts = item.dispenser_properties();
    if let Some(behavior) = facts.registered_behavior {
        return match behavior {
            DispenseBehaviorKind::Noop => Ok(None),
            DispenseBehaviorKind::ProjectileDispenseBehavior => Ok(Some(projectile_dispense)),
            other => Err(LevelError::Inventory(other.java_class())),
        };
    }
    if component(stack, ComponentTypeId::EQUIPPABLE).is_some() {
        return Err(LevelError::Inventory("EquipmentDispenseItemBehavior"));
    }
    let tags = level.item_tags()?;
    if tags.contains_key(ItemTags::SULFUR_CUBE_SWALLOWABLE, item) {
        return Err(LevelError::Inventory("SulfurCubeBlockDispenseItemBehavior"));
    }
    if facts.spawn_egg && component(stack, ComponentTypeId::ENTITY_DATA).is_some() {
        return Err(LevelError::Inventory("SpawnEggItemBehavior"));
    }
    Ok(Some(default_dispense))
}
fn random_slot(level: &mut Level, handle: BlockEntityRef) -> Result<Option<usize>, LevelError> {
    unpack_loot_table(handle, None, level)?;
    let occupied: [bool; 9] = {
        let receiver = level.block_entity::<DispenserBlockEntity>(handle)?;
        std::array::from_fn(|slot| !receiver.parent.parent.items[slot].is_empty())
    };
    let mut selected = None;
    let mut odds = 1;
    for (slot, occupied) in occupied.into_iter().enumerate() {
        if occupied {
            if level.random().next_int_bound(odds) == 0 {
                selected = Some(slot);
            }
            odds += 1;
        }
    }
    Ok(selected)
}
pub struct DispenserBlock;
pub static DISPENSER: DispenserBlock = DispenserBlock;
impl BlockBehavior for DispenserBlock {
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        _source: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let should_trigger =
            level.has_neighbor_signal(pos)? || level.has_neighbor_signal(pos.above(1))?;
        let triggered = state.get(p::TRIGGERED).unwrap();
        if should_trigger && !triggered {
            level.schedule_block_tick_after(pos, state.block(), 4)?;
            level.set_block(
                pos,
                state.with(p::TRIGGERED, true).unwrap(),
                UpdateFlags::CLIENTS,
                UPDATE_LIMIT,
            )?;
        } else if !should_trigger && triggered {
            level.set_block(
                pos,
                state.with(p::TRIGGERED, false).unwrap(),
                UpdateFlags::CLIENTS,
                UPDATE_LIMIT,
            )?;
        }
        Ok(())
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        let dropper = state.block() == Block::DROPPER;
        let Some(handle) = level.block_entity_ref(pos)? else {
            return Ok(());
        };
        let kind = if dropper {
            BlockEntityType::DROPPER
        } else {
            BlockEntityType::DISPENSER
        };
        if level.block_entity_state(handle)?.data().kind != kind {
            return Ok(());
        }
        level.with_block_entity_callback(handle, |level| {
            let Some(slot) = random_slot(level, handle)? else {
                effect(level, pos, 1001, 0);
                if !dropper {
                    let state = level.block_entity_state(handle)?.data().block_state;
                    level.game_event(GameEvent::BLOCK_ACTIVATE, center(pos), None, Some(state))?;
                }
                return Ok(());
            };
            let mut operation = StackOperation::new(level);
            let stack = operation
                .pin(ContainerSlot {
                    container: ContainerHandle::BlockEntity(handle),
                    slot,
                })
                .map_err(stack_error)?;
            let source = DispenseSource {
                pos,
                state,
                entity: handle,
                slot,
                stack,
            };
            let result = if dropper {
                if operation
                    .level
                    .stack(stack)
                    .map_err(stack_error)?
                    .is_empty()
                {
                    return Ok(());
                }
                let direction = facing(operation.level.get_block_state(pos)?);
                if let Some(into) =
                    hopper::container_at(operation.level, pos.relative(direction, 1))?
                {
                    let piece =
                        copy_with_count(operation.level.stack(stack).map_err(stack_error)?, 1);
                    let remaining = hopper::add_item(
                        Some(&ContainerRef::Single(handle)),
                        &into,
                        piece,
                        Some(direction.opposite()),
                        operation.level,
                    )?;
                    StackValue::New(copy_after_transfer(
                        stack,
                        remaining.is_empty(),
                        operation.level,
                    )?)
                } else {
                    default_dispense(source, operation.level)?
                }
            } else if let Some(hook) = method(
                operation.level,
                operation.level.stack(stack).map_err(stack_error)?,
            )? {
                hook(source, operation.level)?
            } else {
                return Ok(());
            };
            access::set_item_value(handle, slot, result, operation.level)
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
        if level.block_entity_state(handle)?.container().is_none() {
            return Ok(0);
        }
        access::analog_output(&ContainerRef::Single(handle), level)
    }
    fn has_analog_output_signal(&self, _state: BlockState) -> bool {
        true
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
