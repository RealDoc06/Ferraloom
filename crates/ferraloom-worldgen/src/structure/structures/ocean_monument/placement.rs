use super::{
    MonumentRoom,
    graph::{RoomGraph, RoomKind},
};
use crate::{
    Error,
    feature::FeatureContext,
    structure::{BoundingBox, piece::StructurePieceData},
};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{Direction, random::RandomSource};
const GRAY: Block = Block::PRISMARINE;
const LIGHT: Block = Block::PRISMARINE_BRICKS;
const BLACK: Block = Block::DARK_PRISMARINE;
const LAMP: Block = Block::SEA_LANTERN;
struct RoomPlacer<'a, 'b, 'c, 'd> {
    data: &'a StructurePieceData,
    context: &'b mut FeatureContext<'c, 'd>,
    bounds: BoundingBox,
}
impl RoomPlacer<'_, '_, '_, '_> {
    fn block(&self, p: [i32; 3]) -> Result<BlockState, Error> {
        let p = self.data.world_position(p);
        if self.bounds.contains(p) {
            self.context.region.block(p)
        } else {
            Ok(Block::AIR.default_state())
        }
    }
    fn put(&mut self, p: [i32; 3], block: Block) -> Result<(), Error> {
        self.data
            .place_block(self.context.region, block.default_state(), p, self.bounds)
    }
    fn fill(&mut self, a: [i32; 3], b: [i32; 3], block: Block) -> Result<(), Error> {
        self.data.generate_box(
            self.context.region,
            self.bounds,
            a,
            b,
            block.default_state(),
            block.default_state(),
            false,
        )
    }
    fn boxes(&mut self, boxes: &[([i32; 3], [i32; 3])], block: Block) -> Result<(), Error> {
        for &(a, b) in boxes {
            self.fill(a, b, block)?;
        }
        Ok(())
    }
    fn fill_water_only(&mut self, a: [i32; 3], b: [i32; 3], block: Block) -> Result<(), Error> {
        for y in a[1]..=b[1] {
            for x in a[0]..=b[0] {
                for z in a[2]..=b[2] {
                    let p = [x, y, z];
                    if self.block(p)? == Block::WATER.default_state() {
                        self.put(p, block)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn water(&mut self, a: [i32; 3], b: [i32; 3]) -> Result<(), Error> {
        for y in a[1]..=b[1] {
            for x in a[0]..=b[0] {
                for z in a[2]..=b[2] {
                    let p = [x, y, z];
                    let state = self.block(p)?;
                    if !matches!(
                        state.block(),
                        Block::ICE | Block::PACKED_ICE | Block::BLUE_ICE | Block::WATER
                    ) {
                        let block = if self.data.world_position(p)[1]
                            >= self.context.region.sea_level
                            && state != Block::WATER.default_state()
                        {
                            Block::AIR
                        } else {
                            Block::WATER
                        };
                        self.put(p, block)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn floor(&mut self, x: i32, z: i32, down: bool) -> Result<(), Error> {
        if down {
            for (a, b) in [
                ([0, 0, 0], [2, 0, 7]),
                ([5, 0, 0], [7, 0, 7]),
                ([3, 0, 0], [4, 0, 2]),
                ([3, 0, 5], [4, 0, 7]),
            ] {
                self.fill([a[0] + x, 0, a[2] + z], [b[0] + x, 0, b[2] + z], GRAY)?;
            }
            for (a, b) in [
                ([3, 0, 2], [4, 0, 2]),
                ([3, 0, 5], [4, 0, 5]),
                ([2, 0, 3], [2, 0, 4]),
                ([5, 0, 3], [5, 0, 4]),
            ] {
                self.fill([a[0] + x, 0, a[2] + z], [b[0] + x, 0, b[2] + z], LIGHT)?;
            }
        } else {
            self.fill([x, 0, z], [x + 7, 0, z + 7], GRAY)?;
        }
        Ok(())
    }
    fn entry(&mut self, openings: [bool; 6]) -> Result<(), Error> {
        self.boxes(
            &[
                ([0, 3, 0], [2, 3, 7]),
                ([5, 3, 0], [7, 3, 7]),
                ([0, 2, 0], [1, 2, 7]),
                ([6, 2, 0], [7, 2, 7]),
                ([0, 1, 0], [0, 1, 7]),
                ([7, 1, 0], [7, 1, 7]),
                ([0, 1, 7], [7, 3, 7]),
                ([1, 1, 0], [2, 3, 0]),
                ([5, 1, 0], [6, 3, 0]),
            ],
            LIGHT,
        )?;
        for (direction, a, b) in [
            (Direction::North, [3, 1, 7], [4, 2, 7]),
            (Direction::West, [0, 1, 3], [1, 2, 4]),
            (Direction::East, [6, 1, 3], [7, 2, 4]),
        ] {
            if openings[direction as usize] {
                self.water(a, b)?;
            }
        }
        Ok(())
    }
    fn core(&mut self) -> Result<(), Error> {
        self.fill_water_only([1, 8, 0], [14, 8, 14], GRAY)?;
        self.boxes(
            &[
                ([0, 7, 0], [0, 7, 15]),
                ([15, 7, 0], [15, 7, 15]),
                ([1, 7, 0], [15, 7, 0]),
                ([1, 7, 15], [14, 7, 15]),
            ],
            LIGHT,
        )?;
        for y in 1..=6 {
            let block = if y == 2 || y == 6 { GRAY } else { LIGHT };
            for x in [0, 15] {
                for (z0, z1) in [(0, 1), (6, 9), (14, 15)] {
                    self.fill([x, y, z0], [x, y, z1], block)?;
                }
            }
            for (x0, z0, x1, z1) in [(1, 0, 1, 0), (6, 0, 9, 0), (14, 0, 14, 0), (1, 15, 14, 15)] {
                self.fill([x0, y, z0], [x1, y, z1], block)?;
            }
        }
        self.fill([6, 3, 6], [9, 6, 9], BLACK)?;
        self.fill([7, 4, 7], [8, 5, 8], Block::GOLD_BLOCK)?;
        for y in [3, 6] {
            for x in [6, 9] {
                self.put([x, y, 6], LAMP)?;
                self.put([x, y, 9], LAMP)?;
            }
        }
        self.boxes(
            &[
                ([5, 1, 6], [5, 2, 6]),
                ([5, 1, 9], [5, 2, 9]),
                ([10, 1, 6], [10, 2, 6]),
                ([10, 1, 9], [10, 2, 9]),
                ([6, 1, 5], [6, 2, 5]),
                ([9, 1, 5], [9, 2, 5]),
                ([6, 1, 10], [6, 2, 10]),
                ([9, 1, 10], [9, 2, 10]),
                ([5, 2, 5], [5, 6, 5]),
                ([5, 2, 10], [5, 6, 10]),
                ([10, 2, 5], [10, 6, 5]),
                ([10, 2, 10], [10, 6, 10]),
                ([5, 7, 1], [5, 7, 6]),
                ([10, 7, 1], [10, 7, 6]),
                ([5, 7, 9], [5, 7, 14]),
                ([10, 7, 9], [10, 7, 14]),
                ([1, 7, 5], [6, 7, 5]),
                ([1, 7, 10], [6, 7, 10]),
                ([9, 7, 5], [14, 7, 5]),
                ([9, 7, 10], [14, 7, 10]),
                ([2, 1, 2], [2, 1, 3]),
                ([3, 1, 2], [3, 1, 2]),
                ([13, 1, 2], [13, 1, 3]),
                ([12, 1, 2], [12, 1, 2]),
                ([2, 1, 12], [2, 1, 13]),
                ([3, 1, 13], [3, 1, 13]),
                ([13, 1, 12], [13, 1, 13]),
                ([12, 1, 13], [12, 1, 13]),
            ],
            LIGHT,
        )
    }
    fn simple(&mut self, graph: &RoomGraph, id: usize, design: i32) -> Result<(), Error> {
        let room = graph.room(id);
        if room.index / 25 > 0 {
            self.floor(0, 0, room.openings[Direction::Down as usize])?;
        }
        if room.connections[Direction::Up as usize].is_none() {
            self.fill_water_only([1, 4, 1], [6, 4, 6], GRAY)?;
        }
        let pillar = design != 0
            && self.context.random.next_boolean()
            && !room.openings[Direction::Down as usize]
            && !room.openings[Direction::Up as usize]
            && room.openings.iter().filter(|&&v| v).count() > 1;
        match design {
            0 => {
                for (x, z) in [(0, 0), (5, 0), (0, 5), (5, 5)] {
                    self.fill([x, 1, z], [x + 2, 1, z + 2], LIGHT)?;
                    self.fill([x, 3, z], [x + 2, 3, z + 2], LIGHT)?;
                    let side_x = if x == 0 { 0 } else { 7 };
                    let side_z = if z == 0 { 0 } else { 7 };
                    self.fill([side_x, 2, z], [side_x, 2, z + 2], GRAY)?;
                    self.fill(
                        [if x == 0 { 1 } else { 5 }, 2, side_z],
                        [if x == 0 { 2 } else { 6 }, 2, side_z],
                        GRAY,
                    )?;
                    self.put(
                        [if x == 0 { 1 } else { 6 }, 2, if z == 0 { 1 } else { 6 }],
                        LAMP,
                    )?;
                }
                for (direction, a, b, closed_a, closed_b, gray_a, gray_b) in [
                    (
                        Direction::South,
                        [3, 3, 0],
                        [4, 3, 0],
                        [3, 3, 0],
                        [4, 3, 1],
                        [3, 2, 0],
                        [4, 2, 0],
                    ),
                    (
                        Direction::North,
                        [3, 3, 7],
                        [4, 3, 7],
                        [3, 3, 6],
                        [4, 3, 7],
                        [3, 2, 7],
                        [4, 2, 7],
                    ),
                    (
                        Direction::West,
                        [0, 3, 3],
                        [0, 3, 4],
                        [0, 3, 3],
                        [1, 3, 4],
                        [0, 2, 3],
                        [0, 2, 4],
                    ),
                    (
                        Direction::East,
                        [7, 3, 3],
                        [7, 3, 4],
                        [6, 3, 3],
                        [7, 3, 4],
                        [7, 2, 3],
                        [7, 2, 4],
                    ),
                ] {
                    if room.openings[direction as usize] {
                        self.fill(a, b, LIGHT)?;
                    } else {
                        self.fill(closed_a, closed_b, LIGHT)?;
                        self.fill(gray_a, gray_b, GRAY)?;
                        self.fill(
                            [closed_a[0], 1, closed_a[2]],
                            [closed_b[0], 1, closed_b[2]],
                            LIGHT,
                        )?;
                    }
                }
            }
            1 => {
                for (x, z) in [(2, 2), (2, 5), (5, 5), (5, 2)] {
                    self.fill([x, 1, z], [x, 3, z], LIGHT)?;
                }
                for (x, z) in [(2, 2), (2, 5), (5, 5), (5, 2)] {
                    self.put([x, 2, z], LAMP)?;
                }
                self.boxes(
                    &[
                        ([0, 1, 0], [1, 3, 0]),
                        ([0, 1, 1], [0, 3, 1]),
                        ([0, 1, 7], [1, 3, 7]),
                        ([0, 1, 6], [0, 3, 6]),
                        ([6, 1, 7], [7, 3, 7]),
                        ([7, 1, 6], [7, 3, 6]),
                        ([6, 1, 0], [7, 3, 0]),
                        ([7, 1, 1], [7, 3, 1]),
                    ],
                    LIGHT,
                )?;
                for p in [
                    [1, 2, 0],
                    [0, 2, 1],
                    [1, 2, 7],
                    [0, 2, 6],
                    [6, 2, 7],
                    [7, 2, 6],
                    [6, 2, 0],
                    [7, 2, 1],
                ] {
                    self.put(p, GRAY)?;
                }
                for (direction, a, b) in [
                    (Direction::South, [1, 0, 0], [6, 0, 0]),
                    (Direction::North, [1, 0, 7], [6, 0, 7]),
                    (Direction::West, [0, 0, 1], [0, 0, 6]),
                    (Direction::East, [7, 0, 1], [7, 0, 6]),
                ] {
                    if !room.openings[direction as usize] {
                        for y in (1..=3).rev() {
                            self.fill(
                                [a[0], y, a[2]],
                                [b[0], y, b[2]],
                                if y == 2 { GRAY } else { LIGHT },
                            )?;
                        }
                    }
                }
            }
            2 => {
                for y in 1..=3 {
                    self.boxes(
                        &[
                            ([0, y, 0], [0, y, 7]),
                            ([7, y, 0], [7, y, 7]),
                            ([1, y, 0], [6, y, 0]),
                            ([1, y, 7], [6, y, 7]),
                        ],
                        if y == 2 { BLACK } else { LIGHT },
                    )?;
                }
                self.boxes(
                    &[
                        ([0, 1, 3], [0, 2, 4]),
                        ([7, 1, 3], [7, 2, 4]),
                        ([3, 1, 0], [4, 2, 0]),
                        ([3, 1, 7], [4, 2, 7]),
                    ],
                    BLACK,
                )?;
                for (direction, a, b) in [
                    (Direction::South, [3, 1, 0], [4, 2, 0]),
                    (Direction::North, [3, 1, 7], [4, 2, 7]),
                    (Direction::West, [0, 1, 3], [0, 2, 4]),
                    (Direction::East, [7, 1, 3], [7, 2, 4]),
                ] {
                    if room.openings[direction as usize] {
                        self.water(a, b)?;
                    }
                }
            }
            _ => return Err(Error("invalid monument simple-room design".into())),
        }
        if pillar {
            for y in 1..=3 {
                self.fill([3, y, 3], [4, y, 4], if y == 2 { GRAY } else { LIGHT })?;
            }
        }
        Ok(())
    }
    fn simple_top(&mut self, graph: &RoomGraph, id: usize) -> Result<(), Error> {
        let room = graph.room(id);
        if room.index / 25 > 0 {
            self.floor(0, 0, room.openings[Direction::Down as usize])?;
        }
        if room.connections[Direction::Up as usize].is_none() {
            self.fill_water_only([1, 4, 1], [6, 4, 6], GRAY)?;
        }
        for x in 1..=6 {
            for z in 1..=6 {
                if self.context.random.next_int_bound(3) != 0 {
                    let y = 2 + if self.context.random.next_int_bound(4) == 0 {
                        0
                    } else {
                        1
                    };
                    self.fill([x, y, z], [x, 3, z], Block::WET_SPONGE)?;
                }
            }
        }
        for y in 1..=3 {
            let block = if y == 2 { BLACK } else { LIGHT };
            self.boxes(
                &[
                    ([0, y, 0], [0, y, 7]),
                    ([7, y, 0], [7, y, 7]),
                    ([1, y, 0], [6, y, 0]),
                    ([1, y, 7], [6, y, 7]),
                ],
                block,
            )?;
        }
        self.boxes(
            &[
                ([0, 1, 3], [0, 2, 4]),
                ([7, 1, 3], [7, 2, 4]),
                ([3, 1, 0], [4, 2, 0]),
                ([3, 1, 7], [4, 2, 7]),
            ],
            BLACK,
        )?;
        if room.openings[Direction::South as usize] {
            self.water([3, 1, 0], [4, 2, 0])?;
        }
        Ok(())
    }
}
impl MonumentRoom {
    /// Port of the monument child-room postProcess implementations. Unported
    /// room families remain explicit errors while their geometry is added.
    pub fn post_process(
        &self,
        context: &mut FeatureContext<'_, '_>,
        graph: &RoomGraph,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        let mut placer = RoomPlacer {
            data: &self.data,
            context,
            bounds,
        };
        match self.kind {
            RoomKind::Entry => {
                placer.entry(graph.room(self.room.expect("entry definition")).openings)
            }
            RoomKind::Core => placer.core(),
            RoomKind::Simple { design } => {
                placer.simple(graph, self.room.expect("simple definition"), design)
            }
            RoomKind::SimpleTop => placer.simple_top(graph, self.room.expect("sponge definition")),
            _ => Err(Error(format!(
                "Unsupported monument room geometry {:?}",
                self.kind
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        feature::FeatureRegistry,
        region::{ChunkStatus, RegionChunk, WorldGenRegion},
        structure::{
            processor::tests::{StaticBlocks, generator},
            structures::ocean_monument::MonumentBuilding,
        },
    };
    use ferraloom_util::random::{Algorithm, LegacyRandomSource, WorldgenRandom};
    use std::io::Read;
    fn initial(mode: i32, [x, y, z]: [i32; 3]) -> BlockState {
        if mode == 0 {
            return if y < 80 { Block::WATER } else { Block::AIR }.default_state();
        }
        match (x * 3 + y * 5 + z).rem_euclid(8) {
            0 => Block::WATER.default_state(),
            1 => Block::WATER
                .default_state()
                .with(ferraloom_data::properties::LEVEL, 3)
                .unwrap(),
            2 => Block::ICE.default_state(),
            3 => Block::PACKED_ICE.default_state(),
            4 => Block::BLUE_ICE.default_state(),
            5 => Block::STONE.default_state(),
            _ => Block::AIR.default_state(),
        }
    }
    #[test]
    fn monument_single_core_and_sponge_geometry_clipping_water_and_rng_match_vanilla() {
        let (_, generator) = generator();
        let features = FeatureRegistry::default();
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../../testdata/monument_room_blocks.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: serde_json::Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let seed = case["seed"].as_i64().unwrap();
            let direction =
                Direction::from_2d_data_value(case["direction"].as_i64().unwrap() as i32);
            let mode = case["mode"].as_i64().unwrap() as i32;
            let building =
                MonumentBuilding::new(&mut LegacyRandomSource::new(seed), -46, 8, direction);
            let mut child = building.children[case["child"].as_u64().unwrap() as usize].clone();
            let min = child.data.bounds.min;
            child.data.bounds = child.data.bounds.moved([-min[0], 64 - min[1], -min[2]]);
            assert_eq!(
                child.data.to_nbt(&child.kind.piece_id().parse().unwrap()),
                ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap()
            );
            let mut chunk = generator.new_chunk([0, 0]);
            for x in 0..16 {
                for z in 0..16 {
                    for y in 63..=80 {
                        chunk.set_block(x as u8, y, z as u8, initial(mode, [x, y, z]));
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
                67,
            )
            .unwrap();
            let b = &case["bounds"];
            let bounds = BoundingBox::new(
                std::array::from_fn(|i| b[i].as_i64().unwrap() as i32),
                std::array::from_fn(|i| b[i + 3].as_i64().unwrap() as i32),
            );
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(17));
            child
                .post_process(
                    &mut FeatureContext {
                        region: &mut region,
                        random: &mut random,
                        origin: [0; 3],
                        generator: &generator,
                        registry: &features,
                    },
                    &building.graph,
                    bounds,
                )
                .unwrap();
            assert_eq!(
                random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "seed={seed} direction={direction:?} kind={:?} mode={mode}",
                child.kind
            );
            let actual = region
                .chunk([0, 0], ChunkStatus::Terrain)
                .unwrap()
                .poi_changes
                .iter()
                .map(|(p, _, state)| (*p, state.id()))
                .collect::<Vec<_>>();
            let expected = case["writes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| {
                    (
                        std::array::from_fn(|i| w["pos"][i].as_i64().unwrap() as i32),
                        w["state"].as_u64().unwrap() as u16,
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actual, expected,
                "seed={seed} direction={direction:?} kind={:?} mode={mode} bounds={bounds:?}",
                child.kind
            );
        }
    }
}
