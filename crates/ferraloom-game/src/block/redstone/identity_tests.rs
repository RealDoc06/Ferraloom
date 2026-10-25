use super::{dispenser, hopper, tests::level_with_behaviors};
use crate::{
    Behaviors,
    block::{
        BlockBehavior,
        containers::{
            access::{self, ContainerRef},
            runtime::ContainerResources,
            storage::{BaseContainerBlockEntity, RandomizableContainerBlockEntity},
        },
        entity::{BlockEntityRef, create},
    },
    entity::{
        EntityRef, EntityState,
        item::ItemEntity,
        player::inventory::{count, set_count, split},
    },
    inventory::{ContainerHandle, ContainerSlot, StackOperation, StackRef, StackValue},
    level::{Level, LevelError, effects::LevelEffect, redstone::Orientation},
    loot::{LootData, LootTable},
};
use ferraloom_data::{Block, Item};
use ferraloom_protocol::{Identifier, component::ItemStack};
use ferraloom_util::{BlockPos, Vec3};
use std::{cell::RefCell, sync::Arc};

const POS: BlockPos = BlockPos::new(8, 64, 8);
const FIRST: BlockPos = BlockPos::new(6, 64, 8);
const SOURCE: BlockPos = BlockPos::new(8, 64, 10);
const LATCH: BlockPos = BlockPos::new(13, 64, 8);
static EMPTY: Behaviors = Behaviors::EMPTY;
thread_local! {
    static PLACEMENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
fn cases(prefix: &str) -> impl Iterator<Item = (&str, &str)> {
    include_str!("../../../tests/java/blocks/redstone/identity.tsv")
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(move |(name, _)| name.starts_with(prefix))
}
fn describe(stack: &ItemStack) -> String {
    format!("{},{}", stack.item().protocol_id(), count(stack))
}
fn address(handle: BlockEntityRef) -> ContainerSlot {
    ContainerSlot {
        container: ContainerHandle::BlockEntity(handle),
        slot: 0,
    }
}
fn install(level: &mut Level, pos: BlockPos, block: Block) -> BlockEntityRef {
    let state = block.default_state();
    level.set_block_state(pos, state).unwrap();
    level.set_block_entity(create(pos, state).unwrap()).unwrap();
    level.block_entity_ref(pos).unwrap().unwrap()
}
fn put(level: &mut Level, handle: BlockEntityRef, item: Item, amount: i32) {
    level
        .block_entity_mut::<BaseContainerBlockEntity>(handle)
        .unwrap()
        .replace_item(0, ItemStack::new(item, amount, Default::default()));
}
fn same_object(level: &Level, held: StackRef, slot: ContainerSlot) -> bool {
    std::ptr::eq(
        level.stack(held).unwrap(),
        level.stack(level.stack_ref(slot).unwrap()).unwrap(),
    )
}
#[derive(Clone, Copy)]
enum CallbackKind {
    Merge,
    Place,
    Restore,
    Pickup,
}
struct Callback {
    kind: CallbackKind,
    replace: bool,
}
impl BlockBehavior for Callback {
    fn neighbor_changed(
        &self,
        _state: ferraloom_data::BlockState,
        _pos: BlockPos,
        _source: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !level.get_block_state(LATCH)?.is_air() {
            return Ok(());
        }
        level.set_block_state(LATCH, Block::STONE.default_state())?;
        if matches!(self.kind, CallbackKind::Pickup) {
            let entity = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
            let mut operation = StackOperation::new(level);
            let original = operation
                .pin(ContainerSlot {
                    container: ContainerHandle::Entity(entity),
                    slot: 0,
                })
                .map_err(super::stack_error)?;
            if self.replace {
                operation
                    .level
                    .entity_mut::<ItemEntity>(entity)?
                    .set_item(ItemStack::new(Item::DIAMOND, 7, Default::default()));
            }
            let stack = operation
                .level
                .stack_mut(original)
                .map_err(super::stack_error)?;
            let amount = count(stack) + 3;
            set_count(stack, amount);
            return Ok(());
        }
        let target = if matches!(self.kind, CallbackKind::Restore) {
            POS.above(1)
        } else {
            POS
        };
        let handle = level.block_entity_ref(target)?.unwrap();
        let mut operation = StackOperation::new(level);
        let held = operation.pin(address(handle)).map_err(super::stack_error)?;
        let stack = operation
            .level
            .stack_mut(held)
            .map_err(super::stack_error)?;
        let amount = count(stack) + 2;
        set_count(stack, amount);
        if self.replace {
            access::set_item_value(
                handle,
                0,
                StackValue::New(ItemStack::new(
                    if matches!(self.kind, CallbackKind::Restore) {
                        Item::DIAMOND
                    } else {
                        Item::DIRT
                    },
                    if matches!(self.kind, CallbackKind::Restore) {
                        7
                    } else {
                        5
                    },
                    Default::default(),
                )),
                operation.level,
            )?;
        }
        if matches!(self.kind, CallbackKind::Place) {
            let stack = operation
                .level
                .stack_mut(held)
                .map_err(super::stack_error)?;
            let amount = count(stack) + 3;
            set_count(stack, amount);
            PLACEMENTS.with_borrow_mut(|trace| {
                trace.push(format!(
                    "{}:{}",
                    describe(operation.level.stack(held).unwrap()),
                    same_object(operation.level, held, address(handle)),
                ))
            });
        }
        Ok(())
    }
}
static MERGE_KEEP: Callback = Callback {
    kind: CallbackKind::Merge,
    replace: false,
};
static MERGE_REPLACE: Callback = Callback {
    kind: CallbackKind::Merge,
    replace: true,
};
static PLACE_KEEP: Callback = Callback {
    kind: CallbackKind::Place,
    replace: false,
};
static PLACE_REPLACE: Callback = Callback {
    kind: CallbackKind::Place,
    replace: true,
};
static RESTORE_KEEP: Callback = Callback {
    kind: CallbackKind::Restore,
    replace: false,
};
static RESTORE_REPLACE: Callback = Callback {
    kind: CallbackKind::Restore,
    replace: true,
};
static PICKUP_KEEP: Callback = Callback {
    kind: CallbackKind::Pickup,
    replace: false,
};
static PICKUP_REPLACE: Callback = Callback {
    kind: CallbackKind::Pickup,
    replace: true,
};
static BEHAVIORS: [Behaviors; 8] = [
    Behaviors::EMPTY.with_block(Block::COMPARATOR, &MERGE_KEEP),
    Behaviors::EMPTY.with_block(Block::COMPARATOR, &MERGE_REPLACE),
    Behaviors::EMPTY.with_block(Block::COMPARATOR, &PLACE_KEEP),
    Behaviors::EMPTY.with_block(Block::COMPARATOR, &PLACE_REPLACE),
    Behaviors::EMPTY.with_block(Block::COMPARATOR, &RESTORE_KEEP),
    Behaviors::EMPTY.with_block(Block::COMPARATOR, &RESTORE_REPLACE),
    Behaviors::EMPTY.with_block(Block::COMPARATOR, &PICKUP_KEEP),
    Behaviors::EMPTY.with_block(Block::COMPARATOR, &PICKUP_REPLACE),
];
fn arm(level: &mut Level, pos: BlockPos) {
    level
        .set_block_state(pos, Block::COMPARATOR.default_state())
        .unwrap();
    level
        .set_block_state(LATCH, Block::AIR.default_state())
        .unwrap();
}
fn lazy_first(level: &mut Level, handle: BlockEntityRef) {
    // A real lazy fill provides the synchronous isEmpty callback. Only the
    // callback's stack effects are compared with the native container fixture.
    let key = Identifier::parse("test:redstone_receiver").unwrap();
    let mut loot = LootData::default();
    loot.tables.insert(
        key.clone(),
        LootTable::decode(&serde_json::json!({
            "type":"chest", "pools":[{"rolls":1,"entries":[{"type":"item","name":"stone"}]}],
        }))
        .unwrap(),
    );
    let resources = level.container_resources().unwrap();
    level.set_container_resources(
        Arc::new(ContainerResources {
            recipes: Arc::clone(&resources.recipes),
            crafting: Arc::clone(&resources.crafting),
            loot: Arc::new(loot),
            advancements: Arc::clone(&resources.advancements),
            recipe_tags: resources.recipe_tags.clone(),
            recipe_synchronization: Arc::clone(&resources.recipe_synchronization),
        }),
        0,
    );
    let entity = level
        .block_entity_mut::<RandomizableContainerBlockEntity>(handle)
        .unwrap();
    entity.loot_table = Some(key);
    entity.loot_table_seed = 1234;
}

#[test]
fn hopper_pinned_merge_targets_and_new_split_placement_match_native_identity() {
    for (name, expected) in cases("merge:") {
        let replace = name.ends_with("true");
        let mut level = level_with_behaviors(1, &BEHAVIORS[usize::from(replace)]);
        let first = install(&mut level, FIRST, Block::BARREL);
        let target = install(&mut level, POS, Block::BARREL);
        let source = install(&mut level, SOURCE, Block::HOPPER);
        put(&mut level, target, Item::STONE, 60);
        put(&mut level, source, Item::STONE, 10);
        lazy_first(&mut level, first);
        arm(&mut level, FIRST.north(1));
        let mut operation = StackOperation::new(&mut level);
        let held = operation.pin(address(target)).unwrap();
        let input = operation.pin(address(source)).unwrap();
        let piece = split(operation.level.stack_mut(input).unwrap(), 3);
        let result = hopper::try_move_in_item(
            Some(&ContainerRef::Single(source)),
            &ContainerRef::Double([first, target]),
            piece,
            27,
            None,
            operation.level,
        )
        .unwrap();
        let current = operation.level.stack_ref(address(target)).unwrap();
        assert_eq!(
            format!(
                "{}|{}|{}|{}|{}",
                describe(operation.level.stack(input).unwrap()),
                describe(&result),
                describe(operation.level.stack(held).unwrap()),
                describe(operation.level.stack(current).unwrap()),
                same_object(operation.level, held, address(target)),
            ),
            expected,
            "{name}"
        );
    }
    for (name, expected) in cases("place:") {
        PLACEMENTS.with_borrow_mut(Vec::clear);
        let replace = name.ends_with("true");
        let mut level = level_with_behaviors(1, &BEHAVIORS[2 + usize::from(replace)]);
        let target = install(&mut level, POS, Block::BARREL);
        let source = install(&mut level, SOURCE, Block::HOPPER);
        put(&mut level, source, Item::STONE, 10);
        arm(&mut level, POS.north(1));
        let mut operation = StackOperation::new(&mut level);
        let old_empty = operation.pin(address(target)).unwrap();
        let input = operation.pin(address(source)).unwrap();
        let piece = split(operation.level.stack_mut(input).unwrap(), 3);
        let result = hopper::add_item(
            Some(&ContainerRef::Single(source)),
            &ContainerRef::Single(target),
            piece,
            None,
            operation.level,
        )
        .unwrap();
        assert!(
            !same_object(operation.level, old_empty, address(target)),
            "{name} new split object"
        );
        let current = operation.level.stack_ref(address(target)).unwrap();
        assert_eq!(
            format!(
                "{}|{}|{}|{}",
                describe(operation.level.stack(input).unwrap()),
                describe(&result),
                describe(operation.level.stack(current).unwrap()),
                PLACEMENTS.with_borrow(|trace| trace.join(";")),
            ),
            expected,
            "{name}"
        );
    }
}

#[test]
fn hopper_failed_pull_restores_original_receiver_after_slot_replacement() {
    for (name, expected) in cases("restore:") {
        let mut parts = name.split(':').skip(1);
        let amount = parts.next().unwrap().parse().unwrap();
        let replace = parts.next().unwrap().parse::<bool>().unwrap();
        let mut level = level_with_behaviors(1, &BEHAVIORS[4 + usize::from(replace)]);
        let hopper = install(&mut level, POS, Block::HOPPER);
        let source = install(&mut level, POS.above(1), Block::BARREL);
        for slot in 0..5 {
            level
                .block_entity_mut::<BaseContainerBlockEntity>(hopper)
                .unwrap()
                .replace_item(slot, ItemStack::new(Item::STONE, 64, Default::default()));
        }
        put(&mut level, source, Item::STONE, amount);
        arm(&mut level, POS.above(1).north(1));
        let mut operation = StackOperation::new(&mut level);
        let original = operation.pin(address(source)).unwrap();
        let success = hopper::suck_in_items(operation.level, hopper).unwrap();
        let current = operation.level.stack_ref(address(source)).unwrap();
        assert_eq!(
            format!(
                "{success}|{}|{}|{}",
                describe(operation.level.stack(original).unwrap()),
                describe(operation.level.stack(current).unwrap()),
                same_object(operation.level, original, address(source)),
            ),
            expected,
            "{name}"
        );
    }
}

#[test]
fn hopper_item_entity_copy_precedes_callbacks_and_original_survives_removal() {
    for (name, expected) in cases("pickup:") {
        let replace = name.ends_with("true");
        let mut level = level_with_behaviors(1, &BEHAVIORS[6 + usize::from(replace)]);
        let hopper = install(&mut level, POS, Block::HOPPER);
        let entity = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        let item = ItemEntity::new_at(
            &mut level,
            Vec3::new(8.5, 65.0, 8.5),
            ItemStack::new(Item::STONE, 4, Default::default()),
        )
        .unwrap();
        level
            .add_entity(entity, EntityState::Item(Box::new(item)))
            .unwrap();
        arm(&mut level, POS.north(1));
        let mut operation = StackOperation::new(&mut level);
        let held = operation
            .pin(ContainerSlot {
                container: ContainerHandle::Entity(entity),
                slot: 0,
            })
            .unwrap();
        let success = hopper::take_item_entity(operation.level, hopper, entity).unwrap();
        let current = operation.level.entity::<ItemEntity>(entity).unwrap();
        let target = operation.level.stack_ref(address(hopper)).unwrap();
        assert_eq!(
            format!(
                "{success}|{}|{}|{}|{}",
                describe(operation.level.stack(held).unwrap()),
                describe(current.item()),
                current.parent.removed,
                describe(operation.level.stack(target).unwrap())
            ),
            expected,
            "{name}"
        );
    }
}

fn mutate_dispensed(
    source: dispenser::DispenseSource,
    replace: bool,
    level: &mut Level,
) -> Result<(), LevelError> {
    if replace {
        access::set_item(
            source.entity,
            source.slot,
            ItemStack::new(Item::DIRT, 7, Default::default()),
            level,
        )?;
    }
    let stack = level.stack_mut(source.stack).map_err(super::stack_error)?;
    let amount = count(stack) + 3;
    set_count(stack, amount);
    Ok(())
}
fn default_keep(
    source: dispenser::DispenseSource,
    level: &mut Level,
) -> Result<StackValue, LevelError> {
    let result = dispenser::default_dispense(source, level)?;
    mutate_dispensed(source, false, level)?;
    Ok(result)
}
fn default_replace(
    source: dispenser::DispenseSource,
    level: &mut Level,
) -> Result<StackValue, LevelError> {
    let result = dispenser::default_dispense(source, level)?;
    mutate_dispensed(source, true, level)?;
    Ok(result)
}
fn projectile_callback(
    projectile: dispenser::ProjectileDispense,
    replace: bool,
    level: &mut Level,
) -> Result<EntityRef, LevelError> {
    let source = dispenser::DispenseSource {
        pos: POS,
        state: level.get_block_state(POS)?,
        entity: level.block_entity_ref(POS)?.unwrap(),
        slot: 0,
        stack: projectile.stack,
    };
    mutate_dispensed(source, replace, level)?;
    // The concrete projectile factory is outside this receiver-boundary test.
    Ok(EntityRef::fixture(crate::level::RegionId::FIXTURE, 0))
}
fn projectile_keep(
    projectile: dispenser::ProjectileDispense,
    level: &mut Level,
) -> Result<EntityRef, LevelError> {
    projectile_callback(projectile, false, level)
}
fn projectile_replace(
    projectile: dispenser::ProjectileDispense,
    level: &mut Level,
) -> Result<EntityRef, LevelError> {
    projectile_callback(projectile, true, level)
}

#[test]
fn dispenser_and_projectile_callbacks_return_the_original_receiver_to_set_item() {
    for prefix in ["dispenser:", "projectile:"] {
        for (name, expected) in cases(prefix) {
            let replace = name.ends_with("true");
            let mut level = level_with_behaviors(1, &EMPTY);
            let handle = install(&mut level, POS, Block::DISPENSER);
            put(
                &mut level,
                handle,
                if prefix == "projectile:" {
                    Item::ARROW
                } else {
                    Item::STONE
                },
                4,
            );
            if prefix == "projectile:" {
                dispenser::set_projectile_spawn_hook(
                    &mut level,
                    if replace {
                        projectile_replace
                    } else {
                        projectile_keep
                    },
                );
            } else {
                dispenser::register_behavior(
                    &mut level,
                    Item::STONE,
                    if replace {
                        default_replace
                    } else {
                        default_keep
                    },
                );
            }
            let mut operation = StackOperation::new(&mut level);
            let original = operation.pin(address(handle)).unwrap();
            operation.level.take_effects();
            dispenser::DISPENSER
                .tick(
                    operation.level.get_block_state(POS).unwrap(),
                    POS,
                    operation.level,
                )
                .unwrap();
            let events = operation
                .level
                .take_effects()
                .into_iter()
                .filter_map(|effect| match effect {
                    LevelEffect::LevelEvent { id, .. } => Some(id),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let current = operation.level.stack_ref(address(handle)).unwrap();
            assert_eq!(
                format!(
                    "{}|{}|{}|{events:?}",
                    describe(operation.level.stack(original).unwrap()),
                    describe(operation.level.stack(current).unwrap()),
                    same_object(operation.level, original, address(handle))
                ),
                expected,
                "{name}"
            );
        }
    }
}

#[test]
fn dropper_post_callback_copies_use_the_held_receiver_and_create_a_new_object() {
    for (name, expected) in cases("dropper:") {
        let mut parts = name.split(':').skip(1);
        let transferred = parts.next().unwrap().parse::<bool>().unwrap();
        let replace = parts.next().unwrap().parse::<bool>().unwrap();
        let mut level = level_with_behaviors(1, &EMPTY);
        let handle = install(&mut level, POS, Block::DROPPER);
        put(&mut level, handle, Item::STONE, 4);
        let mut operation = StackOperation::new(&mut level);
        let original = operation.pin(address(handle)).unwrap();
        // These mutations are the destination callback's effects captured by
        // the real native DropperBlock tick, immediately before its copy branch.
        let source = dispenser::DispenseSource {
            pos: POS,
            state: operation.level.get_block_state(POS).unwrap(),
            entity: handle,
            slot: 0,
            stack: original,
        };
        mutate_dispensed(source, replace, operation.level).unwrap();
        let result =
            dispenser::copy_after_transfer(original, transferred, operation.level).unwrap();
        access::set_item_value(handle, 0, StackValue::New(result), operation.level).unwrap();
        let current = operation.level.stack_ref(address(handle)).unwrap();
        assert_eq!(
            format!(
                "{}|{}|{}",
                describe(operation.level.stack(original).unwrap()),
                describe(operation.level.stack(current).unwrap()),
                same_object(operation.level, original, address(handle))
            ),
            expected,
            "{name}"
        );
    }
}
