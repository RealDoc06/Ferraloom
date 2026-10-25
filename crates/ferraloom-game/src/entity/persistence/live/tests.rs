use super::*;
use crate::{
    Behaviors, ServerShared,
    block::UnportedBehavior,
    entity::{animal::Cow, item::ItemEntityBehavior, synched_data::SynchedValue},
    inventory::{ContainerHandle, ContainerSlot},
};
use ferraloom_data::{EntityType, Item};
use ferraloom_protocol::component::{CowSoundVariant, CowVariant, EquipmentSlot, ItemStack};
use ferraloom_util::{Aabb, Uuid, Vec3, random::RandomSource};
use std::cell::RefCell;

const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");

fn tag(value: &str) -> CompoundTag {
    ferraloom_nbt::parse_compound(value).unwrap()
}
fn stack(item: Item, count: i32) -> ItemStack {
    ItemStack::new(item, count, Default::default())
}
fn vector(value: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        value.x.to_bits(),
        value.y.to_bits(),
        value.z.to_bits()
    )
}
fn base_snapshot(entity: EntityRef, level: &Level) -> String {
    let state = level.entity::<EntityData>(entity).unwrap();
    let head = level
        .entity::<LivingEntity>(entity)
        .map(|living| {
            format!(
                "{:x},{:x}",
                living.y_head_rot.to_bits(),
                living.y_body_rot.to_bits()
            )
        })
        .unwrap_or_else(|_| "-".into());
    let bounds = state.bounding_box;
    [
        vector(state.position),
        vector(state.velocity),
        format!(
            "{:x},{:x},{:x},{:x}",
            state.yaw.to_bits(),
            state.pitch.to_bits(),
            state.old_yaw.to_bits(),
            state.old_pitch.to_bits()
        ),
        state.uuid.to_string(),
        state.network_id.to_string(),
        state.tick_count.to_string(),
        state.removed.to_string(),
        state.needs_sync.to_string(),
        head,
        vector(Vec3::new(bounds.min_x, bounds.min_y, bounds.min_z)),
        vector(Vec3::new(bounds.max_x, bounds.max_y, bounds.max_z)),
    ]
    .join("/")
}
fn fixture(kind: &str) -> (Level, EntityRef) {
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(UnportedBehavior::Strict);
    level.install_shared(Box::<ServerShared>::default());
    level
        .shared_mut()
        .unwrap()
        .scoreboard
        .add_player_team("loads");
    let kind = EntityType::from_name(&format!("minecraft:{kind}")).unwrap();
    let parent = EntityData::new(kind, 99, 0);
    let mut state = if kind == EntityType::COW {
        let mut cow = Cow::with_level(parent, &level).unwrap();
        cow.read_cold_data(&tag("{Age:-100}"), &level).unwrap();
        cow.as_living_mut()
            .entity_data
            .set(synched_data::MOB_FLAGS, 4);
        EntityState::Cow(Box::new(cow))
    } else if kind == EntityType::ITEM {
        let mut item = ItemEntity::with_parent(parent);
        item.set_item(stack(Item::DIAMOND, 3));
        EntityState::Item(Box::new(item))
    } else if kind == EntityType::TNT {
        EntityState::PrimedTnt(Box::new(PrimedTnt::with_parent(parent)))
    } else if kind == EntityType::FALLING_BLOCK {
        EntityState::FallingBlock(Box::new(FallingBlockEntity::with_parent(parent)))
    } else {
        assert_eq!(kind, EntityType::EXPERIENCE_ORB);
        EntityState::ExperienceOrb(Box::new(ExperienceOrb::with_parent(parent)))
    };
    let base = state.as_entity_mut();
    base.uuid = Uuid::new(0, 1);
    base.set_pos(Vec3::new(8.5, 64.0, 8.5));
    base.velocity = Vec3::new(1.0, 2.0, 3.0);
    base.set_y_rot(11.25);
    base.set_x_rot(-12.5);
    base.tick_count = 37;
    base.random.set_seed(12345);
    let entity = level.next_entity_ref();
    level.add_entity(entity, state).unwrap();
    (level, entity)
}
fn projection(kind: &str, saved: &CompoundTag) -> CompoundTag {
    let fields: &[&str] = match kind {
        "cow" => &[
            "Age",
            "ForcedAge",
            "AgeLocked",
            "InLove",
            "LoveCause",
            "Health",
            "HurtTime",
            "DeathTime",
            "AbsorptionAmount",
            "CanPickUpLoot",
            "PersistenceRequired",
            "LeftHanded",
            "NoAI",
            "drop_chances",
            "leash",
            "home_radius",
            "home_pos",
            "DeathLootTable",
            "DeathLootTableSeed",
            "variant",
            "sound_variant",
            "equipment",
        ],
        "item" => &["Health", "Age", "PickupDelay", "Owner", "Thrower", "Item"],
        "tnt" => &["fuse", "block_state", "explosion_power", "owner"],
        "falling_block" => &[
            "BlockState",
            "Time",
            "DropItem",
            "CancelDrop",
            "HurtEntities",
            "FallHurtAmount",
            "FallHurtMax",
        ],
        "experience_orb" => &["Health", "Age", "Value", "Count"],
        _ => unreachable!(),
    };
    let mut result = CompoundTag::new();
    for &field in fields {
        if let Some(value) = saved.get(field) {
            result.insert(field, value.clone());
        }
    }
    result
}

