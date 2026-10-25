//! Native EntityEquipmentPredicate, EntityFlagsPredicate and position facts.
use super::{
    EntityData, EntityRef, EntityState, LivingEntity,
    animal::Cow,
    experience_orb::ExperienceOrb,
    item::{ItemEntity, PrimedTnt},
    player::{GameType, ServerPlayer, equipment::slot_name},
};
use crate::{
    level::{Level, LevelError},
    predicates::{EntityPredicate, PredicateContext},
};
use ferraloom_data::{BlockState, EntityType, Item};
use ferraloom_protocol::{
    codec::HolderSet,
    component::{DamageTypeSet, DataComponent, DataComponentPatch, EquipmentSlot, ItemStack},
    entity_data::Pose,
};
use ferraloom_util::{BlockPos, Uuid, Vec3};
use std::cell::RefCell;

const VECTORS: &str = include_str!("../../tests/java/entity/vectors.tsv");
const SLOTS: [EquipmentSlot; 7] = [
    EquipmentSlot::Head,
    EquipmentSlot::Chest,
    EquipmentSlot::Legs,
    EquipmentSlot::Feet,
    EquipmentSlot::Body,
    EquipmentSlot::Mainhand,
    EquipmentSlot::Offhand,
];
fn rows(prefix: &str) -> impl Iterator<Item = Vec<&str>> {
    VECTORS
        .lines()
        .filter(move |row| row.starts_with(prefix))
        .map(|row| row.split('\t').collect())
}
fn spawn(level: &mut Level, kind: &str) -> EntityRef {
    let id = level.next_entity_ref();
    let state = match kind {
        "player" => EntityState::Player(Box::new(ServerPlayer::new(
            Uuid::new(0, id.serial() as i64),
            "facts".into(),
            level.next_entity_id().unwrap(),
            Vec3::ZERO,
            GameType::Survival,
        ))),
        "cow" => EntityState::Cow(Box::new(Cow::with_parent(
            level.create_entity_data(EntityType::COW).unwrap(),
        ))),
        "orb" => EntityState::ExperienceOrb(Box::new(ExperienceOrb::with_parent(
            level
                .create_entity_data(EntityType::EXPERIENCE_ORB)
                .unwrap(),
        ))),
        "item" => EntityState::Item(Box::new(ItemEntity::with_parent(
            level.create_entity_data(EntityType::ITEM).unwrap(),
        ))),
        _ => EntityState::PrimedTnt(Box::new(PrimedTnt::with_parent(
            level.create_entity_data(EntityType::TNT).unwrap(),
        ))),
    };
    level.add_entity(id, state).unwrap();
    id
}
fn stack(item: Item, count: i32) -> ItemStack {
    let mut stack = ItemStack::new(item, 1, DataComponentPatch::default());
    // Keep the native retained payload at count <= 0 to test effective reads.
    stack.0.as_mut().unwrap().count = count;
    stack
}

#[test]
fn native_careful_stepping_reads_shift_independently_of_crouching_pose() {
    let mut cases = 0;
    for fields in rows("entity-careful-stepping\t") {
        let mut level = super::tests::level(false);
        let entity = spawn(&mut level, fields[1]);
        level
            .entity_mut::<EntityData>(entity)
            .unwrap()
            .set_shared_flag(1, fields[2].parse().unwrap());
        super::dimensions::set_pose(
            entity,
            if fields[3] == "true" {
                Pose::Crouching
            } else {
                Pose::Standing
            },
            &mut level,
        )
        .unwrap();
        assert_eq!(
            level
                .entity_behavior(entity)
                .unwrap()
                .is_stepping_carefully(entity, &level)
                .unwrap(),
            fields[4].parse::<bool>().unwrap(),
            "{fields:?}",
        );
        cases += 1;
    }
    assert_eq!(cases, 12);
}
fn equip(level: &mut Level, id: EntityRef, slot: EquipmentSlot, stack: ItemStack) {
    if slot == EquipmentSlot::Mainhand
        && let Ok(player) = level.entity_mut::<ServerPlayer>(id)
    {
        player.inventory.set_selected_item(stack);
    } else {
        level
            .entity_mut::<LivingEntity>(id)
            .unwrap()
            .equipment
            .set(slot, stack);
    }
}
fn equipment_test(slots: &[EquipmentSlot]) -> EntityPredicate {
    let equipment: serde_json::Map<_, _> = slots
        .iter()
        .map(|slot| {
            (
                slot_name(*slot).to_owned(),
                serde_json::json!({"items":"minecraft:leather_boots"}),
            )
        })
        .collect();
    EntityPredicate::decode(&serde_json::json!({"minecraft:equipment":equipment})).unwrap()
}

