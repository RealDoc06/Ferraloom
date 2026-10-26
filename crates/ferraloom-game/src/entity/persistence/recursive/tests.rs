use super::*;
use crate::{
    entity::{EntityData, EntitySpawnReason, EntityState, item::PrimedTnt, teleport},
    level::entity::EntityManagementEvent,
};
use ferraloom_util::{Uuid, Vec3};

fn request() -> EntitySpawnRequest {
    EntitySpawnRequest {
        reason: EntitySpawnReason::Command,
        ignore_checks: false,
    }
}
fn vector(value: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        value.x.to_bits(),
        value.y.to_bits(),
        value.z.to_bits()
    )
}
fn tree(entity: EntityRef, level: &Level, rows: &mut Vec<String>) {
    let base = level.entity::<EntityData>(entity).unwrap();
    rows.push(format!(
        "{}:{}:{}:{}:{}:{}:{}:{}:{:x},{:x}",
        base.network_id,
        base.entity_type.name(),
        base.uuid.least,
        base.vehicle
            .get()
            .map(|id| level
                .entity::<EntityData>(id)
                .unwrap()
                .network_id
                .to_string())
            .unwrap_or_else(|| "-".into()),
        base.passengers
            .iter()
            .map(|id| level
                .entity::<EntityData>(*id)
                .unwrap()
                .network_id
                .to_string())
            .collect::<Vec<_>>()
            .join("/"),
        base.removed,
        vector(base.position),
        vector(base.velocity),
        base.yaw.to_bits(),
        base.pitch.to_bits()
    ));
    for &passenger in &base.passengers {
        tree(passenger, level, rows);
    }
}

