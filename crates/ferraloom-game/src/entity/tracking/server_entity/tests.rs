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
    Level::new(Region::new(&BEHAVIORS))
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
        PositionPath::Stepped(_) => panic!("these nonliving trackers use linear paths"),
    }
}
fn delta(value: &VecDelta) -> String {
    match value {
        VecDelta::Linear { xa, ya, za } => format!("{xa},{ya},{za}"),
        VecDelta::Stepped(_) => panic!("these nonliving trackers use linear deltas"),
    }
}
fn packet_text(packet: &Packet) -> String {
    match packet {
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
                        _ => panic!("unexpected metadata {:?}", value.value),
                    };
                    format!("{}:{contents}", value.id)
                })
                .collect::<Vec<_>>()
                .join(",")
        ),
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
fn vanilla_server_entity_packet_order_velocity_precision_mount_and_sync_flags() {
    let mut level = level();
    let id = EntityRef(1);
    let mount = EntityRef(2);
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
    let id = EntityRef(1);
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
    let id = EntityRef(1);
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
