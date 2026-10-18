use crate::structure::{BoundingBox, scattered_piece::ScatteredFeaturePiece};
use crate::{Error, feature::FeatureContext};
use ferraloom_data::{
    Block,
    properties::{self, Facing},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{
    BlockPos, Plane, Rotation,
    random::{LegacyRandomSource, RandomSource},
};
use std::collections::BTreeSet;

/// Port of DesertPyramidPiece. Archaeology positions are transient and collect
/// on each postProcess call; the structure deduplicates them during afterPlace.
#[derive(Clone, Debug)]
pub struct DesertPyramidPiece {
    pub scattered: ScatteredFeaturePiece,
    pub placed_chests: [bool; 4],
    pub potential_suspicious_sand: Vec<[i32; 3]>,
    pub collapsed_roof: [i32; 3],
}
impl DesertPyramidPiece {
    pub fn new(random: &mut impl RandomSource, west: i32, north: i32) -> Self {
        let direction = Plane::Horizontal.faces()[random.next_int_bound(4) as usize];
        Self {
            scattered: ScatteredFeaturePiece::new([west, 64, north], [21, 15, 21], direction),
            placed_chests: [false; 4],
            potential_suspicious_sand: Vec::new(),
            collapsed_roof: [0; 3],
        }
    }
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = self
            .scattered
            .to_nbt(&"minecraft:tedp".parse().expect("builtin piece"));
        for (i, placed) in self.placed_chests.iter().enumerate() {
            tag.insert(format!("hasPlacedChest{i}"), Tag::Byte(i8::from(*placed)));
        }
        tag
    }
    pub fn from_nbt(tag: &CompoundTag) -> Result<Self, Error> {
        Ok(Self {
            scattered: ScatteredFeaturePiece::from_nbt(tag)?,
            placed_chests: std::array::from_fn(|i| {
                tag.get_byte(&format!("hasPlacedChest{i}")).unwrap_or(0) != 0
            }),
            potential_suspicious_sand: Vec::new(),
            collapsed_roof: [0; 3],
        })
    }
    pub fn post_process(
        &mut self,
        context: &mut FeatureContext<'_, '_>,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        let offset = -context.random.next_int_bound(3);
        if !self
            .scattered
            .update_lowest_ground_height(context.region, offset)?
        {
            return Ok(());
        }
        let data = &self.scattered.data;
        let w = self.scattered.size[0];
        let d = self.scattered.size[2];
        let ss = Block::SANDSTONE.default_state();
        let air = Block::AIR.default_state();
        let cut = Block::CUT_SANDSTONE.default_state();
        let chisel = Block::CHISELED_SANDSTONE.default_state();
        let orange = Block::ORANGE_TERRACOTTA.default_state();
        data.generate_box(
            context.region,
            bounds,
            [0, -4, 0],
            [w - 1, 0, d - 1],
            ss,
            ss,
            false,
        )?;
        for p in 1..=9 {
            data.generate_box(
                context.region,
                bounds,
                [p, p, p],
                [w - 1 - p, p, d - 1 - p],
                ss,
                ss,
                false,
            )?;
            data.generate_box(
                context.region,
                bounds,
                [p + 1, p, p + 1],
                [w - 2 - p, p, d - 2 - p],
                air,
                air,
                false,
            )?;
        }
        for x in 0..w {
            for z in 0..d {
                data.fill_column_down(context.region, ss, [x, -5, z], bounds)?;
            }
        }
        let stairs = |facing| {
            Block::SANDSTONE_STAIRS
                .default_state()
                .with(properties::FACING, facing)
                .expect("stairs facing")
        };
        for x in [0, w - 5] {
            data.generate_box(
                context.region,
                bounds,
                [x, 0, 0],
                [x + 4, 9, 4],
                ss,
                air,
                false,
            )?;
            data.generate_box(
                context.region,
                bounds,
                [x + 1, 10, 1],
                [x + 3, 10, 3],
                ss,
                ss,
                false,
            )?;
            for (facing, position) in [
                (Facing::North, [x + 2, 10, 0]),
                (Facing::South, [x + 2, 10, 4]),
                (Facing::East, [x, 10, 2]),
                (Facing::West, [x + 4, 10, 2]),
            ] {
                data.place_block(context.region, stairs(facing), position, bounds)?;
            }
        }
        data.generate_box(
            context.region,
            bounds,
            [8, 0, 0],
            [12, 4, 4],
            ss,
            air,
            false,
        )?;
        data.generate_box(
            context.region,
            bounds,
            [9, 1, 0],
            [11, 3, 4],
            air,
            air,
            false,
        )?;
        for p in [
            [9, 1, 1],
            [9, 2, 1],
            [9, 3, 1],
            [10, 3, 1],
            [11, 3, 1],
            [11, 2, 1],
            [11, 1, 1],
        ] {
            data.place_block(context.region, cut, p, bounds)?;
        }
        for x in [4, 12] {
            data.generate_box(
                context.region,
                bounds,
                [x, 1, 1],
                [x + 4, 3, 3],
                ss,
                air,
                false,
            )?;
            data.generate_box(
                context.region,
                bounds,
                [x, 1, 2],
                [x + 4, 2, 2],
                air,
                air,
                false,
            )?;
        }
        data.generate_box(
            context.region,
            bounds,
            [5, 4, 5],
            [w - 6, 4, d - 6],
            ss,
            ss,
            false,
        )?;
        data.generate_box(
            context.region,
            bounds,
            [9, 4, 9],
            [11, 4, 11],
            air,
            air,
            false,
        )?;
        for [x, z] in [[8, 8], [12, 8], [8, 12], [12, 12]] {
            data.generate_box(
                context.region,
                bounds,
                [x, 1, z],
                [x, 3, z],
                cut,
                cut,
                false,
            )?;
        }
        for (min, max, edge, fill) in [
            ([1, 1, 5], [4, 4, 11], ss, ss),
            ([w - 5, 1, 5], [w - 2, 4, 11], ss, ss),
            ([6, 7, 9], [6, 7, 11], ss, ss),
            ([w - 7, 7, 9], [w - 7, 7, 11], ss, ss),
            ([5, 5, 9], [5, 7, 11], cut, cut),
            ([w - 6, 5, 9], [w - 6, 7, 11], cut, cut),
        ] {
            data.generate_box(context.region, bounds, min, max, edge, fill, false)?;
        }
        for p in [
            [5, 5, 10],
            [5, 6, 10],
            [6, 6, 10],
            [w - 6, 5, 10],
            [w - 6, 6, 10],
            [w - 7, 6, 10],
        ] {
            data.place_block(context.region, air, p, bounds)?;
        }
        for x in [2, w - 3] {
            data.generate_box(
                context.region,
                bounds,
                [x, 4, 4],
                [x, 6, 4],
                air,
                air,
                false,
            )?;
        }
        for x in [2, w - 3] {
            for p in [[x, 4, 5], [x, 3, 4]] {
                data.place_block(context.region, stairs(Facing::North), p, bounds)?;
            }
        }
        for (min, max) in [([1, 1, 3], [2, 2, 3]), ([w - 3, 1, 3], [w - 2, 2, 3])] {
            data.generate_box(context.region, bounds, min, max, ss, ss, false)?;
        }
        for x in [1, w - 2] {
            data.place_block(context.region, ss, [x, 1, 2], bounds)?;
        }
        for x in [1, w - 2] {
            data.place_block(
                context.region,
                Block::SANDSTONE_SLAB.default_state(),
                [x, 2, 2],
                bounds,
            )?;
        }
        data.place_block(context.region, stairs(Facing::West), [2, 1, 2], bounds)?;
        data.place_block(context.region, stairs(Facing::East), [w - 3, 1, 2], bounds)?;
        for x in [4, w - 5] {
            data.generate_box(context.region, bounds, [x, 3, 5], [x, 3, 17], ss, ss, false)?;
        }
        for (min, max) in [([3, 1, 5], [4, 2, 16]), ([w - 6, 1, 5], [w - 5, 2, 16])] {
            data.generate_box(context.region, bounds, min, max, air, air, false)?;
        }
        for z in (5..=17).step_by(2) {
            for x in [4, w - 5] {
                data.place_block(context.region, cut, [x, 1, z], bounds)?;
                data.place_block(context.region, chisel, [x, 2, z], bounds)?;
            }
        }
        for [x, z] in [
            [10, 7],
            [10, 8],
            [9, 9],
            [11, 9],
            [8, 10],
            [12, 10],
            [7, 10],
            [13, 10],
            [9, 11],
            [11, 11],
            [10, 12],
            [10, 13],
        ] {
            data.place_block(context.region, orange, [x, 0, z], bounds)?;
        }
        data.place_block(
            context.region,
            Block::BLUE_TERRACOTTA.default_state(),
            [10, 0, 10],
            bounds,
        )?;
        // The two tower faces share the same seven-row carved pattern.
        let rows = [
            [cut, orange, cut],
            [cut, orange, cut],
            [orange, chisel, orange],
            [cut, orange, cut],
            [orange, chisel, orange],
            [orange, orange, orange],
            [cut, cut, cut],
        ];
        for x in [0, w - 1] {
            for (dy, row) in rows.iter().enumerate() {
                for (dz, &state) in row.iter().enumerate() {
                    data.place_block(
                        context.region,
                        state,
                        [x, dy as i32 + 2, dz as i32 + 1],
                        bounds,
                    )?;
                }
            }
        }
        for x in [2, w - 3] {
            for (dy, row) in rows.iter().enumerate() {
                for (dx, &state) in row.iter().enumerate() {
                    data.place_block(
                        context.region,
                        state,
                        [x + dx as i32 - 1, dy as i32 + 2, 0],
                        bounds,
                    )?;
                }
            }
        }
        data.generate_box(
            context.region,
            bounds,
            [8, 4, 0],
            [12, 6, 0],
            cut,
            cut,
            false,
        )?;
        for p in [[8, 6, 0], [12, 6, 0]] {
            data.place_block(context.region, air, p, bounds)?;
        }
        for (state, p) in [
            (orange, [9, 5, 0]),
            (chisel, [10, 5, 0]),
            (orange, [11, 5, 0]),
        ] {
            data.place_block(context.region, state, p, bounds)?;
        }
        for (min, max, state) in [
            ([8, -14, 8], [12, -11, 12], cut),
            ([8, -10, 8], [12, -10, 12], chisel),
            ([8, -9, 8], [12, -9, 12], cut),
            ([8, -8, 8], [12, -1, 12], ss),
            ([9, -11, 9], [11, -1, 11], air),
        ] {
            data.generate_box(context.region, bounds, min, max, state, state, false)?;
        }
        data.place_block(
            context.region,
            Block::STONE_PRESSURE_PLATE.default_state(),
            [10, -11, 10],
            bounds,
        )?;
        data.generate_box(
            context.region,
            bounds,
            [9, -13, 9],
            [11, -13, 11],
            Block::TNT.default_state(),
            air,
            false,
        )?;
        for (air_x, air_z, cut_x, cut_z) in [
            (8, 10, 7, 10),
            (12, 10, 13, 10),
            (10, 8, 10, 7),
            (10, 12, 10, 13),
        ] {
            data.place_block(context.region, air, [air_x, -11, air_z], bounds)?;
            data.place_block(context.region, air, [air_x, -10, air_z], bounds)?;
            data.place_block(context.region, chisel, [cut_x, -10, cut_z], bounds)?;
            data.place_block(context.region, cut, [cut_x, -11, cut_z], bounds)?;
        }
        let loot = "minecraft:chests/desert_pyramid".parse()?;
        for direction in Plane::Horizontal.faces() {
            let i = direction.data_2d() as usize;
            if !self.placed_chests[i] {
                let step = direction.step();
                self.placed_chests[i] = data.create_chest(
                    context,
                    [10 + step.x * 2, -11, 10 + step.z * 2],
                    bounds,
                    &loot,
                )?;
            }
        }
        self.add_cellar(context, bounds)
    }
    fn add_cellar(
        &mut self,
        context: &mut FeatureContext<'_, '_>,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        let data = &self.scattered.data;
        let ss = Block::SANDSTONE.default_state();
        let cut = Block::CUT_SANDSTONE.default_state();
        let chisel = Block::CHISELED_SANDSTONE.default_state();
        let sand = Block::SAND.default_state();
        let orange = Block::ORANGE_TERRACOTTA.default_state();
        let stairs = Block::SANDSTONE_STAIRS
            .default_state()
            .rotate(Rotation::CounterClockwise90);
        for p in [[13, -1, 17], [14, -2, 17], [15, -3, 17]] {
            data.place_block(context.region, stairs, p, bounds)?;
        }
        let variant = context.region.random_mut()?.next_boolean();
        for p in [
            [12, 0, 17],
            [13, 0, 17],
            [14, 0, 17],
            [15, 0, 17],
            [16, 0, 17],
            [14, -1, 17],
        ] {
            data.place_block(context.region, sand, p, bounds)?;
        }
        data.place_block(
            context.region,
            if variant { sand } else { ss },
            [15, -1, 17],
            bounds,
        )?;
        data.place_block(
            context.region,
            if !variant { sand } else { ss },
            [16, -1, 17],
            bounds,
        )?;
        for (state, p) in [
            (sand, [15, -2, 17]),
            (ss, [16, -2, 17]),
            (sand, [16, -3, 17]),
        ] {
            data.place_block(context.region, state, p, bounds)?;
        }
        for (y, state) in [(-3, cut), (-2, chisel), (-1, cut)] {
            for (min, max) in [
                ([13, y, 10], [13, y, 15]),
                ([19, y, 10], [19, y, 15]),
                ([13, y, 10], [19, y, 11]),
                ([13, y, 16], [19, y, 16]),
            ] {
                data.generate_box(context.region, bounds, min, max, state, state, true)?;
            }
        }
        for y in -3..=-1 {
            for x in 14..=18 {
                for z in 11..=15 {
                    self.potential_suspicious_sand
                        .push(data.world_position([x, y, z]));
                }
            }
        }
        for x in 14..=18 {
            for z in 11..=15 {
                let state = if context.region.random_mut()?.next_float() < 0.33 {
                    ss
                } else {
                    sand
                };
                data.place_block(context.region, state, [x, 0, z], bounds)?;
            }
        }
        let at = data.world_position([14, 0, 11]);
        let mut random = LegacyRandomSource::new(context.region.seed)
            .fork_positional()
            .at(at[0], at[1], at[2]);
        let x = 14 + random.next_int_bound(5);
        let z = 11 + random.next_int_bound(5);
        self.collapsed_roof = data.world_position([x, 0, z]);
        data.place_block(
            context.region,
            Block::BLUE_TERRACOTTA.default_state(),
            [16, -4, 13],
            bounds,
        )?;
        for [x, z] in [
            [17, 12],
            [17, 14],
            [15, 12],
            [15, 14],
            [18, 13],
            [14, 13],
            [16, 15],
            [16, 11],
            [19, 13],
        ] {
            data.place_block(context.region, orange, [x, -4, z], bounds)?;
        }
        for p in [[19, -3, 13], [19, -2, 13]] {
            self.potential_suspicious_sand.push(data.world_position(p));
        }
        for (state, p) in [
            (cut, [20, -3, 13]),
            (chisel, [20, -2, 13]),
            (orange, [13, -4, 13]),
        ] {
            data.place_block(context.region, state, p, bounds)?;
        }
        for p in [[13, -3, 13], [13, -2, 13]] {
            self.potential_suspicious_sand.push(data.world_position(p));
        }
        for (state, p) in [
            (cut, [12, -3, 13]),
            (chisel, [12, -2, 13]),
            (orange, [16, -4, 16]),
        ] {
            data.place_block(context.region, state, p, bounds)?;
        }
        for p in [[16, -3, 16], [16, -2, 16]] {
            self.potential_suspicious_sand.push(data.world_position(p));
        }
        data.place_block(context.region, orange, [16, -4, 10], bounds)?;
        for p in [[16, -3, 10], [16, -2, 10]] {
            self.potential_suspicious_sand.push(data.world_position(p));
        }
        for (state, p) in [(cut, [16, -3, 9]), (chisel, [16, -2, 9])] {
            data.place_block(context.region, state, p, bounds)?;
        }
        Ok(())
    }
}
/// Port of DesertPyramidStructure#afterPlace. Sorting is Vec3i's Y/Z/X order;
/// shuffled selections include positions outside this chunk before clipping.
pub fn archaeology_placements(
    pieces: &[crate::structure::start::StructurePiece],
    seed: i64,
) -> Result<Vec<([i32; 3], bool)>, Error> {
    let mut candidates = BTreeSet::new();
    let mut combined = None;
    let mut placements = Vec::new();
    for piece in pieces {
        if let crate::structure::start::StructurePiece::DesertPyramid(piece) = piece {
            for &[x, y, z] in &piece.potential_suspicious_sand {
                candidates.insert((y, z, x));
            }
            placements.push((piece.collapsed_roof, true));
        }
        combined = Some(combined.map_or(piece.bounds(), |b: BoundingBox| {
            b.encapsulating(piece.bounds())
        }));
    }
    let combined =
        combined.ok_or_else(|| Error("Unable to calculate boundingbox without pieces".into()))?;
    let [x, y, z] = combined.center();
    let mut random = LegacyRandomSource::new(seed).fork_positional().at(x, y, z);
    let mut candidates = candidates
        .into_iter()
        .map(|(y, z, x)| [x, y, z])
        .collect::<Vec<_>>();
    for i in (2..=candidates.len()).rev() {
        let j = random.next_int_bound(i as i32) as usize;
        candidates.swap(i - 1, j);
    }
    let count = candidates.len().min(random.next_int_bound(3) as usize + 5);
    placements.extend(
        candidates
            .into_iter()
            .enumerate()
            .map(|(i, p)| (p, i < count)),
    );
    Ok(placements)
}
pub fn after_place(
    pieces: &[crate::structure::start::StructurePiece],
    context: &mut FeatureContext<'_, '_>,
    bounds: BoundingBox,
) -> Result<(), Error> {
    for (p, suspicious) in archaeology_placements(pieces, context.region.seed)? {
        if !bounds.contains(p) {
            continue;
        }
        if suspicious {
            context
                .region
                .set_block(p, Block::SUSPICIOUS_SAND.default_state(), 2)?;
            context.region.set_loot_table_seed(
                p,
                &"minecraft:archaeology/desert_pyramid".parse()?,
                BlockPos::new(p[0], p[1], p[2]).as_long(),
            )?;
        } else {
            context
                .region
                .set_block(p, Block::SAND.default_state(), 2)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        feature::FeatureRegistry,
        region::{ChunkStatus, RegionChunk, WorldGenRegion},
        structure::{
            processor::tests::{StaticBlocks, generator},
            start::StructurePiece,
            structures::buried_treasure::BuriedTreasurePiece,
        },
    };
    use ferraloom_util::random::{Random, WorldgenRandom};
    use serde_json::Value;
    use std::io::Read;

    fn point(v: &Value) -> [i32; 3] {
        std::array::from_fn(|i| v[i].as_i64().unwrap() as i32)
    }
    fn writes(v: &Value) -> Vec<([i32; 3], u16)> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|v| (point(&v["pos"]), v["state"].as_u64().unwrap() as u16))
            .collect()
    }
    #[test]
    fn pyramid_geometry_cellar_stream_and_archaeology_order_match_vanilla() {
        let (_, generator) = generator();
        let features = FeatureRegistry::default();
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../testdata/desert_pyramid_blocks.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let mode = case["mode"].as_i64().unwrap();
            let seed = case["seed"].as_i64().unwrap();
            let center = std::array::from_fn(|i| case["center"][i].as_i64().unwrap() as i32);
            let mut chunks = Vec::new();
            for cz in 0..2 {
                for cx in 0..2 {
                    let mut chunk = generator.new_chunk([cx, cz]);
                    for z in 0..16 {
                        for x in 0..16 {
                            let ground = if mode == 0 {
                                63
                            } else {
                                60 + ((cx * 16 + x) * 3 + (cz * 16 + z) * 5) % 8
                            };
                            for y in -64..=ground {
                                chunk.set_block(x as u8, y, z as u8, Block::STONE.default_state());
                            }
                        }
                    }
                    chunks.push(RegionChunk::new(chunk, ChunkStatus::Terrain));
                }
            }
            let mut region = WorldGenRegion::new(
                &mut chunks,
                center,
                &[ChunkStatus::Terrain; 2],
                0,
                None,
                &StaticBlocks,
                seed,
                63,
            )
            .unwrap();
            region.set_random(Random::Legacy(LegacyRandomSource::new(777)));
            let mut piece = DesertPyramidPiece::from_nbt(
                &ferraloom_nbt::parse_compound(case["initial"].as_str().unwrap()).unwrap(),
            )
            .unwrap();
            let b = &case["bounds"];
            let bounds = BoundingBox::new(
                point(b),
                std::array::from_fn(|i| b[i + 3].as_i64().unwrap() as i32),
            );
            let mut random = WorldgenRandom::new(LegacyRandomSource::new(17));
            piece
                .post_process(
                    &mut FeatureContext {
                        region: &mut region,
                        random: &mut random,
                        origin: [0; 3],
                        generator: &generator,
                        registry: &features,
                    },
                    bounds,
                )
                .unwrap();
            let label = format!(
                "direction={} mode={mode} center={center:?} seed={seed}",
                case["direction"]
            );
            assert_eq!(
                piece.to_nbt(),
                ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap(),
                "{label}"
            );
            assert_eq!(
                random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "{label}"
            );
            assert_eq!(
                region.random_mut().unwrap().next_int(),
                case["region_next"].as_i64().unwrap() as i32,
                "{label}"
            );
            assert_eq!(
                piece.potential_suspicious_sand,
                case["potential"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(point)
                    .collect::<Vec<_>>(),
                "{label}"
            );
            assert_eq!(piece.collapsed_roof, point(&case["roof"]), "{label}");
            let actual = region
                .chunk(center, ChunkStatus::Terrain)
                .unwrap()
                .poi_changes
                .iter()
                .map(|(p, _, s)| (*p, s.id()))
                .collect::<Vec<_>>();
            let expected = writes(&case["writes"]);
            assert_eq!(actual.len(), expected.len(), "{label}");
            for (i, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(actual, expected, "{label} write={i}");
            }
            let mut pieces = vec![StructurePiece::DesertPyramid(Box::new(piece))];
            if case["mixed"].as_bool().unwrap() {
                pieces.push(StructurePiece::BuriedTreasure(BuriedTreasurePiece::new([
                    100, 20, -40,
                ])));
            }
            let actual = archaeology_placements(&pieces, seed)
                .unwrap()
                .into_iter()
                .filter(|(p, _)| bounds.contains(*p))
                .map(|(p, suspicious)| {
                    (
                        p,
                        if suspicious {
                            Block::SUSPICIOUS_SAND
                        } else {
                            Block::SAND
                        }
                        .default_state()
                        .id(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                writes(&case["archaeology"]),
                "{label} archaeology mixed={}",
                case["mixed"]
            );
        }
    }
}