#[test]
fn native_live_load_preserves_receiver_rng_base_phases_and_full_subclass_data() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("entity-live-load\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let (mut level, entity) = fixture(fields[1]);
        let identity = std::ptr::from_ref(level.entity::<EntityData>(entity).unwrap());
        load_in_place(entity, &tag(fields[3]), &mut level).unwrap();
        assert_eq!(
            std::ptr::from_ref(level.entity::<EntityData>(entity).unwrap()),
            identity,
            "{row}"
        );
        assert_eq!(base_snapshot(entity, &level), fields[4], "{row}");
        assert_eq!(
            projection(fields[1], &level.entity_nbt_to_compare(entity).unwrap()),
            tag(fields[5]),
            "{row}"
        );
        assert_eq!(
            format!(
                "{:x}",
                level
                    .entity_mut::<EntityData>(entity)
                    .unwrap()
                    .random
                    .next_long() as u64
            ),
            fields[6],
            "{row}"
        );
        if fields[2] == "5" {
            let holder = level.entity::<EntityData>(entity).unwrap().uuid.to_string();
            assert_eq!(
                level
                    .shared()
                    .unwrap()
                    .scoreboard
                    .scoreboard()
                    .get_players_team(&holder)
                    .unwrap()
                    .name(),
                "loads"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 30);
}

#[test]
fn cow_metadata_accessors_match_native_ids_and_serializers() {
    let actual = [
        (
            "DATA_VARIANT_ID",
            synched_data::COW_VARIANT.id(),
            CowVariant("minecraft:temperate".parse().unwrap())
                .into_value()
                .serializer(),
        ),
        (
            "DATA_SOUND_VARIANT_ID",
            synched_data::COW_SOUND_VARIANT.id(),
            CowSoundVariant("minecraft:classic".parse().unwrap())
                .into_value()
                .serializer(),
        ),
        (
            "AGE_LOCKED",
            synched_data::AGE_LOCKED.id(),
            false.into_value().serializer(),
        ),
    ]
    .map(|(name, id, serializer)| {
        format!("entity-cow-accessor\t{name}\t{id}\t{}", serializer as i32)
    });
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|row| row.starts_with("entity-cow-accessor\t"))
        .collect();
    assert_eq!(actual.as_slice(), expected);
}