#[test]
fn recursive_load_and_fresh_add_match_native_constructor_callback_riding_and_uuid_order() {
    let mut count = 0;
    for line in include_str!("../../../../tests/java/entity/summon/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("entity-recursive\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let scenario = fields[1];
        let mut level = crate::entity::tests::persistence_level();
        level.set_difficulty(if fields[3] == "0" {
            Difficulty::Peaceful
        } else {
            Difficulty::Normal
        });
        let existing = level.next_entity_ref();
        let mut state = PrimedTnt::with_parent(level.create_entity_data(EntityType::TNT).unwrap());
        state.parent.uuid = Uuid::new(0, 99);
        level
            .add_entity(existing, EntityState::PrimedTnt(Box::new(state)))
            .unwrap();
        level.take_entity_management_events();
        let mut tag = ferraloom_nbt::parse_compound(fields[2]).unwrap();
        if scenario == "invalid-position" {
            // DoubleTag's NaNd text reparses as a string, not a numeric NaN.
            // Restore the actual typed payload constructed by the Java fixture.
            tag.insert(
                "Pos",
                Tag::List(ferraloom_nbt::ListTag(vec![
                    Tag::Double(f64::NAN),
                    Tag::Double(64.0),
                    Tag::Double(0.0),
                ])),
            );
        }
        let mut callbacks = Vec::new();
        let root = load_entity_recursive(&tag, request(), &mut level, |id, level| {
            let base = level.entity::<EntityData>(id)?;
            callbacks.push(format!(
                "{}:{}:{}:{}:{}:{}",
                base.network_id,
                base.entity_type.name(),
                base.uuid.least,
                base.passengers.len(),
                base.vehicle.get().is_some(),
                base.removed
            ));
            let (yaw, pitch) = (base.yaw, base.pitch);
            teleport::abs_snap_to(id, Vec3::new(8.5, 70.0, 8.5), yaw, pitch, level)?;
            // Native post-load callbacks may acquire chunks and run management.
            // The retained root and every linked child must remain canonical.
            level.finish_entity_management();
            Ok(())
        })
        .unwrap();
        assert_eq!(
            if callbacks.is_empty() {
                "-".into()
            } else {
                callbacks.join(";")
            },
            fields[4],
            "{scenario}: callbacks"
        );
        let mut rows = Vec::new();
        if let Some(root) = root {
            tree(root, &level, &mut rows);
        }
        assert_eq!(
            if rows.is_empty() {
                "-".into()
            } else {
                rows.join(";")
            },
            fields[5],
            "{scenario}: tree"
        );
        let added = root
            .map(|root| {
                level
                    .try_add_fresh_entity_with_passengers(root)
                    .unwrap()
                    .to_string()
            })
            .unwrap_or_else(|| "none".into());
        assert_eq!(added, fields[6], "{scenario}: add");
        let created = level
            .take_entity_management_events()
            .into_iter()
            .filter_map(|event| {
                if let EntityManagementEvent::Created(id) = event {
                    Some(
                        level
                            .entity::<EntityData>(id)
                            .unwrap()
                            .network_id
                            .to_string(),
                    )
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            if created.is_empty() {
                "-".into()
            } else {
                created.join(",")
            },
            fields[7],
            "{scenario}: registration"
        );
        assert_eq!(
            level.next_entity_id().unwrap() - 1,
            fields[8].parse::<i32>().unwrap(),
            "{scenario}: constructors"
        );
        if let Some(root) = root {
            level.release_entity(root);
            level.finish_entity_management();
            assert_eq!(
                level.entity_state(root).is_ok(),
                added == "true",
                "{scenario}: retained tree"
            );
        }
        count += 1;
    }
    assert_eq!(count, 10);
}

#[test]
fn recursive_callback_failure_releases_root_and_required_factories_are_explicit() {
    let mut level = crate::entity::tests::persistence_level();
    let tag = ferraloom_nbt::parse_compound("{id:'minecraft:tnt'}").unwrap();
    let mut captured = None;
    let result = load_entity_recursive(&tag, request(), &mut level, |entity, _| {
        captured = Some(entity);
        Err(LevelError::Inventory("post-load callback failed"))
    });
    assert!(matches!(
        result,
        Err(EntityPersistenceError::Level(LevelError::Inventory(
            "post-load callback failed"
        )))
    ));
    level.finish_entity_management();
    assert!(level.entity_state(captured.unwrap()).is_err());
    let tag = ferraloom_nbt::parse_compound("{id:'minecraft:zombie'}").unwrap();
    assert!(
        matches!(load_entity_recursive(&tag, request(), &mut level, |_, _| Ok(())), Err(EntityPersistenceError::UnsupportedType(name)) if name == "minecraft:zombie")
    );
}

#[test]
fn single_root_creation_and_transform_virtuals_match_native_without_loading_passengers() {
    use ferraloom_util::{Mirror, Rotation};
    fn bits(value: f32) -> u32 {
        if value.is_nan() {
            0x7fc00000
        } else {
            value.to_bits()
        }
    }
    let mut count = 0;
    for line in include_str!("../../../../tests/java/entity/single_root/vectors.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let mut level = crate::entity::tests::persistence_level();
        if fields[0] == "entity-single" {
            let mut tag = ferraloom_nbt::parse_compound(fields[2]).unwrap();
            if fields[1] == "invalid-position" {
                tag.insert(
                    "Pos",
                    Tag::List(ferraloom_nbt::ListTag(vec![
                        Tag::Double(f64::NAN),
                        Tag::Double(64.0),
                        Tag::Double(0.0),
                    ])),
                );
            }
            let result = create_entity_from_tag(&tag, request(), &mut level);
            let observed = match result {
                Ok(Some(entity)) => {
                    level.finish_entity_management();
                    let base = level.entity::<EntityData>(entity).unwrap();
                    let observed = format!(
                        "{}:{}:{}:{}",
                        base.entity_type.name(),
                        base.uuid.least,
                        base.passengers.len(),
                        base.vehicle.get().is_some()
                    );
                    let network_id = base.network_id;
                    // A saved passenger, including an unported one, never invokes
                    // its constructor on this single-root path.
                    assert_eq!(
                        level
                            .create_entity_data(EntityType::TNT)
                            .unwrap()
                            .network_id,
                        network_id + 1
                    );
                    assert!(level.all_entities().is_empty());
                    level.release_entity(entity);
                    level.finish_entity_management();
                    assert!(level.entity_state(entity).is_err());
                    observed
                }
                Ok(None) => "none".into(),
                Err(EntityPersistenceError::Base(_)) => "error".into(),
                Err(error) => panic!("{}: {error}", fields[1]),
            };
            assert_eq!(observed, fields[3], "{}", fields[1]);
        } else {
            let tag = ferraloom_nbt::parse_compound("{id:\"minecraft:tnt\"}").unwrap();
            let entity = create_entity_from_tag(&tag, request(), &mut level)
                .unwrap()
                .unwrap();
            let yaw = u32::from_str_radix(fields[1], 16).unwrap();
            level.entity_mut::<EntityData>(entity).unwrap().yaw = f32::from_bits(yaw);
            let behavior = level.entity_behavior(entity).unwrap();
            let result = if fields[0] == "entity-rotate" {
                behavior
                    .rotate(
                        entity,
                        match fields[2] {
                            "NONE" => Rotation::None,
                            "CLOCKWISE_90" => Rotation::Clockwise90,
                            "CLOCKWISE_180" => Rotation::Clockwise180,
                            "COUNTERCLOCKWISE_90" => Rotation::CounterClockwise90,
                            value => panic!("{value}"),
                        },
                        &mut level,
                    )
                    .unwrap()
            } else {
                behavior
                    .mirror(
                        entity,
                        match fields[2] {
                            "NONE" => Mirror::None,
                            "LEFT_RIGHT" => Mirror::LeftRight,
                            "FRONT_BACK" => Mirror::FrontBack,
                            value => panic!("{value}"),
                        },
                        &mut level,
                    )
                    .unwrap()
            };
            assert_eq!(
                bits(result),
                u32::from_str_radix(fields[3], 16).unwrap(),
                "{line}"
            );
            assert_eq!(
                bits(level.entity::<EntityData>(entity).unwrap().yaw),
                u32::from_str_radix(fields[4], 16).unwrap(),
                "{line}: yaw must not mutate"
            );
            level.release_entity(entity);
        }
        count += 1;
    }
    assert_eq!(count, 125);
}
