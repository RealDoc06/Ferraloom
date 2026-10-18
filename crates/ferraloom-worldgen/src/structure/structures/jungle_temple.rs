use crate::{
    Error,
    structure::StructurePlacementContext,
    structure::{BoundingBox, scattered_piece::ScatteredFeaturePiece},
};
use ferraloom_data::{
    Block,
    properties::{self, East, Face, Facing, North, South, West},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{Plane, random::RandomSource};

/// Port of JungleTemplePiece, including ordered moss selection outside the
/// writable box. Chest and dispenser loot use the shared game behavior bridge.
#[derive(Clone, Debug)]
pub struct JungleTemplePiece {
    pub scattered: ScatteredFeaturePiece,
    pub placed_main_chest: bool,
    pub placed_hidden_chest: bool,
    pub placed_traps: [bool; 2],
}
impl JungleTemplePiece {
    pub fn new(random: &mut impl RandomSource, west: i32, north: i32) -> Self {
        Self {
            scattered: ScatteredFeaturePiece::new(
                [west, 64, north],
                [12, 10, 15],
                Plane::Horizontal.faces()[random.next_int_bound(4) as usize],
            ),
            placed_main_chest: false,
            placed_hidden_chest: false,
            placed_traps: [false; 2],
        }
    }
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = self
            .scattered
            .to_nbt(&"minecraft:tejp".parse().expect("builtin piece"));
        for (name, value) in [
            ("placedMainChest", self.placed_main_chest),
            ("placedHiddenChest", self.placed_hidden_chest),
            ("placedTrap1", self.placed_traps[0]),
            ("placedTrap2", self.placed_traps[1]),
        ] {
            tag.insert(name, Tag::Byte(i8::from(value)));
        }
        tag
    }
    pub fn from_nbt(tag: &CompoundTag) -> Result<Self, Error> {
        let placed = |name| tag.get_byte(name).unwrap_or(0) != 0;
        Ok(Self {
            scattered: ScatteredFeaturePiece::from_nbt(tag)?,
            placed_main_chest: placed("placedMainChest"),
            placed_hidden_chest: placed("placedHiddenChest"),
            placed_traps: [placed("placedTrap1"), placed("placedTrap2")],
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
        let stone_box = |context: &mut StructurePlacementContext<'_, '_>,
                         min: [i32; 3],
                         max: [i32; 3]|
         -> Result<(), Error> {
            for y in min[1]..=max[1] {
                for x in min[0]..=max[0] {
                    for z in min[2]..=max[2] {
                        let block = if context.random.next_float() < 0.4 {
                            Block::COBBLESTONE
                        } else {
                            Block::MOSSY_COBBLESTONE
                        };
                        data.place_block(context.region, block.default_state(), [x, y, z], bounds)?;
                    }
                }
            }
            Ok(())
        };
        let air_box = |context: &mut StructurePlacementContext<'_, '_>, min, max| {
            data.generate_box(
                context.region,
                bounds,
                min,
                max,
                Block::AIR.default_state(),
                Block::AIR.default_state(),
                false,
            )
        };
        for (min, max) in [
            ([0, -4, 0], [11, 0, 14]),
            ([2, 1, 2], [9, 2, 2]),
            ([2, 1, 12], [9, 2, 12]),
            ([2, 1, 3], [2, 2, 11]),
            ([9, 1, 3], [9, 2, 11]),
            ([1, 3, 1], [10, 6, 1]),
            ([1, 3, 13], [10, 6, 13]),
            ([1, 3, 2], [1, 6, 12]),
            ([10, 3, 2], [10, 6, 12]),
            ([2, 3, 2], [9, 3, 12]),
            ([2, 6, 2], [9, 6, 12]),
            ([3, 7, 3], [8, 7, 11]),
            ([4, 8, 4], [7, 8, 10]),
        ] {
            stone_box(context, min, max)?;
        }
        for (min, max) in [
            ([3, 1, 3], [8, 2, 11]),
            ([4, 3, 6], [7, 3, 9]),
            ([2, 4, 2], [9, 5, 12]),
            ([4, 6, 5], [7, 6, 9]),
            ([5, 7, 6], [6, 7, 8]),
            ([5, 1, 2], [6, 2, 2]),
            ([5, 2, 12], [6, 2, 12]),
            ([5, 5, 1], [6, 5, 1]),
            ([5, 5, 13], [6, 5, 13]),
        ] {
            air_box(context, min, max)?;
        }
        for p in [[1, 5, 5], [10, 5, 5], [1, 5, 9], [10, 5, 9]] {
            data.place_block(context.region, Block::AIR.default_state(), p, bounds)?;
        }
        for z in [0, 14] {
            for x in [2, 4, 7, 9] {
                stone_box(context, [x, 4, z], [x, 5, z])?;
            }
        }
        stone_box(context, [5, 6, 0], [6, 6, 0])?;
        for x in [0, 11] {
            for z in (2..=12).step_by(2) {
                stone_box(context, [x, 4, z], [x, 5, z])?;
            }
            for z in [5, 9] {
                stone_box(context, [x, 6, z], [x, 6, z])?;
            }
        }
        for (min, max) in [
            ([2, 7, 2], [2, 9, 2]),
            ([9, 7, 2], [9, 9, 2]),
            ([2, 7, 12], [2, 9, 12]),
            ([9, 7, 12], [9, 9, 12]),
            ([4, 9, 4], [4, 9, 4]),
            ([7, 9, 4], [7, 9, 4]),
            ([4, 9, 10], [4, 9, 10]),
            ([7, 9, 10], [7, 9, 10]),
            ([5, 9, 7], [6, 9, 7]),
        ] {
            stone_box(context, min, max)?;
        }
        let stairs = |facing| {
            Block::COBBLESTONE_STAIRS
                .default_state()
                .with(properties::FACING, facing)
                .expect("stair facing")
        };
        for (facing, p) in [
            (Facing::North, [5, 9, 6]),
            (Facing::North, [6, 9, 6]),
            (Facing::South, [5, 9, 8]),
            (Facing::South, [6, 9, 8]),
            (Facing::North, [4, 0, 0]),
            (Facing::North, [5, 0, 0]),
            (Facing::North, [6, 0, 0]),
            (Facing::North, [7, 0, 0]),
            (Facing::North, [4, 1, 8]),
            (Facing::North, [4, 2, 9]),
            (Facing::North, [4, 3, 10]),
            (Facing::North, [7, 1, 8]),
            (Facing::North, [7, 2, 9]),
            (Facing::North, [7, 3, 10]),
        ] {
            data.place_block(context.region, stairs(facing), p, bounds)?;
        }
        for (min, max) in [
            ([4, 1, 9], [4, 1, 9]),
            ([7, 1, 9], [7, 1, 9]),
            ([4, 1, 10], [7, 2, 10]),
            ([5, 4, 5], [6, 4, 5]),
        ] {
            stone_box(context, min, max)?;
        }
        data.place_block(context.region, stairs(Facing::East), [4, 4, 5], bounds)?;
        data.place_block(context.region, stairs(Facing::West), [7, 4, 5], bounds)?;
        for i in 0..4 {
            for x in [5, 6] {
                data.place_block(
                    context.region,
                    stairs(Facing::South),
                    [x, -i, 6 + i],
                    bounds,
                )?;
            }
            air_box(context, [5, -i, 7 + i], [6, -i, 9 + i])?;
        }
        for (min, max) in [
            ([1, -3, 12], [10, -1, 13]),
            ([1, -3, 1], [3, -1, 13]),
            ([1, -3, 1], [9, -1, 5]),
        ] {
            air_box(context, min, max)?;
        }
        for z in (1..=13).step_by(2) {
            stone_box(context, [1, -3, z], [1, -2, z])?;
        }
        for z in (2..=12).step_by(2) {
            stone_box(context, [1, -1, z], [3, -1, z])?;
        }
        for (min, max) in [
            ([2, -2, 1], [5, -2, 1]),
            ([7, -2, 1], [9, -2, 1]),
            ([6, -3, 1], [6, -3, 1]),
            ([6, -1, 1], [6, -1, 1]),
        ] {
            stone_box(context, min, max)?;
        }
        let hook = |facing| {
            Block::TRIPWIRE_HOOK
                .default_state()
                .with(properties::FACING, facing)
                .unwrap()
                .with(properties::ATTACHED, true)
                .unwrap()
        };
        let trip_ew = Block::TRIPWIRE
            .default_state()
            .with(properties::EAST, East::True)
            .unwrap()
            .with(properties::WEST, West::True)
            .unwrap()
            .with(properties::ATTACHED, true)
            .unwrap();
        let trip_ns = Block::TRIPWIRE
            .default_state()
            .with(properties::NORTH, North::True)
            .unwrap()
            .with(properties::SOUTH, South::True)
            .unwrap()
            .with(properties::ATTACHED, true)
            .unwrap();
        let wire_ns = Block::REDSTONE_WIRE
            .default_state()
            .with(properties::NORTH, North::Side)
            .unwrap()
            .with(properties::SOUTH, South::Side)
            .unwrap();
        let wire_ew = Block::REDSTONE_WIRE
            .default_state()
            .with(properties::EAST, East::Side)
            .unwrap()
            .with(properties::WEST, West::Side)
            .unwrap();
        let moss = Block::MOSSY_COBBLESTONE.default_state();
        for (state, p) in [
            (hook(Facing::East), [1, -3, 8]),
            (hook(Facing::West), [4, -3, 8]),
            (trip_ew, [2, -3, 8]),
            (trip_ew, [3, -3, 8]),
        ] {
            data.place_block(context.region, state, p, bounds)?;
        }
        for z in (2..=7).rev() {
            data.place_block(context.region, wire_ns, [5, -3, z], bounds)?;
        }
        let wire_nw = Block::REDSTONE_WIRE
            .default_state()
            .with(properties::NORTH, North::Side)
            .unwrap()
            .with(properties::WEST, West::Side)
            .unwrap();
        for (state, p) in [
            (wire_nw, [5, -3, 1]),
            (wire_ew, [4, -3, 1]),
            (moss, [3, -3, 1]),
        ] {
            data.place_block(context.region, state, p, bounds)?;
        }
        if !self.placed_traps[0] {
            self.placed_traps[0] = data.create_dispenser(
                context,
                [3, -2, 1],
                Facing::North,
                bounds,
                &"minecraft:chests/jungle_temple_dispenser".parse()?,
            )?;
        }
        let vine_s = Block::VINE
            .default_state()
            .with(properties::SOUTH, South::True)
            .unwrap();
        for (state, p) in [
            (vine_s, [3, -2, 2]),
            (hook(Facing::North), [7, -3, 1]),
            (hook(Facing::South), [7, -3, 5]),
            (trip_ns, [7, -3, 2]),
            (trip_ns, [7, -3, 3]),
            (trip_ns, [7, -3, 4]),
            (wire_ew, [8, -3, 6]),
            (
                Block::REDSTONE_WIRE
                    .default_state()
                    .with(properties::WEST, West::Side)
                    .unwrap()
                    .with(properties::SOUTH, South::Side)
                    .unwrap(),
                [9, -3, 6],
            ),
            (
                Block::REDSTONE_WIRE
                    .default_state()
                    .with(properties::NORTH, North::Side)
                    .unwrap()
                    .with(properties::SOUTH, South::Up)
                    .unwrap(),
                [9, -3, 5],
            ),
            (moss, [9, -3, 4]),
            (wire_ns, [9, -2, 4]),
        ] {
            data.place_block(context.region, state, p, bounds)?;
        }
        if !self.placed_traps[1] {
            self.placed_traps[1] = data.create_dispenser(
                context,
                [9, -2, 3],
                Facing::West,
                bounds,
                &"minecraft:chests/jungle_temple_dispenser".parse()?,
            )?;
        }
        let vine_e = Block::VINE
            .default_state()
            .with(properties::EAST, East::True)
            .unwrap();
        for p in [[8, -1, 3], [8, -2, 3]] {
            data.place_block(context.region, vine_e, p, bounds)?;
        }
        let loot = "minecraft:chests/jungle_temple".parse()?;
        if !self.placed_main_chest {
            self.placed_main_chest = data.create_chest(context, [8, -3, 3], bounds, &loot)?;
        }
        for p in [
            [9, -3, 2],
            [8, -3, 1],
            [4, -3, 5],
            [5, -2, 5],
            [5, -1, 5],
            [6, -3, 5],
            [7, -2, 5],
            [7, -1, 5],
            [8, -3, 5],
        ] {
            data.place_block(context.region, moss, p, bounds)?;
        }
        stone_box(context, [9, -1, 1], [9, -1, 5])?;
        air_box(context, [8, -3, 8], [10, -1, 10])?;
        for x in 8..=10 {
            data.place_block(
                context.region,
                Block::CHISELED_STONE_BRICKS.default_state(),
                [x, -2, 11],
                bounds,
            )?;
        }
        let lever = Block::LEVER
            .default_state()
            .with(properties::FACING, Facing::North)
            .unwrap()
            .with(properties::FACE, Face::Wall)
            .unwrap();
        for x in 8..=10 {
            data.place_block(context.region, lever, [x, -2, 12], bounds)?;
        }
        stone_box(context, [8, -3, 8], [8, -3, 10])?;
        stone_box(context, [10, -3, 8], [10, -3, 10])?;
        let all_wire = wire_ns
            .with(properties::EAST, East::Side)
            .unwrap()
            .with(properties::WEST, West::Side)
            .unwrap();
        let piston = |facing| {
            Block::STICKY_PISTON
                .default_state()
                .with(properties::FACING, facing)
                .unwrap()
        };
        for (state, p) in [
            (moss, [10, -2, 9]),
            (wire_ns, [8, -2, 9]),
            (wire_ns, [8, -2, 10]),
            (all_wire, [10, -1, 9]),
            (piston(Facing::Up), [9, -2, 8]),
            (piston(Facing::West), [10, -2, 8]),
            (piston(Facing::West), [10, -1, 8]),
            (
                Block::REPEATER
                    .default_state()
                    .with(properties::FACING, Facing::North)
                    .unwrap(),
                [10, -2, 10],
            ),
        ] {
            data.place_block(context.region, state, p, bounds)?;
        }
        if !self.placed_hidden_chest {
            self.placed_hidden_chest = data.create_chest(context, [9, -3, 10], bounds, &loot)?;
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
    fn saved_temple_blocks_moss_rng_clipping_and_ground_height_match_vanilla() {
        let (_, generator) = generator();
        let features = FeatureRegistry::default();
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../testdata/jungle_temple_blocks.json.gz")[..],
        )
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
            let mut piece = JungleTemplePiece::from_nbt(
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
