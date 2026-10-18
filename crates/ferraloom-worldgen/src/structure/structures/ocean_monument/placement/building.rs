use super::{BLACK, GRAY, LAMP, LIGHT, RoomPlacer};
use crate::{
    Error,
    feature::FeatureContext,
    structure::{BoundingBox, structures::ocean_monument::MonumentBuilding},
};
impl RoomPlacer<'_, '_, '_, '_> {
    fn intersects(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
        let a = self.data.world_position([x0, 0, z0]);
        let b = self.data.world_position([x1, 0, z1]);
        self.bounds.intersects_xz(
            [a[0].min(b[0]), a[2].min(b[2])],
            [a[0].max(b[0]), a[2].max(b[2])],
        )
    }
    fn outer_wing(&mut self, flipped: bool, x: i32) -> Result<(), Error> {
        if !self.intersects(x, 0, x + 23, 20) {
            return Ok(());
        }
        self.fill([x, 0, 0], [x + 24, 0, 20], GRAY)?;
        self.water([x, 1, 0], [x + 24, 10, 20])?;
        for i in 0..4 {
            self.boxes(
                &[
                    ([x + i, i + 1, i], [x + i, i + 1, 20]),
                    ([x + i + 7, i + 5, i + 7], [x + i + 7, i + 5, 20]),
                    ([x + 17 - i, i + 5, i + 7], [x + 17 - i, i + 5, 20]),
                    ([x + 24 - i, i + 1, i], [x + 24 - i, i + 1, 20]),
                    ([x + i + 1, i + 1, i], [x + 23 - i, i + 1, i]),
                    ([x + i + 8, i + 5, i + 7], [x + 16 - i, i + 5, i + 7]),
                ],
                LIGHT,
            )?;
        }
        self.boxes(
            &[
                ([x + 4, 4, 4], [x + 6, 4, 20]),
                ([x + 7, 4, 4], [x + 17, 4, 6]),
                ([x + 18, 4, 4], [x + 20, 4, 20]),
                ([x + 11, 8, 11], [x + 13, 8, 20]),
            ],
            GRAY,
        )?;
        for z in [12, 15, 18] {
            self.put([x + 12, 9, z], LIGHT)?;
        }
        let left = x + if flipped { 19 } else { 5 };
        let right = x + if flipped { 5 } else { 19 };
        for z in [20, 17, 14, 11, 8, 5] {
            self.put([left, 5, z], LIGHT)?;
        }
        for z in [19, 16, 13, 10, 7] {
            self.put([right, 5, z], LIGHT)?;
        }
        for i in 0..4 {
            self.put(
                [
                    if flipped {
                        x + 24 - (17 - i * 3)
                    } else {
                        x + 17 - i * 3
                    },
                    5,
                    5,
                ],
                LIGHT,
            )?;
        }
        self.put([right, 5, 5], LIGHT)?;
        self.boxes(
            &[
                ([x + 11, 1, 12], [x + 13, 7, 12]),
                ([x + 12, 1, 11], [x + 12, 7, 13]),
            ],
            GRAY,
        )
    }
    fn entrance_arches(&mut self) -> Result<(), Error> {
        if !self.intersects(22, 5, 35, 17) {
            return Ok(());
        }
        self.water([25, 0, 0], [32, 8, 20])?;
        for i in 0..4 {
            let z = 5 + i * 4;
            self.boxes(&[([24, 2, z], [24, 4, z]), ([22, 4, z], [23, 4, z])], LIGHT)?;
            self.put([25, 5, z], LIGHT)?;
            self.put([26, 6, z], LIGHT)?;
            self.put([26, 5, z], LAMP)?;
            self.boxes(&[([33, 2, z], [33, 4, z]), ([34, 4, z], [35, 4, z])], LIGHT)?;
            self.put([32, 5, z], LIGHT)?;
            self.put([31, 6, z], LIGHT)?;
            self.put([31, 5, z], LAMP)?;
            self.fill([27, 6, z], [30, 6, z], GRAY)?;
        }
        Ok(())
    }
    fn entrance_wall(&mut self) -> Result<(), Error> {
        if !self.intersects(15, 20, 42, 21) {
            return Ok(());
        }
        self.fill([15, 0, 21], [42, 0, 21], GRAY)?;
        self.water([26, 1, 21], [31, 3, 21])?;
        self.boxes(
            &[
                ([21, 12, 21], [36, 12, 21]),
                ([17, 11, 21], [40, 11, 21]),
                ([16, 10, 21], [41, 10, 21]),
                ([15, 7, 21], [42, 9, 21]),
                ([16, 6, 21], [41, 6, 21]),
                ([17, 5, 21], [40, 5, 21]),
                ([21, 4, 21], [36, 4, 21]),
                ([22, 3, 21], [26, 3, 21]),
                ([31, 3, 21], [35, 3, 21]),
                ([23, 2, 21], [25, 2, 21]),
                ([32, 2, 21], [34, 2, 21]),
            ],
            GRAY,
        )?;
        self.fill([28, 4, 20], [29, 4, 21], LIGHT)?;
        for p in [
            [27, 3, 21],
            [30, 3, 21],
            [26, 2, 21],
            [31, 2, 21],
            [25, 1, 21],
            [32, 1, 21],
        ] {
            self.put(p, LIGHT)?;
        }
        for i in 0..7 {
            self.put([28 - i, 6 + i, 21], BLACK)?;
            self.put([29 + i, 6 + i, 21], BLACK)?;
        }
        for i in 0..4 {
            self.put([28 - i, 9 + i, 21], BLACK)?;
            self.put([29 + i, 9 + i, 21], BLACK)?;
        }
        self.put([28, 12, 21], BLACK)?;
        self.put([29, 12, 21], BLACK)?;
        for i in 0..3 {
            for p in [
                [22 - i * 2, 8, 21],
                [22 - i * 2, 9, 21],
                [35 + i * 2, 8, 21],
                [35 + i * 2, 9, 21],
            ] {
                self.put(p, BLACK)?;
            }
        }
        for (a, b) in [
            ([15, 13, 21], [42, 15, 21]),
            ([15, 1, 21], [15, 6, 21]),
            ([16, 1, 21], [16, 5, 21]),
            ([17, 1, 21], [20, 4, 21]),
            ([21, 1, 21], [21, 3, 21]),
            ([22, 1, 21], [22, 2, 21]),
            ([23, 1, 21], [24, 1, 21]),
            ([42, 1, 21], [42, 6, 21]),
            ([41, 1, 21], [41, 5, 21]),
            ([37, 1, 21], [40, 4, 21]),
            ([36, 1, 21], [36, 3, 21]),
            ([33, 1, 21], [34, 1, 21]),
            ([35, 1, 21], [35, 2, 21]),
        ] {
            self.water(a, b)?;
        }
        Ok(())
    }
    fn roof(&mut self) -> Result<(), Error> {
        if !self.intersects(21, 21, 36, 36) {
            return Ok(());
        }
        self.fill([21, 0, 22], [36, 0, 36], GRAY)?;
        self.water([21, 1, 22], [36, 23, 36])?;
        for i in 0..4 {
            self.boxes(
                &[
                    ([21 + i, 13 + i, 21 + i], [36 - i, 13 + i, 21 + i]),
                    ([21 + i, 13 + i, 36 - i], [36 - i, 13 + i, 36 - i]),
                    ([21 + i, 13 + i, 22 + i], [21 + i, 13 + i, 35 - i]),
                    ([36 - i, 13 + i, 22 + i], [36 - i, 13 + i, 35 - i]),
                ],
                LIGHT,
            )?;
        }
        self.fill([25, 16, 25], [32, 16, 32], GRAY)?;
        self.boxes(
            &[
                ([25, 17, 25], [25, 19, 25]),
                ([32, 17, 25], [32, 19, 25]),
                ([25, 17, 32], [25, 19, 32]),
                ([32, 17, 32], [32, 19, 32]),
            ],
            LIGHT,
        )?;
        for (p, q) in [
            ([26, 20, 26], [27, 21, 27]),
            ([26, 20, 31], [27, 21, 30]),
            ([31, 20, 31], [30, 21, 30]),
            ([31, 20, 26], [30, 21, 27]),
        ] {
            self.put(p, LIGHT)?;
            self.put(q, LIGHT)?;
            self.put([q[0], 20, q[2]], LAMP)?;
        }
        self.boxes(
            &[
                ([28, 21, 27], [29, 21, 27]),
                ([27, 21, 28], [27, 21, 29]),
                ([28, 21, 30], [29, 21, 30]),
                ([30, 21, 28], [30, 21, 29]),
            ],
            GRAY,
        )
    }
    fn lower_walls(&mut self) -> Result<(), Error> {
        if self.intersects(0, 21, 6, 58) {
            self.fill([0, 0, 21], [6, 0, 57], GRAY)?;
            self.water([0, 1, 21], [6, 7, 57])?;
            self.fill([4, 4, 21], [6, 4, 53], GRAY)?;
            for i in 0..4 {
                self.fill([i, i + 1, 21], [i, i + 1, 57 - i], LIGHT)?;
            }
            for z in (23..53).step_by(3) {
                self.put([5, 5, z], LIGHT)?;
            }
            self.put([5, 5, 52], LIGHT)?;
            // Vanilla repeats this ledge after the decoration dots.
            for i in 0..4 {
                self.fill([i, i + 1, 21], [i, i + 1, 57 - i], LIGHT)?;
            }
            self.boxes(&[([4, 1, 52], [6, 3, 52]), ([5, 1, 51], [5, 3, 53])], GRAY)?;
        }
        if self.intersects(51, 21, 58, 58) {
            self.fill([51, 0, 21], [57, 0, 57], GRAY)?;
            self.water([51, 1, 21], [57, 7, 57])?;
            self.fill([51, 4, 21], [53, 4, 53], GRAY)?;
            for i in 0..4 {
                self.fill([57 - i, i + 1, 21], [57 - i, i + 1, 57 - i], LIGHT)?;
            }
            for z in (23..53).step_by(3) {
                self.put([52, 5, z], LIGHT)?;
            }
            self.put([52, 5, 52], LIGHT)?;
            self.boxes(
                &[([51, 1, 52], [53, 3, 52]), ([52, 1, 51], [52, 3, 53])],
                GRAY,
            )?;
        }
        if self.intersects(0, 51, 57, 57) {
            self.fill([7, 0, 51], [50, 0, 57], GRAY)?;
            self.water([7, 1, 51], [50, 10, 57])?;
            for i in 0..4 {
                self.fill([i + 1, i + 1, 57 - i], [56 - i, i + 1, 57 - i], LIGHT)?;
            }
        }
        Ok(())
    }
    fn middle_walls(&mut self) -> Result<(), Error> {
        if self.intersects(7, 21, 13, 50) {
            self.fill([7, 0, 21], [13, 0, 50], GRAY)?;
            self.water([7, 1, 21], [13, 10, 50])?;
            self.fill([11, 8, 21], [13, 8, 53], GRAY)?;
            for i in 0..4 {
                self.fill([i + 7, i + 5, 21], [i + 7, i + 5, 54], LIGHT)?;
            }
            for z in (21..=45).step_by(3) {
                self.put([12, 9, z], LIGHT)?;
            }
        }
        if self.intersects(44, 21, 50, 54) {
            self.fill([44, 0, 21], [50, 0, 50], GRAY)?;
            self.water([44, 1, 21], [50, 10, 50])?;
            self.fill([44, 8, 21], [46, 8, 53], GRAY)?;
            for i in 0..4 {
                self.fill([50 - i, i + 5, 21], [50 - i, i + 5, 54], LIGHT)?;
            }
            for z in (21..=45).step_by(3) {
                self.put([45, 9, z], LIGHT)?;
            }
        }
        if self.intersects(8, 44, 49, 54) {
            self.fill([14, 0, 44], [43, 0, 50], GRAY)?;
            self.water([14, 1, 44], [43, 10, 50])?;
            for x in (12..=45).step_by(3) {
                self.put([x, 9, 45], LIGHT)?;
                self.put([x, 9, 52], LIGHT)?;
                if matches!(x, 12 | 18 | 24 | 33 | 39 | 45) {
                    for p in [
                        [x, 9, 47],
                        [x, 9, 50],
                        [x, 10, 45],
                        [x, 10, 46],
                        [x, 10, 51],
                        [x, 10, 52],
                        [x, 11, 47],
                        [x, 11, 50],
                        [x, 12, 48],
                        [x, 12, 49],
                    ] {
                        self.put(p, LIGHT)?;
                    }
                }
            }
            for i in 0..3 {
                self.fill([8 + i, 5 + i, 54], [49 - i, 5 + i, 54], GRAY)?;
            }
            self.fill([11, 8, 54], [46, 8, 54], LIGHT)?;
            self.fill([14, 8, 44], [43, 8, 53], GRAY)?;
        }
        Ok(())
    }
    fn upper_walls(&mut self) -> Result<(), Error> {
        if self.intersects(14, 21, 20, 43) {
            self.fill([14, 0, 21], [20, 0, 43], GRAY)?;
            self.water([14, 1, 22], [20, 14, 43])?;
            self.fill([18, 12, 22], [20, 12, 39], GRAY)?;
            self.fill([18, 12, 21], [20, 12, 21], LIGHT)?;
            for i in 0..4 {
                self.fill([i + 14, i + 9, 21], [i + 14, i + 9, 43 - i], LIGHT)?;
            }
            for z in (23..=39).step_by(3) {
                self.put([19, 13, z], LIGHT)?;
            }
        }
        if self.intersects(37, 21, 43, 43) {
            self.fill([37, 0, 21], [43, 0, 43], GRAY)?;
            self.water([37, 1, 22], [43, 14, 43])?;
            self.fill([37, 12, 22], [39, 12, 39], GRAY)?;
            self.fill([37, 12, 21], [39, 12, 21], LIGHT)?;
            for i in 0..4 {
                self.fill([43 - i, i + 9, 21], [43 - i, i + 9, 43 - i], LIGHT)?;
            }
            for z in (23..=39).step_by(3) {
                self.put([38, 13, z], LIGHT)?;
            }
        }
        if self.intersects(15, 37, 42, 43) {
            self.fill([21, 0, 37], [36, 0, 43], GRAY)?;
            self.water([21, 1, 37], [36, 14, 43])?;
            self.fill([21, 12, 37], [36, 12, 39], GRAY)?;
            for i in 0..4 {
                self.fill([15 + i, i + 9, 43 - i], [42 - i, i + 9, 43 - i], LIGHT)?;
            }
            for x in (21..=36).step_by(3) {
                self.put([x, 13, 38], LIGHT)?;
            }
        }
        Ok(())
    }
}
impl MonumentBuilding {
    /// Port of MonumentBuilding#postProcess. The outer shell always precedes
    /// ordered child rooms, and each piece consumes the caller's decoration RNG.
    pub fn post_process(
        &self,
        context: &mut FeatureContext<'_, '_>,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        {
            let mut placer = RoomPlacer {
                data: &self.data,
                context,
                bounds,
            };
            let water_height = placer.context.region.sea_level.max(64) - self.data.bounds.min[1];
            placer.water([0, 0, 0], [58, water_height, 58])?;
            placer.outer_wing(false, 0)?;
            placer.outer_wing(true, 33)?;
            placer.entrance_arches()?;
            placer.entrance_wall()?;
            placer.roof()?;
            placer.lower_walls()?;
            placer.middle_walls()?;
            placer.upper_walls()?;
            for px in 0..7 {
                let mut pz = 0;
                while pz < 7 {
                    if pz == 0 && px == 3 {
                        pz = 6;
                    }
                    for w in 0..4 {
                        for d in 0..4 {
                            let x = px * 9 + w;
                            let z = pz * 9 + d;
                            placer.put([x, 0, z], LIGHT)?;
                            self.data.fill_column_down(
                                placer.context.region,
                                LIGHT.default_state(),
                                [x, -1, z],
                                bounds,
                            )?;
                        }
                    }
                    pz += if px != 0 && px != 6 { 6 } else { 1 };
                }
            }
            for i in 0..5 {
                for (a, b) in [
                    ([-1 - i, 2 * i, -1 - i], [-1 - i, 23, 58 + i]),
                    ([58 + i, 2 * i, -1 - i], [58 + i, 23, 58 + i]),
                    ([-i, 2 * i, -1 - i], [57 + i, 23, -1 - i]),
                    ([-i, 2 * i, 58 + i], [57 + i, 23, 58 + i]),
                ] {
                    placer.water(a, b)?;
                }
            }
        }
        for child in &self.children {
            if child.data.bounds.intersects(bounds) {
                child.post_process(context, &self.graph, bounds)?;
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
    use ferraloom_data::{Block, BlockState};
    use ferraloom_util::{
        Direction,
        random::{Algorithm, LegacyRandomSource, RandomSource, WorldgenRandom},
    };
    use std::io::Read;
    fn initial(mode: i32, [x, y, z]: [i32; 3]) -> BlockState {
        let ground = if mode == 0 {
            35
        } else {
            32 + (x * 3 + z * 5).rem_euclid(8)
        };
        if y <= ground {
            Block::STONE
        } else if mode == 1 && (x + z * 3 + y).rem_euclid(31) == 0 {
            Block::BLUE_ICE
        } else if y <= 63 {
            Block::WATER
        } else {
            Block::AIR
        }
        .default_state()
    }
    #[test]
    fn monument_building_shell_columns_child_order_clipping_and_rng_match_vanilla() {
        let (_, generator) = generator();
        let features = FeatureRegistry::default();
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../../../testdata/monument_building_blocks.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: serde_json::Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let seed = case["seed"].as_i64().unwrap();
            let direction =
                Direction::from_2d_data_value(case["direction"].as_i64().unwrap() as i32);
            let mode = case["mode"].as_i64().unwrap() as i32;
            let position: [i32; 2] =
                std::array::from_fn(|i| case["chunk"][i].as_i64().unwrap() as i32);
            let mut chunk = generator.new_chunk(position);
            for x in 0..16 {
                for z in 0..16 {
                    for y in -64..=90 {
                        chunk.set_block(
                            x as u8,
                            y,
                            z as u8,
                            initial(mode, [position[0] * 16 + x, y, position[1] * 16 + z]),
                        );
                    }
                }
            }
            let mut chunks = [RegionChunk::new(chunk, ChunkStatus::Terrain)];
            let mut region = WorldGenRegion::new(
                &mut chunks,
                position,
                &[ChunkStatus::Terrain],
                0,
                None,
                &StaticBlocks,
                0,
                63,
            )
            .unwrap();
            let building =
                MonumentBuilding::new(&mut LegacyRandomSource::new(seed), -46, 8, direction);
            let bounds = BoundingBox::new(
                [position[0] * 16, -64, position[1] * 16],
                [position[0] * 16 + 15, 319, position[1] * 16 + 15],
            );
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(17));
            building
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
            let label =
                format!("seed={seed} direction={direction:?} mode={mode} chunk={position:?}");
            assert_eq!(
                random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "{label}"
            );
            let actual = &region
                .chunk(position, ChunkStatus::Terrain)
                .unwrap()
                .poi_changes;
            let expected = case["writes"].as_array().unwrap();
            assert_eq!(actual.len(), expected.len(), "{label} writes");
            for (index, ((p, _, state), w)) in actual.iter().zip(expected).enumerate() {
                assert_eq!(
                    (*p, state.id()),
                    (
                        std::array::from_fn(|i| w[i].as_i64().unwrap() as i32),
                        w[3].as_u64().unwrap() as u16
                    ),
                    "{label} write={index}"
                );
            }
        }
    }
}
