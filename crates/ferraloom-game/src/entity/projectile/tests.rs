use super::*;
use crate::entity::{RemovalReason, item::PrimedTnt};
use ferraloom_data::EntityType;
use ferraloom_util::{ChunkPos, Uuid};

#[test]
fn native_projectile_nbt_and_owner_cache_keep_uuid_and_hidden_receiver_semantics() {
    let rows = include_str!("../../../tests/java/entity/projectile_owner/vectors.tsv");
    let mut count = 0;
    for line in rows
        .lines()
        .filter(|line| line.starts_with("projectile-nbt\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let level = crate::entity::tests::persistence_level();
        let mut projectile = Projectile::with_parent(EntityData::new(EntityType::SNOWBALL, 1, 0));
        projectile.read_additional_save_data(&ferraloom_nbt::parse_compound(fields[1]).unwrap());
        let mut saved = CompoundTag::new();
        projectile
            .add_additional_save_data(&mut saved, &level)
            .unwrap();
        assert_eq!(
            saved,
            ferraloom_nbt::parse_compound(fields[2]).unwrap(),
            "{line}"
        );
        count += 1;
    }
    let mut level = crate::entity::tests::persistence_level();
    let original = level.next_entity_ref();
    let mut owner = PrimedTnt::new(1, 0);
    owner.parent.uuid = Uuid::new(1, 2);
    level
        .add_entity(original, EntityState::PrimedTnt(Box::new(owner)))
        .unwrap();
    // The abstract parent has no factory; a concrete subclass supplies this
    // identity once. Its own owner reference remains the canonical cached object.
    let mut projectile = Projectile::with_parent(EntityData::new(EntityType::SNOWBALL, 2, 0));
    let mut replacement = None;
    let projectile_id = level.next_entity_ref();
    for line in rows
        .lines()
        .filter(|line| line.starts_with("projectile-owner\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let step: i32 = fields[1].parse().unwrap();
        match step {
            0 => projectile.set_owner(Some(original), &level).unwrap(),
            1 => level.entity_mut::<EntityData>(original).unwrap().uuid = Uuid::new(3, 4),
            2 => level
                .remove_entity_with_reason(original, RemovalReason::Discarded)
                .unwrap(),
            3 => {
                let id = level.next_entity_ref();
                let mut owner = PrimedTnt::new(3, 0);
                owner.parent.uuid = Uuid::new(3, 4);
                level
                    .add_entity(id, EntityState::PrimedTnt(Box::new(owner)))
                    .unwrap();
                level
                    .update_entity_chunk_status(
                        ChunkPos::ZERO,
                        crate::level::entity::Visibility::Tracked,
                    )
                    .unwrap();
                replacement = Some(id);
            }
            4 => level
                .update_entity_chunk_status(
                    ChunkPos::ZERO,
                    crate::level::entity::Visibility::Hidden,
                )
                .unwrap(),
            5 => projectile.set_owner(None, &level).unwrap(),
            6 => projectile.read_additional_save_data(
                &ferraloom_nbt::parse_compound("{Owner:[I;0,3,0,4]}").unwrap(),
            ),
            _ => unreachable!(),
        }
        let owner = projectile.get_owner(&level).unwrap();
        let label = match owner {
            None => "none",
            Some(id) if id == original => "original",
            Some(id) => {
                assert_eq!(Some(id), replacement);
                "replacement"
            }
        };
        assert_eq!(label, fields[2], "{line}");
        assert_eq!(
            projectile
                .owner_uuid(&level)
                .unwrap()
                .map(|uuid| uuid.to_string())
                .unwrap_or_else(|| "none".into()),
            fields[3],
            "{line}"
        );
        let effect = projectile.get_effect_source(projectile_id, &level).unwrap();
        assert_eq!(
            if effect == projectile_id {
                "self"
            } else if effect == original {
                "original"
            } else {
                assert_eq!(Some(effect), replacement);
                "replacement"
            },
            fields[4],
            "{line}: effect source"
        );
        let mut refs = Vec::new();
        projectile.trace_entity_refs(&mut refs);
        assert_eq!(
            refs,
            owner.into_iter().collect::<Vec<_>>(),
            "{line}: trace the currently cached object, not an unresolved UUID"
        );
        let mut restored = Projectile::with_parent(EntityData::new(EntityType::SNOWBALL, 4, 0));
        restored.restore_owner_from(&projectile);
        assert_eq!(restored.get_owner(&level).unwrap(), owner);
        count += 1;
    }
    assert_eq!(count, 16);
}

#[test]
fn current_concrete_entities_never_classify_traceable_items_or_tnt_as_projectiles() {
    let mut level = crate::entity::tests::persistence_level();
    let owner = level.next_entity_ref();
    level
        .add_entity(owner, EntityState::Cow(Box::default()))
        .unwrap();
    let item = level.next_entity_ref();
    let mut item_state = crate::entity::item::ItemEntity::new(2, 0);
    item_state.thrower = Some(EntityReference::of(owner, &level).unwrap());
    level
        .add_entity(item, EntityState::Item(Box::new(item_state)))
        .unwrap();
    assert!(level.entity_state(item).unwrap().as_projectile().is_none());
    let behavior = level.entity_behavior(item).unwrap();
    assert_eq!(behavior.get_owner(item, &mut level).unwrap(), Some(owner));
    assert_eq!(behavior.effect_source(item, &mut level).unwrap(), item);
}
