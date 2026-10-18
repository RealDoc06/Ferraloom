use crate::structure::{BoundingBox, scattered_piece::ScatteredFeaturePiece};
use crate::{Error, region::StructureEntitySpawnFlags, structure::StructurePlacementContext};
use ferraloom_data::{
    Block,
    properties::{self, Facing, Shape},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{Plane, random::RandomSource};

/// Port of SwampHutPiece, including persisted one-time witch/cat attempts.
#[derive(Clone, Debug)]
pub struct SwampHutPiece {
    pub scattered: ScatteredFeaturePiece,
    pub spawned_witch: bool,
    pub spawned_cat: bool,
}
impl SwampHutPiece {
    pub fn new(random: &mut impl RandomSource, west: i32, north: i32) -> Self {
        let direction = Plane::Horizontal.faces()[random.next_int_bound(4) as usize];
        Self {
            scattered: ScatteredFeaturePiece::new([west, 64, north], [7, 7, 9], direction),
            spawned_witch: false,
            spawned_cat: false,
        }
    }
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = self
            .scattered
            .to_nbt(&"minecraft:tesh".parse().expect("builtin piece"));
        tag.insert("Witch", Tag::Byte(i8::from(self.spawned_witch)));
        tag.insert("Cat", Tag::Byte(i8::from(self.spawned_cat)));
        tag
    }
    pub fn from_nbt(tag: &CompoundTag) -> Result<Self, Error> {
        Ok(Self {
            scattered: ScatteredFeaturePiece::from_nbt(tag)?,
            spawned_witch: tag.get_byte("Witch").unwrap_or(0) != 0,
            spawned_cat: tag.get_byte("Cat").unwrap_or(0) != 0,
        })
    }
    pub fn post_process(
        &mut self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        if !self
            .scattered
            .update_average_ground_height(context.region, bounds, 0)?
        {
            return Ok(());
        }
        let data = &self.scattered.data;
        for (min, max) in [
            ([1, 1, 1], [5, 1, 7]),
            ([1, 4, 2], [5, 4, 7]),
            ([2, 1, 0], [4, 1, 0]),
            ([2, 2, 2], [3, 3, 2]),
            ([1, 2, 3], [1, 3, 6]),
            ([5, 2, 3], [5, 3, 6]),
            ([2, 2, 7], [4, 3, 7]),
        ] {
            let state = Block::SPRUCE_PLANKS.default_state();
            data.generate_box(context.region, bounds, min, max, state, state, false)?;
        }
        for [x, z] in [[1, 2], [5, 2], [1, 7], [5, 7]] {
            let state = Block::OAK_LOG.default_state();
            data.generate_box(
                context.region,
                bounds,
                [x, 0, z],
                [x, 3, z],
                state,
                state,
                false,
            )?;
        }
        for (block, p) in [
            (Block::OAK_FENCE, [2, 3, 2]),
            (Block::OAK_FENCE, [3, 3, 7]),
            (Block::AIR, [1, 3, 4]),
            (Block::AIR, [5, 3, 4]),
            (Block::AIR, [5, 3, 5]),
            (Block::POTTED_RED_MUSHROOM, [1, 3, 5]),
            (Block::CRAFTING_TABLE, [3, 2, 6]),
            (Block::CAULDRON, [4, 2, 6]),
            (Block::OAK_FENCE, [1, 2, 1]),
            (Block::OAK_FENCE, [5, 2, 1]),
        ] {
            data.place_block(context.region, block.default_state(), p, bounds)?;
        }
        let stairs = |facing| {
            Block::SPRUCE_STAIRS
                .default_state()
                .with(properties::FACING, facing)
                .expect("stairs facing")
        };
        for (facing, min, max) in [
            (Facing::North, [0, 4, 1], [6, 4, 1]),
            (Facing::East, [0, 4, 2], [0, 4, 7]),
            (Facing::West, [6, 4, 2], [6, 4, 7]),
            (Facing::South, [0, 4, 8], [6, 4, 8]),
        ] {
            let state = stairs(facing);
            data.generate_box(context.region, bounds, min, max, state, state, false)?;
        }
        for (facing, shape, p) in [
            (Facing::North, Shape::OuterRight, [0, 4, 1]),
            (Facing::North, Shape::OuterLeft, [6, 4, 1]),
            (Facing::South, Shape::OuterLeft, [0, 4, 8]),
            (Facing::South, Shape::OuterRight, [6, 4, 8]),
        ] {
            data.place_block(
                context.region,
                stairs(facing)
                    .with(properties::SHAPE, shape)
                    .expect("stairs shape"),
                p,
                bounds,
            )?;
        }
        for z in [2, 7] {
            for x in [1, 5] {
                data.fill_column_down(
                    context.region,
                    Block::OAK_LOG.default_state(),
                    [x, -1, z],
                    bounds,
                )?;
            }
        }
        let p = data.world_position([2, 2, 5]);
        if bounds.contains(p) {
            let flags = StructureEntitySpawnFlags {
                persistent: true,
                finalize_spawn: true,
                with_passengers: true,
            };
            let position = [
                f64::from(p[0]) + 0.5,
                f64::from(p[1]),
                f64::from(p[2]) + 0.5,
            ];
            if !self.spawned_witch {
                self.spawned_witch = true;
                context.region.spawn_structure_entity(
                    ferraloom_data::EntityType::WITCH,
                    position,
                    Some(0.0),
                    flags,
                )?;
            }
            if !self.spawned_cat {
                self.spawned_cat = true;
                context.region.spawn_structure_entity(
                    ferraloom_data::EntityType::CAT,
                    position,
                    Some(0.0),
                    flags,
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        feature::FeatureRegistry,
        region::{ChunkStatus, RegionChunk, WorldGenRegion},
        structure::processor::tests::{StaticBlocks, generator},
    };
    use ferraloom_util::random::{LegacyRandomSource, WorldgenRandom};
    use serde_json::Value;
    use std::io::Read;
    fn point(v: &Value) -> [i32; 3] {
        std::array::from_fn(|i| v[i].as_i64().unwrap() as i32)
    }
    #[test]
    fn saved_hut_blocks_clipping_ground_height_and_postprocessing_match_vanilla() {
        let (_, generator) = generator();
        let features = FeatureRegistry::default();
        let mut text = String::new();
        flate2::read::GzDecoder::new(&include_bytes!("../testdata/swamp_hut_blocks.json.gz")[..])
            .read_to_string(&mut text)
            .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let mode = case["mode"].as_i64().unwrap();
            let mut chunk = generator.new_chunk([0, 0]);
            for z in 0..16 {
                for x in 0..16 {
                    let ground = match mode {
                        0 => 63,
                        1 => 60 + (x * 3 + z * 5) % 8,
                        _ => 55,
                    };
                    for y in -64..=if mode == 2 { 63 } else { ground } {
                        let block = if y <= ground {
                            Block::STONE
                        } else {
                            Block::WATER
                        };
                        chunk.set_block(x as u8, y, z as u8, block.default_state());
                    }
                }
            }
            let mut chunks = [RegionChunk::new(chunk, ChunkStatus::Terrain)];
            let mut region = WorldGenRegion::new(
                &mut chunks,
                [0, 0],
                &[ChunkStatus::Terrain],
                0,
                None,
                &StaticBlocks,
                0,
                63,
            )
            .unwrap();
            let mut piece = SwampHutPiece::from_nbt(
                &ferraloom_nbt::parse_compound(case["initial"].as_str().unwrap()).unwrap(),
            )
            .unwrap();
            let mut random = WorldgenRandom::new(ferraloom_util::random::Random::Legacy(
                LegacyRandomSource::new(17),
            ));
            let b = &case["bounds"];
            let bounds = BoundingBox::new(
                point(b),
                std::array::from_fn(|i| b[i + 3].as_i64().unwrap() as i32),
            );
            piece
                .post_process(
                    &mut StructurePlacementContext {
                        region: &mut region,
                        random: &mut random,
                        origin: [0, 0, 0],
                        generator: &generator,
                        registry: &features,
                    },
                    bounds,
                )
                .unwrap();
            assert_eq!(
                piece.to_nbt(),
                ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap(),
                "direction={} mode={mode}",
                case["direction"]
            );
            assert_eq!(random.next_int(), case["next"].as_i64().unwrap() as i32);
            let chunk = region.chunk([0, 0], ChunkStatus::Terrain).unwrap();
            let actual = chunk
                .poi_changes
                .iter()
                .map(|(p, _, s)| (*p, s.id()))
                .collect::<Vec<_>>();
            let expected = case["writes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (point(&v["pos"]), v["state"].as_u64().unwrap() as u16))
                .collect::<Vec<_>>();
            assert_eq!(
                actual, expected,
                "direction={} mode={mode}",
                case["direction"]
            );
            let mut post = chunk.chunk.post_processing.clone();
            post.sort_by_key(|p| p[1] >> 4);
            let expected = case["post"]
                .as_array()
                .unwrap()
                .iter()
                .map(point)
                .collect::<Vec<_>>();
            assert_eq!(post, expected);
        }
    }
}