#[test]
fn native_builtin_leather_boots_location_predicate_uses_live_equipment_and_support() {
    use ferraloom_data::datapack::{Pack, ResourceManager};
    use std::path::Path;
    let jar =
        std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla resources");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
    let advancement = resources
        .require(
            &"minecraft:advancement/adventure/walk_on_powder_snow_with_leather_boots.json"
                .parse()
                .unwrap(),
        )
        .unwrap()
        .json()
        .unwrap();
    let predicate = EntityPredicate::decode(
        &advancement["criteria"]["walk_on_powder_snow_with_leather_boots"]["conditions"]["player"]
            ["predicate"],
    )
    .unwrap();
    let mut level = super::tests::level(false);
    let mut checked = 0;
    for row in rows("entity-leather-boots-location\t") {
        let test: i32 = row[1].parse().unwrap();
        let entity = spawn(&mut level, "player");
        equip(
            &mut level,
            entity,
            if test == 4 {
                EquipmentSlot::Offhand
            } else {
                EquipmentSlot::Feet
            },
            stack(
                if test == 1 {
                    Item::DIAMOND_BOOTS
                } else {
                    Item::LEATHER_BOOTS
                },
                if test == 2 {
                    0
                } else if test == 3 {
                    -1
                } else {
                    1
                },
            ),
        );
        let support = BlockPos::new(7, 63, 9);
        level
            .set_block_state(
                support,
                if test == 6 {
                    ferraloom_data::Block::STONE
                } else {
                    ferraloom_data::Block::POWDER_SNOW
                }
                .default_state(),
            )
            .unwrap();
        level
            .set_block_state(
                BlockPos::new(8, 63, 8),
                if test == 7 {
                    ferraloom_data::Block::POWDER_SNOW
                } else {
                    ferraloom_data::Block::STONE
                }
                .default_state(),
            )
            .unwrap();
        let base = level.entity_mut::<EntityData>(entity).unwrap();
        base.set_pos(Vec3::new(8.5, 64.0, 8.5));
        base.on_ground = test != 5;
        base.supporting_block_pos = (test != 7).then_some(support);
        assert_eq!(
            position(level.entity_on_pos(entity).unwrap()),
            row[2],
            "{row:?}"
        );
        assert_eq!(
            predicate.matches(Some(entity), None, &level).unwrap(),
            row[3] == "true",
            "{row:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 8);
}

#[test]
fn native_equipment_reads_selected_mainhand_and_hides_retained_empty_payloads() {
    let mut level = super::tests::level(false);
    let mut count = 0;
    for row in rows("entity-equipment-fact\t") {
        let id = spawn(&mut level, row[1]);
        let slot = SLOTS
            .into_iter()
            .find(|slot| slot_name(*slot) == row[2])
            .unwrap();
        if row[1] == "player" {
            level
                .entity_mut::<ServerPlayer>(id)
                .unwrap()
                .inventory
                .set_selected_slot(4);
        }
        if row[1] != "tnt" {
            equip(
                &mut level,
                id,
                slot,
                stack(Item::LEATHER_BOOTS, row[3].parse().unwrap()),
            );
        }
        let item = level
            .entity_equipment_slot(id, slot)
            .unwrap()
            .unwrap_or_default();
        assert_eq!(item.item().name(), row[4], "{row:?}");
        assert_eq!(
            equipment_test(&[slot])
                .matches(Some(id), None, &level)
                .unwrap(),
            row[5] == "true",
            "{row:?}"
        );
        count += 1;
    }
    assert_eq!(count, 63);
    let tnt = spawn(&mut level, "tnt");
    assert!(
        !equipment_test(&[])
            .matches(Some(tnt), None, &level)
            .unwrap()
    );
}

struct EquipmentReads<'a> {
    level: &'a Level,
    reads: RefCell<Vec<EquipmentSlot>>,
}
impl PredicateContext for EquipmentReads<'_> {
    fn entity_has_equipment(&self, entity: EntityRef) -> crate::loot::Result<bool> {
        self.level.entity_has_equipment(entity)
    }
    fn entity_equipment_slot(
        &self,
        entity: EntityRef,
        slot: EquipmentSlot,
    ) -> crate::loot::Result<Option<ItemStack>> {
        self.reads.borrow_mut().push(slot);
        self.level.entity_equipment_slot(entity, slot)
    }
}
#[test]
fn native_equipment_predicate_reads_only_needed_slots_in_codec_order() {
    let mut level = super::tests::level(false);
    let mut count = 0;
    for row in rows("entity-equipment-order\t") {
        let id = spawn(&mut level, "cow");
        let failure: i32 = row[1].parse().unwrap();
        for (index, slot) in SLOTS.into_iter().enumerate() {
            equip(
                &mut level,
                id,
                slot,
                stack(
                    if index as i32 == failure {
                        Item::STONE
                    } else {
                        Item::LEATHER_BOOTS
                    },
                    1,
                ),
            );
        }
        let facts = EquipmentReads {
            level: &level,
            reads: RefCell::default(),
        };
        assert_eq!(
            equipment_test(if failure == -2 { &[] } else { &SLOTS })
                .matches(Some(id), None, &facts)
                .unwrap(),
            row[2] == "true"
        );
        let reads = facts
            .reads
            .into_inner()
            .into_iter()
            .map(slot_name)
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(if reads.is_empty() { "-" } else { &reads }, row[3]);
        count += 1;
    }
    assert_eq!(count, 9);
}

