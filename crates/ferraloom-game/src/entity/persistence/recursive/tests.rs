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
        let mut state = PrimedTnt::with_parent(level.create_entity_data(EntityType::TNT));
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
            level.next_entity_id() - 1,
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
