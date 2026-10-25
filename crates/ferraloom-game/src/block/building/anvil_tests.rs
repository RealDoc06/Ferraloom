use super::{anvil, tests::POS};
use crate::{
    block::{BlockMethod, UnportedBehavior},
    entity::{
        EntityRef, EntityState, item::falling_block::FallingBlockEntity, synched_data::SILENT,
    },
    level::{Level, LevelError, effects::LevelEffect},
};
use ferraloom_data::{Block, BlockState};
use ferraloom_protocol::play::clientbound::Packet;
use ferraloom_util::ChunkPos;

const NATIVE: &str = include_str!("../../../tests/java/blocks/building/anvil.tsv");

fn state(value: &str) -> BlockState {
    BlockState::from_id(value.parse().unwrap()).unwrap()
}
fn receiver(level: &mut Level, silent: bool) -> EntityRef {
    let mut entity = FallingBlockEntity::new(1, 42);
    entity.parent.entity_data.set(SILENT, silent);
    let reference = level.next_entity_ref();
    level
        .add_entity(reference, EntityState::FallingBlock(Box::new(entity)))
        .unwrap();
    level.take_effects();
    reference
}

#[test]
fn anvil_ticks_falling_configuration_and_damage_transform_match_native() {
    let mut current = None;
    let mut checked = 0;
    for line in NATIVE.lines() {
        let f: Vec<_> = line.split('\t').collect();
        match f[0] {
            "damage" => {
                assert_eq!(
                    anvil::damage(state(f[1])).map_or(-1, |state| i32::from(state.id())),
                    f[2].parse::<i32>().unwrap(),
                    "{line}"
                );
            }
            "tick" => {
                let original = state(f[1]);
                let tick = f[4].parse::<i64>().unwrap();
                if tick == 0 {
                    let mut level = super::tests::tagged_level();
                    level
                        .update_entity_chunk_status(
                            ChunkPos::containing(POS),
                            crate::level::entity::Visibility::Ticking,
                        )
                        .unwrap();
                    level
                        .set_chunk_block_ticking(ChunkPos::containing(POS), true)
                        .unwrap();
                    level.set_block_state(POS.below(1), state(f[2])).unwrap();
                    level.set_block_state(POS, original).unwrap();
                    level
                        .required_block_behavior(original.block(), BlockMethod::OnPlace)
                        .unwrap()
                        .on_place(
                            original,
                            POS,
                            Block::AIR.default_state(),
                            f[3] == "true",
                            &mut level,
                        )
                        .unwrap();
                    current = Some(level);
                }
                let level = current.as_mut().unwrap();
                level.tick_block_phase(tick, 65536).unwrap();
                let entities = level
                    .all_entities()
                    .into_iter()
                    .filter_map(|id| level.entity::<FallingBlockEntity>(id).ok())
                    .collect::<Vec<_>>();
                let configuration = entities.first().map_or_else(
                    || "-".to_owned(),
                    |entity| {
                        format!(
                            "{},{},{}",
                            entity.hurt_entities,
                            entity.fall_damage_per_distance.to_bits(),
                            entity.fall_damage_max
                        )
                    },
                );
                assert_eq!(
                    format!(
                        "{}\t{}\t{}\t{configuration}",
                        level.get_block_state(POS).unwrap().id(),
                        level.has_scheduled_block_tick(POS, original.block()),
                        entities.len(),
                    ),
                    f[5..].join("\t"),
                    "{line}"
                );
            }
            _ => continue,
        }
        checked += 1;
    }
    assert_eq!(checked, 590);
}

#[test]
fn fallable_sources_and_anvil_silent_landing_events_match_native() {
    let mut checked = 0;
    for line in NATIVE.lines() {
        let f: Vec<_> = line.split('\t').collect();
        if !matches!(f[0], "event" | "source" | "base-events") {
            continue;
        }
        let mut level = super::tests::tagged_level();
        let entity = receiver(&mut level, f[0] == "event" && f[2] == "true");
        let original = if f[0] == "event" {
            state(f[1])
        } else {
            Block::from_protocol_id(f[1].parse().unwrap())
                .unwrap()
                .default_state()
        };
        if f[0] == "source" {
            let source = level
                .required_block_behavior(original.block(), BlockMethod::GetFallDamageSource)
                .unwrap()
                .get_fall_damage_source(entity, &level)
                .unwrap();
            assert_eq!(source.type_holder().name().to_string(), f[2], "{line}");
            assert_eq!(
                source.direct_entity() == Some(entity),
                f[3] == "true",
                "{line}"
            );
            assert_eq!(
                source.causing_entity() == Some(entity),
                f[4] == "true",
                "{line}"
            );
        } else {
            if f[0] == "base-events" || f[3] == "false" {
                level
                    .required_block_behavior(original.block(), BlockMethod::OnLand)
                    .unwrap()
                    .on_land(
                        original,
                        POS,
                        Block::AIR.default_state(),
                        entity,
                        &mut level,
                    )
                    .unwrap();
            }
            if f[0] == "base-events" || f[3] == "true" {
                level
                    .required_block_behavior(original.block(), BlockMethod::OnBrokenAfterFall)
                    .unwrap()
                    .on_broken_after_fall(POS, entity, &mut level)
                    .unwrap();
            }
            let events = level
                .take_effects()
                .into_iter()
                .filter_map(|effect| match effect {
                    LevelEffect::PlayerPacket { packet, .. } => match *packet {
                        Packet::LevelEvent(event) => {
                            assert_eq!(event.pos, POS);
                            assert_eq!(event.data, 0);
                            Some(event.event_type)
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(format!("{events:?}"), *f.last().unwrap(), "{line}");
        }
        checked += 1;
    }
    assert_eq!(checked, 55);
}

#[test]
fn unported_brushable_and_speleothem_fallable_overrides_stay_required() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let mut level = super::tests::level();
        level.set_unported_behavior(policy);
        for (block, methods) in [
            (
                Block::SUSPICIOUS_SAND,
                &[BlockMethod::OnBrokenAfterFall][..],
            ),
            (
                Block::SUSPICIOUS_GRAVEL,
                &[BlockMethod::OnBrokenAfterFall][..],
            ),
            (
                Block::POINTED_DRIPSTONE,
                &[
                    BlockMethod::OnBrokenAfterFall,
                    BlockMethod::GetFallDamageSource,
                ][..],
            ),
            (
                Block::SULFUR_SPIKE,
                &[
                    BlockMethod::OnBrokenAfterFall,
                    BlockMethod::GetFallDamageSource,
                ][..],
            ),
        ] {
            for &method in methods {
                assert!(matches!(
                    level.required_block_behavior(block, method),
                    Err(LevelError::UnsupportedBlockMethod(b, m)) if b == block && m == method
                ));
            }
        }
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
}