fn position(pos: BlockPos) -> String {
    format!("{},{},{}", pos.x, pos.y, pos.z)
}
#[test]
fn native_supporting_and_movement_positions_drive_predicates() {
    let mut level = super::tests::level(false);
    let stepping = EntityPredicate::decode(
        &serde_json::json!({"minecraft:stepping_on":{"position":{"x":7.5,"y":63.5,"z":9.5}}}),
    )
    .unwrap();
    let movement = EntityPredicate::decode(&serde_json::json!({"minecraft:movement_affected_by":{"position":{"x":7.5,"y":63.5,"z":9.5}}})).unwrap();
    let mut count = 0;
    for row in rows("entity-position-fact\t") {
        let id = spawn(&mut level, row[1]);
        let y = f64::from_bits(u64::from_str_radix(row[3], 16).unwrap());
        let support = BlockPos::new(7, 63, 9);
        let state: i32 = row[2].parse().unwrap();
        if state >= 0 {
            level
                .set_block_state(support, BlockState::from_id(state as u16).unwrap())
                .unwrap();
        }
        let base = level.entity_mut::<EntityData>(id).unwrap();
        base.set_pos(Vec3::new(8.5, y, 8.5));
        base.on_ground = row[4] == "true";
        base.supporting_block_pos = (state >= 0).then_some(support);
        assert_eq!(
            position(level.entity_on_pos(id).unwrap()),
            row[5],
            "{row:?}"
        );
        assert_eq!(
            position(super::movement::get_on_pos(id, 0.2, &level).unwrap()),
            row[6],
            "{row:?}"
        );
        assert_eq!(
            position(level.entity_movement_block_pos(id).unwrap()),
            row[7],
            "{row:?}"
        );
        assert_eq!(
            position(super::movement::get_on_pos(id, f32::NAN, &level).unwrap()),
            row[8],
            "{row:?}"
        );
        assert_eq!(
            stepping.matches(Some(id), None, &level).unwrap(),
            row[9] == "true",
            "{row:?}"
        );
        assert_eq!(
            movement.matches(Some(id), None, &level).unwrap(),
            row[10] == "true",
            "{row:?}"
        );
        count += 1;
    }
    assert_eq!(count, 30);
    let id = spawn(&mut level, "tnt");
    let missing = BlockPos::new(1600, 63, 1600);
    level
        .entity_mut::<EntityData>(id)
        .unwrap()
        .supporting_block_pos = Some(missing);
    assert_eq!(level.entity_on_pos(id).unwrap(), missing);
    assert!(matches!(
        level.entity_movement_block_pos(id),
        Err(crate::loot::LootError::Unsupported(_))
    ));
}

