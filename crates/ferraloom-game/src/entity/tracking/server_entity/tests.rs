use super::*;
use crate::{
    Behaviors,
    entity::{
        item::{ItemEntity, PrimedTnt},
        player::inventory,
    },
    level::Region,
};
use ferraloom_protocol::{component::ItemStack, entity_data::EntityDataValue};
use std::{collections::BTreeMap, num::NonZeroI32};

const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
fn level() -> Level {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    {
        let mut fixture_level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        fixture_level.install_shared(Box::default());
        fixture_level
    }
}
fn bits(v: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        v.x.to_bits(),
        v.y.to_bits(),
        v.z.to_bits()
    )
}
fn path(value: &PositionPath) -> String {
    match value {
        PositionPath::Linear(pos) => format!("L:{}", bits(Vec3::new(pos.x, pos.y, pos.z))),
        PositionPath::Stepped(steps) => format!(
            "S:{}",
            steps
                .iter()
                .map(|step| format!(
                    "{}:{}",
                    step.tick_offset.0,
                    bits(Vec3::new(step.position.x, step.position.y, step.position.z))
                ))
                .collect::<Vec<_>>()
                .join(";")
        ),
    }
}
fn delta(value: &VecDelta) -> String {
    match value {
        VecDelta::Linear { xa, ya, za } => format!("{xa},{ya},{za}"),
        VecDelta::Stepped(steps) => steps
            .iter()
            .map(|step| format!("{}:{},{},{}", step.ticks.0, step.xa, step.ya, step.za))
            .collect::<Vec<_>>()
            .join(";"),
    }
}
fn packet_text(packet: &Packet) -> String {
    match packet {
        Packet::AddEntity(p) => format!(
            "spawn:{},{},{}",
            p.y_rot.0 as i8, p.x_rot.0 as i8, p.y_head_rot.0 as i8
        ),
        Packet::RotateHead(p) => format!("head:{}", p.y_head_rot.0 as i8),
        Packet::UpdateAttributes(p) => format!(
            "attributes:{}",
            p.values
                .iter()
                .map(|value| format!(
                    "{}={:x}[{}]",
                    ferraloom_data::registries::attribute::Id::from_protocol_id(
                        value.attribute.0 as u16
                    )
                    .unwrap()
                    .name(),
                    value.base.to_bits(),
                    value
                        .modifiers
                        .iter()
                        .map(|modifier| format!(
                            "{}={:x}:{}",
                            modifier.id,
                            modifier.amount.to_bits(),
                            modifier.operation as i32
                        ))
                        .collect::<Vec<_>>()
                        .join(";")
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Packet::SetEquipment(p) => format!(
            "equipment:{}",
            p.slots
                .iter()
                .map(|entry| format!(
                    "{}={}:{}",
                    crate::entity::player::equipment::slot_name(entry.slot),
                    entry.item.item().name(),
                    inventory::count(&entry.item)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Packet::MoveEntityPos(p) => format!(
            "move:true:false:{}:0,0:{}",
            delta(&p.delta.delta),
            p.delta.on_ground
        ),
        Packet::MoveEntityPosRot(p) => format!(
            "move:true:true:{}:{},{}:{}",
            delta(&p.delta.delta),
            p.y_rot.0 as i8,
            p.x_rot.0 as i8,
            p.delta.on_ground
        ),
        Packet::MoveEntityRot(p) => format!(
            "move:false:true:0,0,0:{},{}:{}",
            p.y_rot.0 as i8, p.x_rot.0 as i8, p.on_ground
        ),
        Packet::EntityPositionSync(p) => format!(
            "sync:{}:{:x},{:x}:{}",
            path(&p.position),
            p.y_rot.to_bits(),
            p.x_rot.to_bits(),
            p.on_ground
        ),
        Packet::SetEntityMotion(p) => format!(
            "motion:{}",
            bits(Vec3::new(p.movement.0.x, p.movement.0.y, p.movement.0.z))
        ),
        Packet::SetEntityData(p) => format!(
            "data:{}",
            p.packed_items
                .0
                .iter()
                .map(|value| {
                    let contents = match &value.value {
                        EntityDataValue::ItemStack(stack) => {
                            format!("item={}", inventory::count(stack))
                        }
                        EntityDataValue::Float(value) => format!("{value:?}"),
                        EntityDataValue::Byte(value) => value.to_string(),
                        EntityDataValue::Int(value) => value.0.to_string(),
                        EntityDataValue::Boolean(value) => value.to_string(),
                        EntityDataValue::CowVariant(value) => value.0.to_string(),
                        EntityDataValue::CowSoundVariant(value) => value.0.to_string(),
                        _ => panic!("unexpected metadata {:?}", value.value),
                    };
                    format!("{}:{contents}", value.id)
                })
                .collect::<Vec<_>>()
                .join(",")
        ),
        Packet::SetPassengers(p) => format!(
            "passengers:{}:{}",
            p.vehicle.0,
            p.passengers
                .iter()
                .map(|id| id.0.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ),
        Packet::SetEntityLink(p) => format!("leash:{}:{}", p.source_id, p.dest_id),
        _ => panic!("unexpected tracking packet {packet:?}"),
    }
}
fn output_text(output: &[TrackingPacket]) -> String {
    if output.is_empty() {
        return "-".into();
    }
    output
        .iter()
        .map(|p| {
            format!(
                "{}:{}",
                if p.audience == TrackingAudience::Players {
                    "T"
                } else {
                    "S"
                },
                packet_text(&p.packet)
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}
fn stack(count: i32) -> ItemStack {
    ItemStack::new(ferraloom_data::Item::STONE, count, Default::default())
}

#[test]
fn real_player_pairing_and_updates_preserve_equipment_attributes_head_and_motion_order() {
    use crate::entity::{
        ai::attributes::{AttributeModifier, AttributeOperation},
        player::{GameType, ServerPlayer},
    };
    use ferraloom_data::{Item, registries::attribute::Id as Attribute};
    use ferraloom_protocol::component::EquipmentSlot;
    use std::sync::Arc;
    let mut level = level();
    let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    let mut player = ServerPlayer::new(
        ferraloom_util::Uuid::new(1, 2),
        "tracking".into(),
        9,
        Vec3::new(8.5, 64.0, 8.5),
        GameType::Survival,
    );
    player.set_y_rot(0.0);
    player.set_x_rot(0.0);
    player.y_head_rot = 10.0;
    player
        .attributes
        .get_instance(Attribute::MAX_HEALTH)
        .unwrap()
        .set_base_value(30.0);
    player.inventory.set_selected_item(stack(3));
    player.equipment.set(
        EquipmentSlot::Offhand,
        ItemStack::new(Item::SHIELD, 1, Default::default()),
    );
    player.equipment.set(
        EquipmentSlot::Chest,
        ItemStack::new(Item::IRON_CHESTPLATE, 1, Default::default()),
    );
    level
        .add_entity(id, EntityState::Player(Box::new(player)))
        .unwrap();
    let mut tracker = ServerEntity::new(
        id,
        &level,
        UpdateInterval::for_entity_type(ferraloom_data::EntityType::PLAYER),
        ferraloom_data::EntityType::PLAYER.data().track_deltas,
    )
    .unwrap();
    let pairing = tracker
        .pairing_data(&mut level)
        .unwrap()
        .iter()
        .map(packet_text)
        .collect::<Vec<_>>()
        .join("|");
    assert_eq!(
        pairing,
        VECTORS
            .lines()
            .find_map(|line| line.strip_prefix("player-pairing\t"))
            .unwrap()
    );
    let mut actual = Vec::new();
    for tick in 0..=6 {
        let player = level.entity_mut::<ServerPlayer>(id).unwrap();
        match tick {
            1 => player.y_head_rot = 23.9,
            2 => player.set_health(7.5),
            3 => {
                player.y_head_rot = 90.0;
                let mut health = player
                    .attributes
                    .get_instance(Attribute::MAX_HEALTH)
                    .unwrap();
                health.set_base_value(35.0);
                health
                    .add_transient_modifier(Arc::new(AttributeModifier {
                        id: "minecraft:bonus".parse().unwrap(),
                        amount: 3.0.into(),
                        operation: AttributeOperation::AddValue,
                    }))
                    .unwrap();
            }
            5 => {
                player.set_shared_flag(7, true);
                player.velocity = Vec3::new(0.125, 0.0, 0.0);
            }
            _ => {}
        }
        actual.push(format!(
            "player-tracking\t{tick}\t{}",
            output_text(&tracker.send_changes(&mut level).unwrap())
        ));
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|line| line.starts_with("player-tracking\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn nonliving_damage_forces_native_velocity_sync_even_without_an_update_interval() {
    use crate::entity::experience_orb::ExperienceOrb;
    let mut rows = 0;
    for row in VECTORS.lines().filter(|row| row.starts_with("hurt-sync\t")) {
        let fields: Vec<_> = row.split('\t').collect();
        let mut level = level();
        let id = level.next_entity_ref();
        let network = level.next_entity_id().unwrap();
        let state = if fields[1] == "item" {
            let mut item = ItemEntity::new(network, 123);
            item.set_item(stack(1));
            EntityState::Item(Box::new(item))
        } else {
            EntityState::ExperienceOrb(Box::new(ExperienceOrb::new(network, 123)))
        };
        level.add_entity(id, state).unwrap();
        let mut tracker = ServerEntity::new(id, &level, UpdateInterval::Never, true).unwrap();
        tracker.send_changes(&mut level).unwrap();
        level.entity_mut::<EntityData>(id).unwrap().velocity = Vec3::new(0.125, 0.25, 0.375);
        let source = level.damage_sources().generic();
        assert!(
            level
                .entity_behavior(id)
                .unwrap()
                .hurt_server(id, &mut level, source, 1.0)
                .unwrap()
        );
        let base = level.entity::<EntityData>(id).unwrap();
        assert_eq!(
            format!("{},{}", base.needs_sync, base.sync_velocity),
            fields[2],
            "{row}"
        );
        let output = tracker.send_changes(&mut level).unwrap();
        assert_eq!(output_text(&output), fields[3], "{row}");
        let base = level.entity::<EntityData>(id).unwrap();
        assert_eq!(
            format!("{},{}", base.needs_sync, base.sync_velocity),
            fields[4],
            "{row}"
        );
        rows += 1;
    }
    assert_eq!(rows, 2);
}

#[test]
fn vanilla_server_entity_packet_order_velocity_precision_mount_and_sync_flags() {
    let mut level = level();
    let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    let mount = EntityRef::fixture(crate::level::RegionId::FIXTURE, 2);
    let mut item = ItemEntity::new(2, 1);
    item.parent.set_pos(Vec3::new(8.5, 64.0, 8.5));
    item.parent.set_y_rot(0.0);
    item.set_item(stack(3));
    level
        .add_entity(id, EntityState::Item(Box::new(item)))
        .unwrap();
    level
        .add_entity(
            mount,
            EntityState::PrimedTnt(Box::new(PrimedTnt::new(3, 2))),
        )
        .unwrap();
    let mut tracker = ServerEntity::new(
        id,
        &level,
        UpdateInterval::Periodic(NonZeroI32::new(3).unwrap()),
        true,
    )
    .unwrap();
    let mut rows = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("server-entity\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let tick: i32 = fields[1].parse().unwrap();
        match tick {
            1 => level.entity_mut::<EntityData>(id).unwrap().velocity = Vec3::new(0.125, 0.0, 0.0),
            2 => {
                level
                    .set_entity_pos(id, Vec3::new(8.625, 64.0, 8.5))
                    .unwrap();
                level.entity_mut::<EntityData>(id).unwrap().set_y_rot(13.0);
            }
            4 => {
                level
                    .set_entity_pos(id, Vec3::new(8.7501, 64.0, 8.5))
                    .unwrap();
                let base = level.entity_mut::<EntityData>(id).unwrap();
                base.vertical_collision = true;
                base.needs_sync = true;
            }
            5 => level.entity_mut::<EntityData>(id).unwrap().sync_velocity = true,
            6 => level.entity_mut::<EntityData>(id).unwrap().on_ground = true,
            7 => {
                assert!(crate::entity::start_riding(id, mount, true, &mut level).unwrap());
                let base = level.entity_mut::<EntityData>(id).unwrap();
                base.set_y_rot(20.0);
                base.needs_sync = true;
            }
            8 => crate::entity::stop_riding(id, &mut level).unwrap(),
            10 => {
                level
                    .set_entity_pos(id, Vec3::new(28.7501, 64.0, 8.5))
                    .unwrap();
                level.entity_mut::<EntityData>(id).unwrap().needs_sync = true;
            }
            11 => {
                let base = level.entity_mut::<EntityData>(id).unwrap();
                base.requires_precise_position = true;
                base.needs_sync = true;
            }
            12 => {
                level
                    .entity_mut::<EntityData>(id)
                    .unwrap()
                    .requires_precise_position = false;
                level
                    .entity_mut::<ItemEntity>(id)
                    .unwrap()
                    .set_item(stack(5));
            }
            _ => {}
        }
        assert_eq!(
            output_text(&tracker.send_changes(&mut level).unwrap()),
            fields[2],
            "{row}"
        );
        rows += 1;
    }
    assert_eq!(rows, 13);
}

#[test]
fn vanilla_periodic_position_and_forced_teleport_cadence() {
    let mut level = level();
    let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    let mut tnt = PrimedTnt::new(3, 1);
    tnt.parent.set_pos(Vec3::new(0.0, 64.0, 0.0));
    level
        .add_entity(id, EntityState::PrimedTnt(Box::new(tnt)))
        .unwrap();
    let mut tracker = ServerEntity::new(
        id,
        &level,
        UpdateInterval::Periodic(NonZeroI32::new(1).unwrap()),
        true,
    )
    .unwrap();
    let expected: BTreeMap<i32, &str> = VECTORS
        .lines()
        .filter(|row| row.starts_with("server-teleport\t"))
        .map(|row| {
            let fields: Vec<_> = row.split('\t').collect();
            (fields[1].parse().unwrap(), fields[2])
        })
        .collect();
    assert_eq!(expected.len(), 8);
    for tick in 0..=401 {
        assert_eq!(
            output_text(&tracker.send_changes(&mut level).unwrap()),
            expected.get(&tick).copied().unwrap_or("-"),
            "tick {tick}"
        );
    }
}

#[test]
fn pairing_uses_cached_metadata_and_position_until_the_next_sync() {
    let mut level = level();
    let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    let mut item = ItemEntity::new(2, 1);
    item.parent.set_pos(Vec3::new(1.0, 64.0, 2.0));
    item.set_item(stack(3));
    level
        .add_entity(id, EntityState::Item(Box::new(item)))
        .unwrap();
    let mut tracker = ServerEntity::new(
        id,
        &level,
        UpdateInterval::for_entity_type(ferraloom_data::EntityType::ITEM),
        true,
    )
    .unwrap();
    level
        .entity_mut::<ItemEntity>(id)
        .unwrap()
        .set_item(stack(5));
    level.set_entity_pos(id, Vec3::new(4.0, 64.0, 2.0)).unwrap();
    let packets = tracker.pairing_data(&mut level).unwrap();
    let Packet::AddEntity(spawn) = &packets[0] else {
        panic!("expected spawn first");
    };
    assert_eq!(spawn.x, 1.0);
    assert_eq!(packet_text(&packets[1]), "data:8:item=3");
    tracker.send_changes(&mut level).unwrap();
    let packets = tracker.pairing_data(&mut level).unwrap();
    assert_eq!(packet_text(&packets[1]), "data:8:item=5");
    assert!(matches!(
        tracker.removal_packet(&level).unwrap(),
        Packet::RemoveEntities(_)
    ));
}

const LIVING_VECTORS: &str = include_str!("../../../../tests/java/entity/tracker/living.tsv");

fn cow(level: &mut Level, network: i32) -> EntityRef {
    let mut cow = crate::entity::animal::Cow::default();
    let living = cow.as_living_mut();
    living.parent.network_id = network;
    living.parent.network_id_allocated = true;
    living.parent.set_pos(Vec3::new(8.5, 64.0, 8.5));
    living.parent.yaw = 0.0;
    living.parent.pitch = 0.0;
    living.y_head_rot = 10.0;
    let id = level.next_entity_ref();
    level
        .add_entity(id, EntityState::Cow(Box::new(cow)))
        .unwrap();
    id
}
fn leash(holder: Option<EntityRef>, delayed_holder_id: i32) -> crate::entity::mob::LeashData {
    crate::entity::mob::LeashData {
        holder,
        delayed: None,
        delayed_holder_id,
        angular_momentum: 0.0,
    }
}

#[test]
fn native_cow_pairing_shares_living_snapshots_and_sends_leash_last_without_effect_packets() {
    use crate::entity::{LivingEntity, animal::Cow};
    use ferraloom_data::{Item, registries::attribute::Id as Attribute};
    use ferraloom_protocol::component::{CowVariant, EquipmentSlot};
    let mut checked = 0;
    for row in LIVING_VECTORS
        .lines()
        .filter(|row| row.starts_with("pairing\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let mut level = level();
        let id = cow(&mut level, 9);
        let other = cow(&mut level, 12);
        if matches!(fields[1], "equipment" | "all") {
            let living = level.entity_mut::<LivingEntity>(id).unwrap();
            living.equipment.set(EquipmentSlot::Mainhand, stack(3));
            living.equipment.set(
                EquipmentSlot::Offhand,
                ItemStack::new(Item::SHIELD, 1, Default::default()),
            );
            living.equipment.set(
                EquipmentSlot::Chest,
                ItemStack::new(Item::IRON_CHESTPLATE, 1, Default::default()),
            );
            living
                .attributes
                .get_instance(Attribute::MAX_HEALTH)
                .unwrap()
                .set_base_value(30.0);
        }
        if matches!(fields[1], "passengers" | "all") {
            assert!(crate::entity::start_riding(other, id, true, &mut level).unwrap());
        }
        if fields[1] == "mounted" {
            assert!(crate::entity::start_riding(id, other, true, &mut level).unwrap());
        }
        if matches!(fields[1], "leash" | "all") {
            level.entity_mut::<Cow>(id).unwrap().as_mob_mut().leash_data =
                Some(leash(Some(other), 0));
        }
        if fields[1] == "delayed" {
            level.entity_mut::<Cow>(id).unwrap().as_mob_mut().leash_data = Some(leash(None, 12));
        }
        if fields[1] == "effect" {
            let tag = ferraloom_nbt::parse_compound(
                "{active_effects:[{id:'minecraft:speed',duration:200,amplifier:1}]}",
            )
            .unwrap();
            let living = level.entity_mut::<LivingEntity>(id).unwrap();
            assert!(
                living
                    .active_effects
                    .load(tag.get("active_effects"))
                    .is_empty()
            );
            assert_eq!(living.active_effects.len(), 1);
        }
        if fields[1] == "variant" {
            level
                .entity_mut::<LivingEntity>(id)
                .unwrap()
                .entity_data
                .set(
                    crate::entity::synched_data::COW_VARIANT,
                    CowVariant("minecraft:cold".parse().unwrap()),
                );
        }
        let tracker = ServerEntity::new(
            id,
            &level,
            UpdateInterval::for_entity_type(ferraloom_data::EntityType::COW),
            true,
        )
        .unwrap();
        let actual = tracker
            .pairing_data(&mut level)
            .unwrap()
            .iter()
            .map(packet_text)
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(actual, fields[2], "{row}");
        checked += 1;
    }
    assert_eq!(checked, 9);
}

#[test]
fn native_cow_updates_use_canonical_steps_prediction_dirty_attributes_and_flying_motion() {
    use crate::entity::{
        EntityData, LivingEntity,
        movement::{MoverType, record_movement},
    };
    use ferraloom_data::registries::attribute::Id as Attribute;
    let mut checked = 0;
    for track_delta in [false, true] {
        let mut level = level();
        let id = cow(&mut level, 9);
        let mut tracker = ServerEntity::new(
            id,
            &level,
            UpdateInterval::for_entity_type(ferraloom_data::EntityType::COW),
            track_delta,
        )
        .unwrap();
        for tick in 0..16 {
            match tick {
                1 | 2 => {
                    let base = level.entity_mut::<EntityData>(id).unwrap();
                    base.set_pos(if tick == 1 {
                        Vec3::new(8.625, 64.0, 8.5)
                    } else {
                        Vec3::new(8.75, 64.125, 8.5)
                    });
                    base.sync_position = true;
                }
                3 => level.entity_mut::<LivingEntity>(id).unwrap().y_head_rot = 23.9,
                4 => level
                    .entity_mut::<LivingEntity>(id)
                    .unwrap()
                    .set_health(7.5),
                5 => level
                    .entity_mut::<LivingEntity>(id)
                    .unwrap()
                    .attributes
                    .get_instance(Attribute::MAX_HEALTH)
                    .unwrap()
                    .set_base_value(35.0),
                6 => {
                    let base = level.entity_mut::<EntityData>(id).unwrap();
                    base.set_shared_flag(7, true);
                    base.velocity = Vec3::new(0.125, 0.0, 0.0);
                }
                7 => {
                    let base = level.entity_mut::<EntityData>(id).unwrap();
                    base.set_pos(Vec3::new(8.875, 64.125, 8.5));
                    base.sync_position = true;
                }
                8 => {
                    record_movement(id, MoverType::Piston, Vec3::new(0.25, 0.0, 0.0), &mut level)
                        .unwrap();
                    level
                        .entity_mut::<EntityData>(id)
                        .unwrap()
                        .set_pos(Vec3::new(9.125, 64.125, 8.5));
                }
                10 => {
                    let base = level.entity_mut::<EntityData>(id).unwrap();
                    base.on_ground = true;
                    base.needs_sync = true;
                }
                11 => {
                    let base = level.entity_mut::<EntityData>(id).unwrap();
                    base.requires_precise_position = true;
                    base.set_pos(Vec3::new(9.25, 64.125, 8.5));
                    base.needs_sync = true;
                }
                12 => {
                    let base = level.entity_mut::<EntityData>(id).unwrap();
                    base.requires_precise_position = false;
                    base.velocity = Vec3::ZERO;
                    base.sync_velocity = true;
                }
                _ => {}
            }
            let actual = output_text(&tracker.send_changes(&mut level).unwrap());
            let prefix = format!("updates\t{track_delta}\t{tick}\t");
            let expected = LIVING_VECTORS
                .lines()
                .find_map(|row| row.strip_prefix(&prefix))
                .unwrap();
            let sync = level.entity::<EntityData>(id).unwrap().sync_position;
            assert_eq!(format!("{actual}\t{sync}"), expected, "{prefix}");
            checked += 1;
        }
    }
    assert_eq!(checked, 32);
}

#[test]
fn bound_leash_pairing_requires_the_retained_holder() {
    use crate::entity::animal::Cow;
    let mut level = level();
    let id = cow(&mut level, 9);
    let missing = EntityRef::fixture(level.region_id(), 123456);
    level.entity_mut::<Cow>(id).unwrap().as_mob_mut().leash_data = Some(leash(Some(missing), 0));
    let tracker = ServerEntity::new(id, &level, UpdateInterval::Never, true).unwrap();
    assert!(matches!(
        tracker.pairing_data(&mut level),
        Err(TrackingError::Level(_))
    ));
}
