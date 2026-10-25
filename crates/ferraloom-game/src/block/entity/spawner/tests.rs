use super::*;
use crate::{
    Behaviors,
    block::entity::create,
    level::{LevelContext, Region, chunk::LevelChunk, effects::LevelEffect},
};
use ferraloom_nbt::parse_compound;
use ferraloom_util::{
    BlockPos, ChunkPos,
    random::{LegacyRandomSource, Random},
};
use std::{io::Read, sync::Arc};

const POS: BlockPos = BlockPos::new(8, 64, 8);
const SEED: &str = "{Delay:81s,SpawnData:{entity:{id:'pig'}},SpawnPotentials:[{weight:5,data:{entity:{id:'cow'}}}],MinSpawnDelay:301,MaxSpawnDelay:901,SpawnCount:9,MaxNearbyEntities:8,RequiredPlayerRange:12,SpawnRange:7,components:{'minecraft:custom_name':'old'}}";
static BEHAVIORS: Behaviors = Behaviors::EMPTY;
fn level() -> Level {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let biome = ferraloom_chunk::BiomeId(0);
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            0..6,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
                )),
                biome,
            ),
        ))
        .unwrap();
    level.set_random_streams(Random::Legacy(LegacyRandomSource::new(7)), 0);
    level
}

#[test]
fn native_spawner_load_notifications_use_the_live_state_and_retained_generation() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/spawner-load.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let mut checked = 0;
    for row in vectors.lines() {
        let f: Vec<_> = row.split('\t').collect();
        assert_eq!(f[0], "spawner-live");
        let current = ferraloom_data::BlockState::from_id(f[2].parse().unwrap()).unwrap();
        let custom = f[3] == "true";
        let incoming = parse_compound(f[4]).unwrap();
        let mut original = create(POS, Block::SPAWNER.default_state()).unwrap();
        original.load_with_components(
            &parse_compound(SEED).unwrap(),
            crate::level::LevelContext::fixture(0, 63)
                .registry_context()
                .unwrap(),
        );
        let actual = if f[1] == "false" {
            if custom {
                original.load_custom_only(
                    &incoming,
                    crate::level::LevelContext::fixture(0, 63)
                        .registry_context()
                        .unwrap(),
                );
            } else {
                original.load_with_components(
                    &incoming,
                    crate::level::LevelContext::fixture(0, 63)
                        .registry_context()
                        .unwrap(),
                );
            }
            assert!(f[6].is_empty(), "unattached load: {row}");
            original.save_with_full_metadata().unwrap()
        } else {
            let mut level = level();
            level
                .set_block_state(POS, Block::SPAWNER.default_state())
                .unwrap();
            level.set_block_entity(original).unwrap();
            let handle = level.block_entity_ref(POS).unwrap().unwrap();
            level.retain_block_entity(handle).unwrap();
            // Native retained receivers remain attached after a replacement.
            let replacement = create(POS, Block::SPAWNER.default_state()).unwrap();
            let replacement_saved = replacement.save_with_full_metadata().unwrap();
            level.set_block_entity(replacement).unwrap();
            let replacement = level.block_entity_ref(POS).unwrap().unwrap();
            assert_ne!(handle, replacement);
            level.set_block_state(POS, current).unwrap();
            level.take_effects();
            if custom {
                handle.load_custom_only(&incoming, &mut level).unwrap();
            } else {
                handle.load_with_components(&incoming, &mut level).unwrap();
            }
            let notifications = level
                .take_effects()
                .into_iter()
                .map(|effect| match effect {
                    LevelEffect::BlockUpdated { pos, old, new } => {
                        assert_eq!(pos, POS, "{row}");
                        format!("{},{}", old.id(), new.id())
                    }
                    other => panic!("load must not add setChanged effects: {row}: {other:?}"),
                })
                .collect::<Vec<_>>()
                .join(";");
            let expected = f[6]
                .split(';')
                .filter(|value| !value.is_empty())
                .map(|notification| {
                    let (states, flags) = notification.rsplit_once(',').unwrap();
                    // sendBlockUpdated captures server state; these rendering bits
                    // belong to the native client, not to this server effect.
                    assert_eq!(flags, "260", "{row}");
                    states
                })
                .collect::<Vec<_>>()
                .join(";");
            assert_eq!(notifications, expected, "{row}");
            assert_eq!(
                level
                    .block_entity_state(replacement)
                    .unwrap()
                    .save_with_full_metadata()
                    .unwrap(),
                replacement_saved,
                "loading the old receiver must not retarget: {row}"
            );
            assert_eq!(
                level.random().next_long(),
                LegacyRandomSource::new(7).next_long(),
                "loading does not select weighted spawn data: {row}"
            );
            let saved = level
                .block_entity_state(handle)
                .unwrap()
                .save_with_full_metadata()
                .unwrap();
            level.release_block_entity(handle);
            saved
        };
        assert_eq!(actual, parse_compound(f[5]).unwrap(), "{row}");
        checked += 1;
    }
    assert_eq!(checked, 64);
}
