//! Actual protochunk receivers share the canonical game states and codecs.
//! No world/level callback occurs while these level-less objects are borrowed.
use super::BlockEntityState;
use ferraloom_data::EntityType;
use ferraloom_nbt::CompoundTag;
use ferraloom_util::random::WorldgenRandom;
use ferraloom_worldgen::{Error, Identifier, region::WorldgenBlockEntity};

impl WorldgenBlockEntity for BlockEntityState {
    fn clone_state(&self) -> Box<dyn WorldgenBlockEntity> {
        Box::new(self.clone())
    }
    fn save(&self) -> Result<CompoundTag, Error> {
        self.save_with_full_metadata()
            .map_err(|error| Error(format!("block entity save: {error}")))
    }
    fn is_randomizable(&self) -> bool {
        self.randomizable().is_some()
    }
    fn set_loot_table(&mut self, table: &Identifier, seed: i64) -> Result<(), Error> {
        if let Some(container) = self.randomizable_mut() {
            container.loot_table = Some(
                ferraloom_protocol::Identifier::parse(&table.to_string())
                    .map_err(|error| Error(format!("loot table: {error}")))?,
            );
            container.loot_table_seed = seed;
        }
        Ok(())
    }
    fn set_entity_id(
        &mut self,
        kind: EntityType,
        random: &mut WorldgenRandom,
    ) -> Result<(), Error> {
        if let Self::Spawner(entity) = self {
            entity.spawner.set_entity_id(kind, random);
        }
        Ok(())
    }
    fn populate_beehive(&mut self, random: &mut WorldgenRandom) -> Result<(), Error> {
        if let Self::Beehive(entity) = self {
            entity.populate_worldgen(random);
        }
        Ok(())
    }
    fn load_with_components(&mut self, tag: &CompoundTag) -> Result<(), Error> {
        BlockEntityState::load_with_components(self, tag);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{entity, worldgen::GameWorldgenBlockBehavior};
    use ferraloom_data::{
        Block,
        datapack::{Pack, ResourceManager},
    };
    use ferraloom_nbt::parse_compound;
    use ferraloom_util::{
        BlockPos,
        random::{Algorithm, LegacyRandomSource, RandomSource},
    };
    use ferraloom_worldgen::{
        generator::{
            GeneratedChunk, GenerationData,
            flat::{FlatLayerInfo, FlatLevelGeneratorSettings, FlatLevelSource},
        },
        region::{ChunkStatus, RegionChunk, WorldGenRegion},
        settings::DimensionType,
    };
    use std::{
        path::Path,
        sync::{Arc, OnceLock},
    };

    const POS: [i32; 3] = [8, 64, 8];
    const VECTORS: &str = include_str!("../../../tests/java/blocks/entities/worldgen.tsv");

    fn chunk() -> GeneratedChunk {
        static CHUNK: OnceLock<GeneratedChunk> = OnceLock::new();
        CHUNK
            .get_or_init(|| {
                let jar =
                    std::env::var_os("FERRALOOM_VANILLA_JAR").expect("native fixture datapack");
                let resources =
                    ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
                let data = Arc::new(GenerationData::load(&resources).unwrap());
                let generator = FlatLevelSource::new(
                    data,
                    FlatLevelGeneratorSettings {
                        structure_overrides: Some(Vec::new()),
                        structure_override_tag: None,
                        layers: vec![FlatLayerInfo {
                            height: 1,
                            block: Block::DIRT,
                        }],
                        lakes: false,
                        features: false,
                        biome: "plains".parse().unwrap(),
                    },
                )
                .unwrap();
                let dimension = DimensionType::decode(
                    &resources
                        .require(&"dimension_type/overworld.json".parse().unwrap())
                        .unwrap()
                        .json()
                        .unwrap(),
                )
                .unwrap();
                generator.build_terrain([0, 0], dimension)
            })
            .clone()
    }

    #[test]
    fn worldgen_materialization_rng_and_saved_nbt_match_vanilla() {
        let mut observations = 0;
        for line in VECTORS
            .lines()
            .filter(|line| line.starts_with("worldgen\t"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let block = Block::from_name(fields[2]).unwrap();
            let seed = fields[3].parse().unwrap();
            let mut generated = chunk();
            generated.set_block(8, 64, 8, block.default_state());
            let mut chunks = [RegionChunk::new(generated, ChunkStatus::Features)];
            let pending = if fields[1] == "load" {
                Some(parse_compound("{id:'DUMMY',x:8,y:64,z:8}").unwrap())
            } else {
                (fields[4] != "-").then(|| parse_compound(fields[4]).unwrap())
            };
            if let Some(pending) = pending {
                chunks[0].block_entities.insert(POS, pending);
            }
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(seed));
            let mut region = WorldGenRegion::new(
                &mut chunks,
                [0, 0],
                &[ChunkStatus::Features],
                0,
                None,
                &GameWorldgenBlockBehavior,
                0,
                63,
            )
            .unwrap();
            let table = "minecraft:chests/simple_dungeon".parse().unwrap();
            match fields[1] {
                "loot" => region.set_loot_table(POS, &table, &mut random).unwrap(),
                "seed" => region
                    .set_loot_table_seed(POS, &table, -9123456789)
                    .unwrap(),
                "spawner" | "spawner-twice" => {
                    region
                        .set_spawner_entity(POS, EntityType::ZOMBIE, &mut random)
                        .unwrap();
                    if fields[1] == "spawner-twice" {
                        region
                            .set_spawner_entity(POS, EntityType::SKELETON, &mut random)
                            .unwrap();
                    }
                }
                "bees" => region.populate_worldgen_beehive(POS, &mut random).unwrap(),
                "load" => region
                    .load_block_entity(POS, parse_compound(fields[5]).unwrap(), &mut random)
                    .unwrap(),
                action => panic!("unknown native action {action}"),
            }
            // Actual native lookups materialize on observation, even after a
            // non-matching operation; a second lookup must keep that receiver.
            let first = region
                .get_or_create_block_entity(POS)
                .unwrap()
                .map(|value| std::ptr::from_ref(value).cast::<()>());
            let second = region
                .get_or_create_block_entity(POS)
                .unwrap()
                .map(|value| std::ptr::from_ref(value).cast::<()>());
            assert_eq!(
                first == second,
                fields[9].parse::<bool>().unwrap(),
                "{line}"
            );
            assert_eq!(
                random.next_long(),
                fields[7].parse::<i64>().unwrap(),
                "{line}"
            );
            let saved = chunks[0].saved_block_entities().unwrap();
            let expected = (fields[6] != "null").then(|| parse_compound(fields[6]).unwrap());
            assert_eq!(saved.first(), expected.as_ref(), "{line}");
            assert_eq!(
                !chunks[0].block_entities.is_empty(),
                fields[8].parse::<bool>().unwrap(),
                "{line}"
            );
            observations += 1;
        }
        assert_eq!(observations, 180);
    }

    #[test]
    fn promotion_preserves_first_selection_but_nbt_boundary_loses_later_alias() {
        let pos = BlockPos::new(8, 64, 8);
        let state = Block::SPAWNER.default_state();
        let mut observations = 0;
        for line in VECTORS
            .lines()
            .filter(|line| line.starts_with("promotion\t"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let seed: i64 = fields[1].parse().unwrap();
            let mut original = entity::load_static(pos, state, &parse_compound(fields[2]).unwrap())
                .unwrap()
                .unwrap();
            let mut prep = LegacyRandomSource::new(seed);
            let BlockEntityState::Spawner(spawner) = &mut original else {
                unreachable!()
            };
            spawner.spawner.set_entity_id(EntityType::ZOMBIE, &mut prep);
            let saved = original.save_with_full_metadata().unwrap();
            assert_eq!(saved, parse_compound(fields[3]).unwrap());
            // This is the current serialized FULL boundary. The actual native
            // LevelChunk constructor keeps original; both paths select the same
            // first SpawnData and consume the same RNG, before later mutation.
            let mut loaded = entity::load_static(pos, state, &saved).unwrap().unwrap();
            let mut native_random = LegacyRandomSource::new(seed + 1);
            let mut loaded_random = LegacyRandomSource::new(seed + 1);
            for (entity, random) in [
                (&mut original, &mut native_random),
                (&mut loaded, &mut loaded_random),
            ] {
                let BlockEntityState::Spawner(entity) = entity else {
                    unreachable!()
                };
                assert_eq!(
                    entity.spawner.get_or_create_next_spawn_data(random).entity,
                    parse_compound(fields[4]).unwrap()
                );
                random.next_long(); // The native proof compares this first draw.
                entity.spawner.set_entity_id(EntityType::CREEPER, random);
            }
            assert_eq!(
                original.save_with_full_metadata().unwrap(),
                parse_compound(fields[5]).unwrap()
            );
            assert_eq!(
                loaded.save_with_full_metadata().unwrap(),
                parse_compound(fields[6]).unwrap()
            );
            assert_ne!(
                original.save_with_full_metadata().unwrap(),
                loaded.save_with_full_metadata().unwrap()
            );
            assert_eq!(native_random.next_long(), fields[7].parse::<i64>().unwrap());
            assert_eq!(loaded_random.next_long(), fields[8].parse::<i64>().unwrap());
            observations += 1;
        }
        assert_eq!(observations, 4);
    }
}
