use super::{MineshaftPiece, MineshaftPieceKind};
use crate::{
    Error,
    biome_manager::{obfuscate_seed, zoom_quart},
    feature::FeatureContext,
    region::{Heightmap, WorldGenRegion},
    structure::BoundingBox,
};
use ferraloom_data::{
    Block, BlockClass, BlockState,
    properties::{self, East, Facing, Shape, West},
};
use ferraloom_util::{Direction, random::RandomSource};

impl MineshaftPiece {
    /// Port of MineshaftPieces' four postProcess implementations. All world
    /// access is through the exclusively owned neighborhood and game bridge.
    pub fn post_process(
        &mut self,
        context: &mut FeatureContext<'_, '_>,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        if self.invalid_location(context, bounds)? {
            return Ok(());
        }
        let air = Block::CAVE_AIR.default_state();
        match self.kind.clone() {
            MineshaftPieceKind::Room { entrances } => {
                let b = self.data.bounds;
                self.box_blocks(
                    context,
                    bounds,
                    [b.min[0], b.min[1] + 1, b.min[2]],
                    [b.max[0], (b.min[1] + 3).min(b.max[1]), b.max[2]],
                    air,
                )?;
                for b in entrances {
                    self.box_blocks(
                        context,
                        bounds,
                        [b.min[0], b.max[1] - 2, b.min[2]],
                        b.max,
                        air,
                    )?;
                }
                let min = [b.min[0], b.min[1] + 4, b.min[2]];
                let max = b.max;
                let diag = std::array::from_fn::<_, 3, _>(|i| (max[i] - min[i] + 1) as f32);
                let center = [min[0] as f32 + diag[0] / 2.0, min[2] as f32 + diag[2] / 2.0];
                for y in min[1]..=max[1] {
                    let ny = (y - min[1]) as f32 / diag[1];
                    for x in min[0]..=max[0] {
                        let nx = (x as f32 - center[0]) / (diag[0] * 0.5);
                        for z in min[2]..=max[2] {
                            let nz = (z as f32 - center[1]) / (diag[2] * 0.5);
                            if nx * nx + ny * ny + nz * nz <= 1.05 {
                                self.place(context.region, air, [x, y, z], bounds)?;
                            }
                        }
                    }
                }
            }
            MineshaftPieceKind::Crossing { two_floored, .. } => {
                let b = self.data.bounds;
                let lo = b.min;
                let hi = b.max;
                for (bottom, top) in if two_floored {
                    vec![(lo[1], lo[1] + 2), (hi[1] - 2, hi[1])]
                } else {
                    vec![(lo[1], hi[1])]
                } {
                    self.box_blocks(
                        context,
                        bounds,
                        [lo[0] + 1, bottom, lo[2]],
                        [hi[0] - 1, top, hi[2]],
                        air,
                    )?;
                    self.box_blocks(
                        context,
                        bounds,
                        [lo[0], bottom, lo[2] + 1],
                        [hi[0], top, hi[2] - 1],
                        air,
                    )?;
                }
                if two_floored {
                    self.box_blocks(
                        context,
                        bounds,
                        [lo[0] + 1, lo[1] + 3, lo[2] + 1],
                        [hi[0] - 1, lo[1] + 3, hi[2] - 1],
                        air,
                    )?;
                }
                let planks = self.mine_type.planks().default_state();
                for [x, z] in [
                    [lo[0] + 1, lo[2] + 1],
                    [lo[0] + 1, hi[2] - 1],
                    [hi[0] - 1, lo[2] + 1],
                    [hi[0] - 1, hi[2] - 1],
                ] {
                    if !self
                        .block(context.region, [x, hi[1] + 1, z], bounds)?
                        .is_air()
                    {
                        self.box_blocks(context, bounds, [x, lo[1], z], [x, hi[1], z], planks)?;
                    }
                }
                for x in lo[0]..=hi[0] {
                    for z in lo[2]..=hi[2] {
                        self.plank_floor(context, [x, lo[1] - 1, z], bounds)?;
                    }
                }
            }
            MineshaftPieceKind::Stairs => {
                self.box_blocks(context, bounds, [0, 5, 0], [2, 7, 1], air)?;
                self.box_blocks(context, bounds, [0, 0, 7], [2, 2, 8], air)?;
                for i in 0..5 {
                    self.box_blocks(
                        context,
                        bounds,
                        [0, 5 - i - i32::from(i < 4), 2 + i],
                        [2, 7 - i, 2 + i],
                        air,
                    )?;
                }
            }
            MineshaftPieceKind::Corridor {
                rails,
                spiders,
                mut placed_spider,
                sections,
            } => {
                let length = sections * 5 - 1;
                self.box_blocks(context, bounds, [0, 0, 0], [2, 1, length], air)?;
                self.maybe_box(context, bounds, [0, 2, 0], [2, 2, length], 0.8, air, false)?;
                if spiders {
                    self.maybe_box(
                        context,
                        bounds,
                        [0, 0, 0],
                        [2, 1, length],
                        0.6,
                        Block::COBWEB.default_state(),
                        true,
                    )?;
                }
                for section in 0..sections {
                    let z = 2 + section * 5;
                    self.support(context, bounds, z)?;
                    for (probability, p) in [
                        (0.1, [0, 2, z - 1]),
                        (0.1, [2, 2, z - 1]),
                        (0.1, [0, 2, z + 1]),
                        (0.1, [2, 2, z + 1]),
                        (0.05, [0, 2, z - 2]),
                        (0.05, [2, 2, z - 2]),
                        (0.05, [0, 2, z + 2]),
                        (0.05, [2, 2, z + 2]),
                    ] {
                        self.maybe_web(context, bounds, p, probability)?;
                    }
                    if context.random.next_int_bound(100) == 0 {
                        self.chest_minecart(context, bounds, [2, 0, z - 1])?;
                    }
                    if context.random.next_int_bound(100) == 0 {
                        self.chest_minecart(context, bounds, [0, 0, z + 1])?;
                    }
                    if spiders && !placed_spider {
                        let p = [1, 0, z - 1 + context.random.next_int_bound(3)];
                        let world = self.data.world_position(p);
                        if bounds.contains(world) && self.interior(context.region, p, bounds)? {
                            placed_spider = true;
                            if let MineshaftPieceKind::Corridor { placed_spider, .. } =
                                &mut self.kind
                            {
                                *placed_spider = true;
                            }
                            context
                                .region
                                .set_block(world, Block::SPAWNER.default_state(), 2)?;
                            context.region.set_spawner_entity(
                                world,
                                ferraloom_data::EntityType::CAVE_SPIDER,
                                context.random,
                            )?;
                        }
                    }
                }
                for x in 0..=2 {
                    for z in 0..=length {
                        self.plank_floor(context, [x, -1, z], bounds)?;
                    }
                }
                for z in if sections > 1 {
                    vec![2, length - 2]
                } else {
                    vec![2]
                } {
                    for x in [0, 2] {
                        if self.block(context.region, [x, -1, z], bounds)?.block()
                            == self.mine_type.planks()
                        {
                            self.pillar_or_chain(context, [x, -1, z], bounds)?;
                        }
                    }
                }
                if rails {
                    let rail = Block::RAIL
                        .default_state()
                        .with(properties::SHAPE, Shape::NorthSouth)
                        .expect("rail shape");
                    for z in 0..=length {
                        let floor = self.block(context.region, [1, -1, z], bounds)?;
                        if !floor.is_air() && floor.data().solid_render {
                            let probability = if self.interior(context.region, [1, 0, z], bounds)? {
                                0.7
                            } else {
                                0.9
                            };
                            if context.random.next_float() < probability {
                                self.place(context.region, rail, [1, 0, z], bounds)?;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn invalid_location(
        &self,
        context: &FeatureContext<'_, '_>,
        bounds: BoundingBox,
    ) -> Result<bool, Error> {
        let b = self.data.bounds;
        let lo = std::array::from_fn::<_, 3, _>(|i| (b.min[i] - 1).max(bounds.min[i]));
        let hi = std::array::from_fn::<_, 3, _>(|i| (b.max[i] + 1).min(bounds.max[i]));
        let center = std::array::from_fn(|i| (lo[i] + hi[i]) / 2);
        let quart = zoom_quart(obfuscate_seed(context.region.seed), center);
        let biome = context.region.noise_biome(quart)?;
        let name = context
            .generator
            .data()
            .biome_names
            .get(biome.0 as usize)
            .ok_or_else(|| Error("unknown mineshaft biome".into()))?;
        if context
            .generator
            .data()
            .biome_tags
            .get(&"minecraft:mineshaft_blocking".parse()?)
            .is_some_and(|ids| ids.contains(name))
        {
            return Ok(true);
        }
        let liquid = |p| {
            context
                .region
                .block(p)
                .map(|s| s.block().is_instance_of(BlockClass::LiquidBlock))
        };
        for x in lo[0]..=hi[0] {
            for z in lo[2]..=hi[2] {
                if liquid([x, lo[1], z])? || liquid([x, hi[1], z])? {
                    return Ok(true);
                }
            }
        }
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                if liquid([x, y, lo[2]])? || liquid([x, y, hi[2]])? {
                    return Ok(true);
                }
            }
        }
        for z in lo[2]..=hi[2] {
            for y in lo[1]..=hi[1] {
                if liquid([lo[0], y, z])? || liquid([hi[0], y, z])? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    fn block(
        &self,
        region: &WorldGenRegion<'_>,
        p: [i32; 3],
        bounds: BoundingBox,
    ) -> Result<BlockState, Error> {
        let p = self.data.world_position(p);
        if bounds.contains(p) {
            region.block(p)
        } else {
            Ok(Block::AIR.default_state())
        }
    }
    fn place(
        &self,
        region: &mut WorldGenRegion<'_>,
        state: BlockState,
        p: [i32; 3],
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        let old = self.block(region, p, bounds)?.block();
        if old == self.mine_type.planks()
            || old == self.mine_type.wood()
            || old == self.mine_type.fence()
            || old == Block::IRON_CHAIN
        {
            return Ok(());
        }
        self.data.place_block(region, state, p, bounds)
    }
    fn box_blocks(
        &self,
        context: &mut FeatureContext<'_, '_>,
        bounds: BoundingBox,
        min: [i32; 3],
        max: [i32; 3],
        state: BlockState,
    ) -> Result<(), Error> {
        for y in min[1]..=max[1] {
            for x in min[0]..=max[0] {
                for z in min[2]..=max[2] {
                    self.place(context.region, state, [x, y, z], bounds)?;
                }
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn maybe_box(
        &self,
        context: &mut FeatureContext<'_, '_>,
        bounds: BoundingBox,
        min: [i32; 3],
        max: [i32; 3],
        probability: f32,
        state: BlockState,
        inside: bool,
    ) -> Result<(), Error> {
        for y in min[1]..=max[1] {
            for x in min[0]..=max[0] {
                for z in min[2]..=max[2] {
                    if context.random.next_float() <= probability
                        && (!inside || self.interior(context.region, [x, y, z], bounds)?)
                    {
                        self.place(context.region, state, [x, y, z], bounds)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn interior(
        &self,
        region: &WorldGenRegion<'_>,
        [x, y, z]: [i32; 3],
        bounds: BoundingBox,
    ) -> Result<bool, Error> {
        let p = self.data.world_position([x, y + 1, z]);
        Ok(bounds.contains(p) && p[1] < region.height_at(Heightmap::OceanFloorWg, p[0], p[2])?)
    }
    fn plank_floor(
        &self,
        context: &mut FeatureContext<'_, '_>,
        p: [i32; 3],
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        if self.interior(context.region, p, bounds)? {
            let world = self.data.world_position(p);
            let old = context.region.block(world)?;
            if !context
                .region
                .state_has_sturdy_face(old, world, Direction::Up)?
            {
                context
                    .region
                    .set_block(world, self.mine_type.planks().default_state(), 2)?;
            }
        }
        Ok(())
    }
    fn support(
        &self,
        context: &mut FeatureContext<'_, '_>,
        bounds: BoundingBox,
        z: i32,
    ) -> Result<(), Error> {
        for x in 0..=2 {
            if self.block(context.region, [x, 3, z], bounds)?.is_air() {
                return Ok(());
            }
        }
        let fence = self.mine_type.fence().default_state();
        self.box_blocks(
            context,
            bounds,
            [0, 0, z],
            [0, 1, z],
            fence
                .with(properties::WEST, West::True)
                .expect("fence face"),
        )?;
        self.box_blocks(
            context,
            bounds,
            [2, 0, z],
            [2, 1, z],
            fence
                .with(properties::EAST, East::True)
                .expect("fence face"),
        )?;
        let planks = self.mine_type.planks().default_state();
        if context.random.next_int_bound(4) == 0 {
            for x in [0, 2] {
                self.box_blocks(context, bounds, [x, 2, z], [x, 2, z], planks)?;
            }
        } else {
            self.box_blocks(context, bounds, [0, 2, z], [2, 2, z], planks)?;
            for (p, facing) in [
                ([1, 2, z - 1], Facing::South),
                ([1, 2, z + 1], Facing::North),
            ] {
                if context.random.next_float() < 0.05 {
                    self.place(
                        context.region,
                        Block::WALL_TORCH
                            .default_state()
                            .with(properties::FACING, facing)
                            .expect("torch facing"),
                        p,
                        bounds,
                    )?;
                }
            }
        }
        Ok(())
    }
    fn maybe_web(
        &self,
        context: &mut FeatureContext<'_, '_>,
        bounds: BoundingBox,
        p: [i32; 3],
        probability: f32,
    ) -> Result<(), Error> {
        if !self.interior(context.region, p, bounds)? || context.random.next_float() >= probability
        {
            return Ok(());
        }
        let world = self.data.world_position(p);
        let mut count = 0;
        for direction in Direction::VALUES {
            let step = direction.step();
            let neighbor = [world[0] + step.x, world[1] + step.y, world[2] + step.z];
            if bounds.contains(neighbor)
                && context.region.state_has_sturdy_face(
                    context.region.block(neighbor)?,
                    neighbor,
                    direction.opposite(),
                )?
            {
                count += 1;
                if count >= 2 {
                    self.place(context.region, Block::COBWEB.default_state(), p, bounds)?;
                    break;
                }
            }
        }
        Ok(())
    }
    fn chest_minecart(
        &self,
        context: &mut FeatureContext<'_, '_>,
        bounds: BoundingBox,
        p: [i32; 3],
    ) -> Result<(), Error> {
        let world = self.data.world_position(p);
        if bounds.contains(world)
            && context.region.block(world)?.is_air()
            && !context
                .region
                .block([world[0], world[1] - 1, world[2]])?
                .is_air()
        {
            let shape = if context.random.next_boolean() {
                Shape::NorthSouth
            } else {
                Shape::EastWest
            };
            self.place(
                context.region,
                Block::RAIL
                    .default_state()
                    .with(properties::SHAPE, shape)
                    .expect("rail shape"),
                p,
                bounds,
            )?;
            context.region.spawn_structure_chest_minecart(
                world.map(|v| f64::from(v) + 0.5),
                &"minecraft:chests/abandoned_mineshaft".parse()?,
                context.random,
            )?;
        }
        Ok(())
    }
    fn pillar_or_chain(
        &self,
        context: &mut FeatureContext<'_, '_>,
        local: [i32; 3],
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        let p = self.data.world_position(local);
        if !bounds.contains(p) {
            return Ok(());
        }
        let world_y = p[1];
        let mut below = true;
        let mut above = true;
        let mut distance = 1;
        let replaceable = |state: BlockState| {
            state.is_air()
                || state.block().is_instance_of(BlockClass::LiquidBlock)
                || matches!(
                    state.block(),
                    Block::GLOW_LICHEN | Block::SEAGRASS | Block::TALL_SEAGRASS
                )
        };
        while below || above {
            if below {
                let q = [p[0], world_y - distance, p[2]];
                let state = context.region.block(q)?;
                let empty = replaceable(state) && state.block() != Block::LAVA;
                if !empty
                    && context
                        .region
                        .state_has_sturdy_face(state, q, Direction::Up)?
                {
                    for y in (world_y - distance + 1)..world_y {
                        context.region.set_block(
                            [p[0], y, p[2]],
                            self.mine_type.wood().default_state(),
                            2,
                        )?;
                    }
                    return Ok(());
                }
                below = distance <= 20 && empty && q[1] > context.region.min_y() + 1;
            }
            if above {
                let q = [p[0], world_y + distance, p[2]];
                let state = context.region.block(q)?;
                let empty = replaceable(state);
                if !empty
                    && context.region.can_support_center(q, Direction::Down)?
                    && !state.block().is_instance_of(BlockClass::FallingBlock)
                {
                    context.region.set_block(
                        [p[0], world_y + 1, p[2]],
                        self.mine_type.fence().default_state(),
                        2,
                    )?;
                    for y in (world_y + 2)..(world_y + distance) {
                        context.region.set_block(
                            [p[0], y, p[2]],
                            Block::IRON_CHAIN.default_state(),
                            2,
                        )?;
                    }
                    return Ok(());
                }
                above = distance <= 50
                    && empty
                    && q[1] < context.region.min_y() + context.region.height() - 1;
            }
            distance += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        feature::FeatureRegistry,
        region::{ChunkStatus, RegionChunk},
        structure::processor::tests::{StaticBlocks, generator},
    };
    use ferraloom_util::random::{Algorithm, WorldgenRandom};
    use serde_json::Value;
    use std::io::Read;
    fn point(v: &Value) -> [i32; 3] {
        std::array::from_fn(|i| v[i].as_i64().unwrap() as i32)
    }
    #[test]
    fn mineshaft_saved_piece_blocks_supports_clipping_and_rng_match_vanilla() {
        let (_, generator) = generator();
        let features = FeatureRegistry::default();
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../../testdata/mineshaft_blocks.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let mode = case["mode"].as_i64().unwrap();
            let initial = ferraloom_nbt::parse_compound(case["initial"].as_str().unwrap()).unwrap();
            let mut piece =
                MineshaftPiece::from_nbt(case["id"].as_str().unwrap(), &initial).unwrap();
            let mut chunks = Vec::new();
            for cz in -1..=1 {
                for cx in -1..=1 {
                    let mut chunk = generator.new_chunk([cx, cz]);
                    generator.create_biomes(&mut chunk);
                    if [cx, cz] == [0, 0] {
                        for z in 0..16 {
                            for x in 0..16 {
                                for y in -64..=100 {
                                    let block = if mode == 1 && [x, y, z] == [1, 49, 1] {
                                        Block::WATER
                                    } else if mode == 2
                                        && (49..=58).contains(&y)
                                        && (x * 3 + z * 7) % 13 == 0
                                    {
                                        match (x + z) % 4 {
                                            0 => Block::OAK_PLANKS,
                                            1 => Block::OAK_LOG,
                                            2 => Block::OAK_FENCE,
                                            _ => Block::IRON_CHAIN,
                                        }
                                    } else {
                                        Block::STONE
                                    };
                                    chunk.set_block(x as u8, y, z as u8, block.default_state());
                                }
                            }
                        }
                    }
                    chunks.push(RegionChunk::new(chunk, ChunkStatus::Terrain));
                }
            }
            let mut region = WorldGenRegion::new(
                &mut chunks,
                [0, 0],
                &[ChunkStatus::Terrain; 2],
                0,
                None,
                &StaticBlocks,
                0,
                63,
            )
            .unwrap();
            let b = &case["bounds"];
            let bounds = BoundingBox::new(
                point(b),
                std::array::from_fn(|i| b[i + 3].as_i64().unwrap() as i32),
            );
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(17));
            let result = piece.post_process(
                &mut FeatureContext {
                    region: &mut region,
                    random: &mut random,
                    origin: [0; 3],
                    generator: &generator,
                    registry: &features,
                },
                bounds,
            );
            assert!(
                result.is_ok(),
                "{initial:?} mode={mode} {bounds:?}: {result:?}"
            );
            assert_eq!(
                piece.to_nbt(),
                ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap()
            );
            assert_eq!(
                random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "{initial:?} mode={mode} {bounds:?}"
            );
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
                actual.len(),
                expected.len(),
                "{initial:?} mode={mode} {bounds:?}"
            );
            for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(a, e, "{initial:?} mode={mode} {bounds:?} write={i}");
            }
            let mut post = chunk.chunk.post_processing.clone();
            post.sort_by_key(|p| p[1] >> 4);
            let expected = case["post"]
                .as_array()
                .unwrap()
                .iter()
                .map(point)
                .collect::<Vec<_>>();
            assert_eq!(post, expected, "{initial:?} mode={mode} {bounds:?}");
        }
    }
}