#[test]
fn native_flags_use_living_virtuals_player_abilities_and_effective_fire_resistance() {
    let mut level = super::tests::level(false);
    let mut count = 0;
    for row in rows("entity-flags-fact\t") {
        let scenario = row[1];
        let kind = if scenario.starts_with("item") {
            "item"
        } else if scenario.starts_with("player") {
            "player"
        } else if scenario.starts_with("cow") {
            "cow"
        } else {
            "tnt"
        };
        let id = spawn(&mut level, kind);
        if kind == "item" {
            let item = if matches!(scenario, "item-stone" | "item-direct" | "item-other") {
                Item::STONE
            } else {
                Item::NETHERITE_INGOT
            };
            let mut stack = stack(
                item,
                match scenario {
                    "item-zero" => 0,
                    "item-negative" => -1,
                    _ => 1,
                },
            );
            if matches!(scenario, "item-direct" | "item-other") {
                stack
                    .0
                    .as_mut()
                    .unwrap()
                    .components
                    .set(DataComponent::DamageResistant(DamageTypeSet(
                        HolderSet::Direct(vec![
                            if scenario == "item-direct" {
                                "minecraft:in_fire"
                            } else {
                                "minecraft:lava"
                            }
                            .parse()
                            .unwrap(),
                        ]),
                    )));
            }
            level.entity_mut::<ItemEntity>(id).unwrap().set_item(stack);
        }
        if scenario == "player-flying" {
            level
                .entity_mut::<ServerPlayer>(id)
                .unwrap()
                .abilities
                .flying = true;
        }
        if scenario == "player-spectator" {
            level.entity_mut::<ServerPlayer>(id).unwrap().game_mode = GameType::Spectator;
        }
        if scenario == "cow-baby" {
            level
                .entity_mut::<Cow>(id)
                .unwrap()
                .parent
                .parent
                .parent
                .age = -1;
        }
        let base = level.entity_mut::<EntityData>(id).unwrap();
        base.on_ground = true;
        base.remaining_fire_ticks = 20;
        base.entity_data
            .set(super::synched_data::POSE, Pose::Crouching);
        base.set_shared_flag(3, true);
        base.set_shared_flag(4, true);
        base.set_shared_flag(7, scenario.ends_with("glide"));
        base.was_touching_water = true;
        let flags = level.entity_flags(id).unwrap();
        let actual = [
            flags.on_ground,
            flags.on_fire,
            flags.crouching,
            flags.sprinting,
            flags.swimming,
            flags.flying,
            flags.baby,
            flags.living,
            flags.in_water,
            flags.fall_flying,
        ]
        .map(|v| v.to_string())
        .join(",");
        assert_eq!(actual, row[2], "{scenario}");
        assert_eq!(
            level
                .entity_behavior(id)
                .unwrap()
                .fire_immune(id, &level)
                .unwrap(),
            row[3] == "true",
            "{scenario}"
        );
        count += 1;
    }
    assert_eq!(count, 14);
    let id = spawn(&mut level, "item");
    let mut stack = stack(Item::STONE, 1);
    stack
        .0
        .as_mut()
        .unwrap()
        .components
        .set(DataComponent::DamageResistant(DamageTypeSet(
            HolderSet::Named("test:missing".parse().unwrap()),
        )));
    level.entity_mut::<ItemEntity>(id).unwrap().set_item(stack);
    assert_eq!(
        level.entity_behavior(id).unwrap().fire_immune(id, &level),
        Err(LevelError::MissingDamageTypeTag)
    );
}

#[test]
fn present_empty_damage_tag_does_not_grant_fire_immunity() {
    use crate::{
        Behaviors,
        entity::damage::DamageTypeRegistry,
        level::{LevelContext, Region},
    };
    use ferraloom_data::datapack::{Pack, ResourceManager};
    use std::{path::Path, sync::Arc};
    let jar =
        std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla resources");
    let mut pack = Pack::new("empty resistance");
    pack.insert(
        "test:tags/damage_type/empty.json".parse().unwrap(),
        br#"{"values":[]}"#.as_slice(),
    );
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap(), pack]);
    let services = LevelContext::fixture(0, 63);
    let context = Arc::new(LevelContext::new(
        0,
        63,
        services.generator.clone(),
        services.features.clone(),
        services.structures.clone(),
        Arc::new(DamageTypeRegistry::load(&resources).unwrap()),
        services.initial_difficulty,
    ));
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = {
        let mut fixture_level = Level::new(Region::new(&BEHAVIORS), context);
        fixture_level.install_shared(Box::default());
        fixture_level
    };
    let id = spawn(&mut level, "item");
    let mut stack = stack(Item::STONE, 1);
    stack
        .0
        .as_mut()
        .unwrap()
        .components
        .set(DataComponent::DamageResistant(DamageTypeSet(
            HolderSet::Named("test:empty".parse().unwrap()),
        )));
    level.entity_mut::<ItemEntity>(id).unwrap().set_item(stack);
    assert!(
        !level
            .entity_behavior(id)
            .unwrap()
            .fire_immune(id, &level)
            .unwrap()
    );
}