thread_local! {
    static CALLBACKS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
struct ReentrantItem;
fn callback(stage: &str, marker: i32, entity: EntityRef, level: &Level) {
    let item = level.entity::<ItemEntity>(entity).unwrap();
    CALLBACKS.with_borrow_mut(|calls| {
        calls.push(format!(
            "{stage}:{marker}:{}:{}:{}:{}",
            item.item().item().name(),
            crate::entity::player::inventory::count(item.item()),
            vector(item.parent.position),
            item.parent.uuid
        ))
    });
}
impl Entity for ReentrantItem {
    fn trace_entity_refs(&self, state: &EntityState, refs: &mut Vec<EntityRef>) {
        ItemEntityBehavior.trace_entity_refs(state, refs);
    }
    fn read_additional_save_data(
        &self,
        entity: EntityRef,
        input: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), EntityPersistenceError> {
        let marker = input.get_int_or("marker", 0);
        callback("before", marker, entity, level);
        if input.get_boolean_or("reenter", false) {
            let row = VECTORS
                .lines()
                .find(|row| row.starts_with("entity-live-load-reentrant\t"))
                .unwrap();
            load_in_place(entity, &tag(row.split('\t').nth(2).unwrap()), level)?;
        }
        super::read_additional_save_data(self, entity, input, level)?;
        callback("after", marker, entity, level);
        Ok(())
    }
    fn post_data_manipulated(
        &self,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        callback("post", 0, entity, level);
        Ok(())
    }
}

#[test]
fn native_reentrant_load_keeps_outer_input_and_post_observes_restored_uuid() {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_entity(EntityType::ITEM, &ReentrantItem);
    let row = VECTORS
        .lines()
        .find(|row| row.starts_with("entity-live-load-reentrant\t"))
        .unwrap();
    let fields: Vec<_> = row.split('\t').collect();
    let mut level = crate::entity::tests::level_with_behaviors(false, &BEHAVIORS);
    level.set_unported_behavior(UnportedBehavior::Strict);
    let entity = level.next_entity_ref();
    let mut item = ItemEntity::new(1, 123);
    item.parent.uuid = Uuid::new(0, 1);
    item.set_item(stack(Item::DIAMOND, 3));
    level
        .add_entity(entity, EntityState::Item(Box::new(item)))
        .unwrap();
    CALLBACKS.with_borrow_mut(Vec::clear);
    let uuid = level.entity::<EntityData>(entity).unwrap().uuid;
    load_in_place(entity, &tag(fields[1]), &mut level).unwrap();
    // EntityDataAccessor restores identity between load and postDataManipulated.
    level.entity_mut::<EntityData>(entity).unwrap().uuid = uuid;
    level
        .entity_behavior(entity)
        .unwrap()
        .post_data_manipulated(entity, &mut level)
        .unwrap();
    let calls = CALLBACKS.with_borrow_mut(std::mem::take);
    assert_eq!(calls.join(";"), fields[3]);
    assert_eq!(level.all_entities(), [entity]);
    assert_eq!(level.entity_by_uuid(uuid), Some(entity));
}

#[test]
fn live_load_publishes_section_changes_and_keeps_held_item_and_equipment_receivers() {
    let (mut level, entity) = fixture("item");
    let old = level
        .stack_ref(ContainerSlot {
            container: ContainerHandle::Entity(entity),
            slot: 0,
        })
        .unwrap();
    level.retain_stack(old).unwrap();
    load_in_place(
        entity,
        &tag("{Pos:[20d,70d,3d],Item:{id:'minecraft:stone',count:7}}"),
        &mut level,
    )
    .unwrap();
    assert_eq!(level.stack(old).unwrap().item(), Item::DIAMOND);
    assert_eq!(
        level.entity::<ItemEntity>(entity).unwrap().item().item(),
        Item::STONE
    );
    assert!(
        level
            .entities_in(
                Aabb::of_size(Vec3::new(8.5, 64.0, 8.5), 1.0, 2.0, 1.0),
                |id, _| id == entity
            )
            .is_empty()
    );
    assert_eq!(
        level.entities_in(
            Aabb::of_size(Vec3::new(20.0, 70.0, 3.0), 1.0, 2.0, 1.0),
            |id, _| id == entity
        ),
        [entity]
    );
    load_in_place(entity, &CompoundTag::new(), &mut level).unwrap();
    level.finish_entity_management();
    assert_eq!(level.stack(old).unwrap().item(), Item::DIAMOND);
    assert!(level.all_entities().is_empty());
    level.release_stack(old);
    level.finish_entity_management();
    assert!(level.entity_state(entity).is_err());

    let (mut level, entity) = fixture("cow");
    let living = level.entity_mut::<LivingEntity>(entity).unwrap();
    living
        .equipment
        .set(EquipmentSlot::Head, stack(Item::DIAMOND_HELMET, 1));
    let (generations, _, index) = living.equipment.stack_parts_mut(EquipmentSlot::Head);
    let old = generations.current(index).unwrap();
    assert!(generations.retain(old));
    load_in_place(
        entity,
        &tag("{equipment:{head:{id:'minecraft:leather_helmet',count:1}}}"),
        &mut level,
    )
    .unwrap();
    let living = level.entity_mut::<LivingEntity>(entity).unwrap();
    let (generations, current, _) = living.equipment.stack_parts_mut(EquipmentSlot::Head);
    assert_eq!(
        generations.get(old, current).unwrap().item(),
        Item::DIAMOND_HELMET
    );
    assert_eq!(current.item(), Item::LEATHER_HELMET);
    assert!(generations.release(old).is_none());
}

#[test]
fn sleeping_and_brain_loads_are_required_errors_under_both_policies() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        for (input, method) in [
            (
                "{sleeping_pos:[I;1,64,2]}",
                EntityMethod::LoadSleepingPosition,
            ),
            ("{Brain:{memories:{test:1}}}", EntityMethod::LoadBrain),
        ] {
            let (mut level, entity) = fixture("cow");
            level.set_unported_behavior(policy);
            assert!(
                matches!(load_in_place(entity, &tag(input), &mut level), Err(EntityPersistenceError::Level(LevelError::UnsupportedEntityMethod(EntityType::COW, actual))) if actual == method)
            );
            level.remove_entity(entity).unwrap();
            level.finish_entity_management();
            assert!(
                level.entity_state(entity).is_err(),
                "load error must release its owner root"
            );
        }
    }
}
