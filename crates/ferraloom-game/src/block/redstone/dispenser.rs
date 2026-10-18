//! DispenserBlock, DropperBlock and common dispense behaviors. Unported native
//! registry classes remain explicit errors until their concrete hooks are installed.
use super::{hopper, state::facing, storage::DispenserBlockEntity};
use crate::{
    block::{
        BlockBehavior, UPDATE_LIMIT, UpdateFlags,
        containers::{
            access::{self, ContainerRef},
            loot::unpack_loot_table,
            storage::BaseContainerBlockEntity,
        },
        entity::{BlockEntityRef, BlockEntityType},
    },
    entity::{
        EntityRef, EntityState,
        item::ItemEntity,
        player::inventory::{component, copy_with_count, count, set_count, split},
    },
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
}
/// A native behavior owns its sound/animation and operates on the canonical
/// receiver before each world callback. The result is delivered by setItem.
pub type DispenseHook = fn(DispenseSource, &mut Level) -> Result<ItemStack, LevelError>;
#[derive(Clone, Debug)]
pub struct ProjectileDispense {
    pub position: Vec3,
    pub direction: Direction,
    pub stack: ItemStack,
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
    let mut item = ItemEntity::new_at(level, position, stack);
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
/// live inventory before addFreshEntity, so reentrant comparator reads see it.
/// Keeping that original ItemStack receiver through callbacks remains pending
/// the canonical StackRef pin/read/mutate API.
pub fn default_dispense(
    source: DispenseSource,
    level: &mut Level,
) -> Result<ItemStack, LevelError> {
    let (piece, remaining) = {
        let item = &mut level
            .block_entity_mut::<BaseContainerBlockEntity>(source.entity)?
            .items[source.slot];
        let piece = split(item, 1);
        (piece, item.clone())
    };
    let direction = facing(source.state);
    let step = direction.step();
    let c = center(source.pos);
    let position = Vec3::new(
        c.x + 0.7 * f64::from(step.x),
        c.y + 0.7 * f64::from(step.y),
        c.z + 0.7 * f64::from(step.z),
    );
    spawn_item(level, piece, 6, direction, position)?;
    effect(level, source.pos, 1000, 0);
    effect(level, source.pos, 2000, direction as i32);
    Ok(remaining)
}
fn projectile_dispense(source: DispenseSource, level: &mut Level) -> Result<ItemStack, LevelError> {
    let stack = level
        .block_entity::<BaseContainerBlockEntity>(source.entity)?
        .items[source.slot]
        .clone();
    let item = stack.item();
    let config = item
        .dispenser_properties()
        .projectile
        .expect("registered projectile behavior");
    let hook = level
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
            stack,
            config,
        },
        level,
    )?;
    // Pending StackRef: shrink the original input receiver after the projectile
    // callback, including when that callback replaced the inventory slot.
    let remaining = {
        let stack = &mut level
            .block_entity_mut::<BaseContainerBlockEntity>(source.entity)?
            .items[source.slot];
        let n = count(stack).wrapping_sub(1);
        set_count(stack, n);
        stack.clone()
    };
    effect(
        level,
        source.pos,
        config.override_dispense_event.unwrap_or(1002),
        0,
    );
    effect(level, source.pos, 2000, direction as i32);
    Ok(remaining)
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
                    level.game_event(GameEvent::BLOCK_ACTIVATE, center(pos), None, Some(state));
                }
                return Ok(());
            };
            let stack = access::get_item(handle, slot, level)?;
            let source = DispenseSource {
                pos,
                state,
                entity: handle,
                slot,
            };
            let result = if dropper {
                if stack.is_empty() {
                    return Ok(());
                }
                let direction = facing(level.get_block_state(pos)?);
                if let Some(into) = hopper::container_at(level, pos.relative(direction, 1))? {
                    let remaining = hopper::add_item(
                        Some(&ContainerRef::Single(handle)),
                        &into,
                        copy_with_count(&stack, 1),
                        Some(direction.opposite()),
                        level,
                    )?;
                    // Pending StackRef: native copies the original getItem
                    // receiver here, even after reentrant destination callbacks.
                    let mut result = level
                        .block_entity::<BaseContainerBlockEntity>(handle)?
                        .items[slot]
                        .clone();
                    if remaining.is_empty() {
                        let size = count(&result).wrapping_sub(1);
                        set_count(&mut result, size);
                    }
                    result
                } else {
                    default_dispense(source, level)?
                }
            } else if let Some(hook) = method(level, &stack)? {
                hook(source, level)?
            } else {
                return Ok(());
            };
            access::set_item(handle, slot, result, level)
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
