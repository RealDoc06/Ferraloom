//! Live configured features use the same concrete mutations and virtual methods
//! as ordinary gameplay. Region writes remain in the worldgen crate.
use super::{Level, LevelError};
use crate::{
    block::{
        BlockCollisionContext, BlockMethod, LevelAccessRef, LevelReaderRef, UPDATE_LIMIT,
        UpdateFlags, plants,
    },
    ticks::{ScheduledTick, TickPriority},
};
use ferraloom_data::{Block, BlockState, FluidState};
use ferraloom_util::{BlockPos, ChunkPos, Direction, random::RandomSource, shapes};
use ferraloom_worldgen::{
    Error, Identifier,
    feature::{FeatureRandomSelector, LiveWorldGenLevel},
    region::{Heightmap, LightLayer, SavedTick},
};

fn pos(p: [i32; 3]) -> BlockPos {
    BlockPos::new(p[0], p[1], p[2])
}
fn required<T>(value: Result<T, LevelError>) -> Result<T, Error> {
    value.map_err(|error| Error(format!("live feature: {error:?}")))
}

impl Level {
    /// Port of Feature#place invoked by TreeGrower and feature bonemeal. Only
    /// immutable services are cloned; canonical random borrows end at each draw.
    pub fn place_configured_feature(
        &mut self,
        feature: &Identifier,
        origin: BlockPos,
        mut random: FeatureRandomSelector<'_>,
    ) -> Result<bool, Error> {
        let context = self.context().clone();
        ferraloom_worldgen::feature::place_configured_feature(
            self,
            context.generator.access(),
            &context.features,
            feature,
            [origin.x, origin.y, origin.z],
            random.reborrow(),
        )
    }
}

impl LiveWorldGenLevel for Level {
    fn structure_templates(
        &self,
    ) -> Result<&ferraloom_worldgen::structure::template::StructureTemplateManager, Error> {
        required(self.context().structure_templates()).map(std::sync::Arc::as_ref)
    }
    fn min_y(&self) -> i32 {
        self.min_build_height()
    }
    fn height(&self) -> i32 {
        self.max_build_height() + 1 - self.min_build_height()
    }
    fn seed(&self) -> i64 {
        Level::seed(self)
    }
    fn sea_level(&self) -> i32 {
        Level::sea_level(self)
    }
    fn random_mut(&mut self) -> Result<&mut ferraloom_util::random::Random, Error> {
        Ok(self.random())
    }
    fn block(&self, p: [i32; 3]) -> Result<BlockState, Error> {
        required(self.get_block_state(pos(p)))
    }
    fn noise_biome(&self, quart: [i32; 3]) -> Result<ferraloom_chunk::BiomeId, Error> {
        required(self.noise_biome_at_quart(quart))
    }
    fn height_at(&self, map: Heightmap, x: i32, z: i32) -> Result<i32, Error> {
        if !(-30_000_000..30_000_000).contains(&x) || !(-30_000_000..30_000_000).contains(&z) {
            return Ok(self.sea_level().wrapping_add(1));
        }
        let map = match map {
            Heightmap::WorldSurface => super::heightmap::Heightmap::WorldSurface,
            Heightmap::OceanFloor => super::heightmap::Heightmap::OceanFloor,
            Heightmap::MotionBlocking => super::heightmap::Heightmap::MotionBlocking,
            Heightmap::MotionBlockingNoLeaves => {
                super::heightmap::Heightmap::MotionBlockingNoLeaves
            }
            Heightmap::WorldSurfaceWg | Heightmap::OceanFloorWg => {
                let key = ChunkPos::new(x >> 4, z >> 4);
                if !self.has_chunk(key) {
                    return Ok(self.min_build_height());
                }
                let chunk = required(self.get_full_chunk(key))?;
                return Ok(chunk.generation_height_at(
                    map == Heightmap::OceanFloorWg,
                    (x & 15) as u8,
                    (z & 15) as u8,
                    required(self.block_tags())?,
                ));
            }
        };
        required(self.heightmap_pos(map, BlockPos::new(x, 0, z))).map(|p| p.y)
    }
    fn set_block(&mut self, p: [i32; 3], state: BlockState, flags: u32) -> Result<bool, Error> {
        required(Level::set_block(
            self,
            pos(p),
            state,
            UpdateFlags(flags),
            UPDATE_LIMIT,
        ))
    }
    fn set_block_in_section(&mut self, p: [i32; 3], state: BlockState) -> Result<(), Error> {
        // OreFeature's BulkSectionAccess writes directly to LevelChunkSection,
        // without Level#setBlock callbacks, heightmaps or change broadcasting.
        required(self.set_block_state(pos(p), state)).map(|_| ())
    }
    fn mark_post_processing(&mut self, p: [i32; 3]) -> Result<(), Error> {
        // Live LevelChunk inherits ChunkAccess's warning-only implementation;
        // adding protochunk callbacks here would schedule work vanilla never does.
        let key = ChunkPos::containing(pos(p));
        required(self.get_full_chunk(key))?;
        eprintln!(
            "Trying to mark a block for post processing @ {p:?}, but this operation is not supported."
        );
        Ok(())
    }
    fn schedule_block_tick(&mut self, tick: SavedTick<Block>, delay: i32) -> Result<(), Error> {
        let trigger_tick = self.game_time().wrapping_add(i64::from(delay));
        required(Level::schedule_block_tick(
            self,
            pos(tick.position),
            tick.kind,
            trigger_tick,
            TickPriority::by_value(tick.priority),
        ))
    }
    fn schedule_fluid_tick(
        &mut self,
        tick: SavedTick<ferraloom_data::registries::fluid::Id>,
        delay: i32,
    ) -> Result<(), Error> {
        let p = pos(tick.position);
        let key = ChunkPos::containing(p);
        required(self.chunk(key).ok_or(LevelError::MissingChunk(key)))?;
        let region = self.region_mut();
        let order = region.next_sub_tick_order;
        region.next_sub_tick_order = order.wrapping_add(1);
        region.fluid_ticks.schedule_tick(ScheduledTick {
            pos: p,
            block: tick.kind,
            trigger_tick: region.game_time.wrapping_add(i64::from(delay)),
            priority: TickPriority::by_value(tick.priority),
            sub_tick_order: order,
        });
        Ok(())
    }
    fn destroy_block(&mut self, p: [i32; 3], drop: bool) -> Result<bool, Error> {
        required(Level::destroy_block(self, pos(p), drop, UPDATE_LIMIT))
    }
    fn place_liquid(&mut self, p: [i32; 3], fluid: FluidState) -> Result<bool, Error> {
        let p = pos(p);
        let state = required(self.get_block_state(p))?;
        let behavior =
            required(self.required_block_behavior(state.block(), BlockMethod::PlaceLiquid))?;
        required(behavior.place_liquid(state, p, fluid, &mut LevelAccessRef::Server(self)))
    }
    fn update_shape(
        &mut self,
        p: [i32; 3],
        direction: Direction,
        neighbor_p: [i32; 3],
        neighbor_state: BlockState,
    ) -> Result<BlockState, Error> {
        let p = pos(p);
        let state = required(self.get_block_state(p))?;
        let behavior =
            required(self.required_block_behavior(state.block(), BlockMethod::UpdateShape))?;
        required(behavior.update_shape(
            state,
            p,
            direction,
            pos(neighbor_p),
            neighbor_state,
            &mut LevelAccessRef::Server(self),
        ))
    }
    fn update_from_neighbor_shapes(
        &mut self,
        p: [i32; 3],
        state: BlockState,
    ) -> Result<BlockState, Error> {
        required(LevelAccessRef::Server(self).update_from_neighbor_shapes(state, pos(p)))
    }
    fn update_neighbors_at(&mut self, p: [i32; 3], block: Block) -> Result<(), Error> {
        required(Level::update_neighbors_at(self, pos(p), block))
    }
    fn is_outline_full_block(&self, state: BlockState, p: [i32; 3]) -> Result<bool, Error> {
        required(LevelReaderRef::Server(self).shape(state, pos(p)))
            .map(|shape| shapes::is_shape_full_block(&shape))
    }
    fn is_collision_full_block(&self, state: BlockState, p: [i32; 3]) -> Result<bool, Error> {
        required(LevelReaderRef::Server(self).is_collision_shape_full_block(state, pos(p)))
    }
    fn has_sturdy_face(&self, p: [i32; 3], face: Direction) -> Result<bool, Error> {
        self.state_has_sturdy_face(self.block(p)?, p, face)
    }
    fn state_has_sturdy_face(
        &self,
        state: BlockState,
        p: [i32; 3],
        face: Direction,
    ) -> Result<bool, Error> {
        required(LevelReaderRef::Server(self).is_face_sturdy(state, pos(p), face))
    }
    fn can_survive(&self, state: BlockState, p: [i32; 3]) -> Result<bool, Error> {
        required(Level::can_survive(self, state, pos(p)))
    }
    fn is_unobstructed(&self, state: BlockState, p: [i32; 3]) -> Result<bool, Error> {
        required(Level::is_unobstructed(
            self,
            state,
            pos(p),
            &BlockCollisionContext::empty(),
        ))
    }
    fn is_collision_empty(&self, p: [i32; 3]) -> Result<bool, Error> {
        let state = self.block(p)?;
        required(LevelReaderRef::Server(self).collision_shape_without_context(state, pos(p)))
            .map(|shape| shape.is_empty())
    }
    fn brightness(&self, layer: LightLayer, p: [i32; 3]) -> Result<u8, Error> {
        let layer = match layer {
            LightLayer::Sky => ferraloom_light::LightLayer::Sky,
            LightLayer::Block => ferraloom_light::LightLayer::Block,
        };
        required(Level::brightness(self, layer, pos(p)))
    }
    fn vine_acceptable_neighbor(&self, p: [i32; 3], direction: Direction) -> Result<bool, Error> {
        required(plants::vine_acceptable_neighbor(
            &LevelReaderRef::Server(self),
            pos(p),
            direction,
        ))
    }
    fn place_double_plant(
        &mut self,
        state: BlockState,
        p: [i32; 3],
        flags: u32,
    ) -> Result<(), Error> {
        required(plants::place_double_plant(
            &mut LevelAccessRef::Server(self),
            state,
            pos(p),
            UpdateFlags(flags),
        ))
    }
    fn place_mossy_carpet(&mut self, p: [i32; 3], flags: u32) -> Result<(), Error> {
        required(plants::place_mossy_carpet(
            &mut LevelAccessRef::Server(self),
            pos(p),
            UpdateFlags(flags),
        ))
    }
    fn multiface_state_for_placement(
        &self,
        block: Block,
        old: BlockState,
        p: [i32; 3],
        face: Direction,
    ) -> Result<Option<BlockState>, Error> {
        required(plants::multiface::state_for_placement(
            &LevelReaderRef::Server(self),
            block,
            old,
            pos(p),
            face,
        ))
    }
    fn multiface_spread(
        &mut self,
        state: BlockState,
        p: [i32; 3],
        face: Direction,
        random: &mut FeatureRandomSelector<'_>,
        post: bool,
    ) -> Result<bool, Error> {
        let mut access = LevelAccessRef::Server(self);
        let spread = match random {
            FeatureRandomSelector::External(random) => {
                plants::multiface_spreader::spread_from_face_toward_random_direction(
                    &mut access,
                    state,
                    pos(p),
                    face,
                    random,
                    post,
                )
            }
            FeatureRandomSelector::Canonical => {
                // Native shuffles the complete list before the first world call.
                // End the canonical random borrow before entering those callbacks.
                let mut directions = Direction::VALUES;
                for size in (2..=directions.len()).rev() {
                    let other = required(access.random())?.next_int_bound(size as i32) as usize;
                    directions.swap(size - 1, other);
                }
                let mut placed = None;
                for direction in directions {
                    placed = required(
                        plants::multiface_spreader::spread_from_face_toward_direction(
                            &mut access,
                            state,
                            pos(p),
                            face,
                            direction,
                            post,
                        ),
                    )?;
                    if placed.is_some() {
                        break;
                    }
                }
                Ok(placed)
            }
        };
        required(spread).map(|spread| spread.is_some())
    }
    fn set_loot_table(
        &mut self,
        p: [i32; 3],
        table: &Identifier,
        random: &mut FeatureRandomSelector<'_>,
    ) -> Result<(), Error> {
        let Some(handle) = required(self.get_or_promote_block_entity(pos(p)))? else {
            return Ok(());
        };
        if required(self.block_entity_state(handle))?
            .randomizable()
            .is_none()
        {
            return Ok(());
        }
        let seed = match random {
            FeatureRandomSelector::Canonical => self.random().next_long(),
            FeatureRandomSelector::External(random) => random.next_long(),
        };
        let table = table
            .to_string()
            .parse()
            .map_err(|error| Error(format!("loot table: {error}")))?;
        let receiver = required(self.block_entity_state_mut(handle))?
            .randomizable_mut()
            .unwrap();
        receiver.loot_table = Some(table);
        receiver.loot_table_seed = seed;
        Ok(())
    }
    fn block_tags(&self) -> Result<&ferraloom_data::datapack::TagSet<Block>, Error> {
        required(Level::block_tags(self))
    }
    fn fluid_tags(
        &self,
    ) -> Result<&ferraloom_data::datapack::TagSet<ferraloom_data::registries::fluid::Id>, Error>
    {
        required(Level::fluid_tags(self))
    }
    fn has_block_entity_at(&self, p: [i32; 3]) -> Result<bool, Error> {
        required(self.block_entity_ref(pos(p))).map(|handle| handle.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        block::{BlockBehavior, UnportedBehavior},
        level::{LevelContext, Region, chunk::LevelChunk, effects::LevelEffect},
    };
    use ferraloom_worldgen::feature::{Feature, FeatureContext, FeatureRegistry};
    use std::sync::Arc;

    struct PlacementCallback;
    impl BlockBehavior for PlacementCallback {
        fn on_place(
            &self,
            _: BlockState,
            p: BlockPos,
            _: BlockState,
            _: bool,
            level: &mut Level,
        ) -> Result<(), LevelError> {
            let data = level.random().next_int();
            level.emit(LevelEffect::LevelEvent {
                pos: p,
                id: 61,
                data,
            });
            Ok(())
        }
    }
    static BEHAVIORS: Behaviors = Behaviors::EMPTY
        .with_block(Block::STONE, &PlacementCallback)
        .with_block(Block::DIRT, &PlacementCallback);

    #[derive(Debug)]
    struct ScheduledPlacement;
    impl Feature for ScheduledPlacement {
        fn place(&self, context: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
            assert_eq!((context.level.seed(), context.level.sea_level()), (123, 63));
            let block = [Block::STONE, Block::DIRT][context.random()?.next_int_bound(2) as usize];
            let p = context.origin;
            let placed = context.level.set_block(p, block.default_state(), 18)?;
            context.level.schedule_block_tick(
                SavedTick {
                    position: p,
                    kind: block,
                    priority: -2,
                },
                7,
            )?;
            context.level.schedule_fluid_tick(
                SavedTick {
                    position: p,
                    kind: ferraloom_data::registries::fluid::Id::WATER,
                    priority: 2,
                },
                9,
            )?;
            let _ = context.random()?.next_float();
            Ok(placed)
        }
    }
    fn level() -> Level {
        let fixture = LevelContext::fixture(123, 63);
        let mut registry = FeatureRegistry::default();
        registry.features.insert(
            "test:scheduled".parse().unwrap(),
            Arc::new(ScheduledPlacement),
        );
        let data = fixture.generator.access().data();
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            Arc::new(LevelContext::new(
                fixture.seed,
                fixture.sea_level,
                fixture.generator.clone(),
                Arc::new(registry),
                fixture.structures.clone(),
                fixture.damage_types().clone(),
                fixture.initial_difficulty,
            )),
        );
        level.set_block_tags(data.block_tags.clone());
        level.set_fluid_tags(data.fluid_tags.clone());
        level
            .add_chunk(LevelChunk::new(
                ChunkPos::ZERO,
                -4..20,
                ferraloom_chunk::LevelChunkSection::new(
                    ferraloom_chunk::Strategy::block_states(),
                    ferraloom_chunk::Strategy::biomes(Arc::new(
                        ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                    )),
                    ferraloom_chunk::BiomeId(0),
                ),
            ))
            .unwrap();
        level.set_game_time(100);
        level
    }
    #[test]
    fn live_placement_keeps_callbacks_delays_priorities_and_canonical_rng() {
        for algorithm in [
            ferraloom_util::random::Algorithm::Legacy,
            ferraloom_util::random::Algorithm::Xoroshiro,
        ] {
            for canonical in [true, false] {
                let mut level = level();
                level.set_random_streams(algorithm.new_instance(72), 0);
                let mut expected_level = algorithm.new_instance(72);
                let mut external = algorithm.new_instance(99);
                let mut expected_external = algorithm.new_instance(99);
                for x in 0..8 {
                    let expected_block = if canonical {
                        expected_level.next_int_bound(2)
                    } else {
                        expected_external.next_int_bound(2)
                    };
                    let callback = expected_level.next_int();
                    if canonical {
                        expected_level.next_float();
                    } else {
                        expected_external.next_float();
                    }
                    let selector = if canonical {
                        FeatureRandomSelector::Canonical
                    } else {
                        FeatureRandomSelector::External(
                            ferraloom_worldgen::feature::FeatureRandom::Live(&mut external),
                        )
                    };
                    let p = BlockPos::new(x, 1, 8);
                    assert!(
                        level
                            .place_configured_feature(
                                &"test:scheduled".parse().unwrap(),
                                p,
                                selector
                            )
                            .unwrap()
                    );
                    assert_eq!(
                        level.get_block_state(p).unwrap().block(),
                        [Block::STONE, Block::DIRT][expected_block as usize]
                    );
                    let effects = level.take_effects();
                    assert!(effects.iter().any(|effect| matches!(effect, LevelEffect::LevelEvent {pos, id:61, data} if *pos==p && *data==callback)));
                }
                assert_eq!(level.random().next_long(), expected_level.next_long());
                assert_eq!(external.next_long(), expected_external.next_long());
                let mut saved = ferraloom_storage::SerializableChunkData::default();
                level.save_chunk_ticks(ChunkPos::ZERO, &mut saved).unwrap();
                let blocks = saved.block_ticks.unwrap();
                let fluids = saved.fluid_ticks.unwrap();
                assert_eq!((blocks.len(), fluids.len()), (8, 8));
                assert!(
                    blocks
                        .iter()
                        .all(|tick| tick.delay == Some(7) && tick.priority == Some(-2))
                );
                assert!(
                    fluids
                        .iter()
                        .all(|tick| tick.delay == Some(9) && tick.priority == Some(2))
                );
            }
        }
    }
    #[test]
    fn live_reads_keep_missing_chunks_required_and_wg_heightmaps_cached() {
        let mut level = level();
        let p = BlockPos::new(8, 4, 8);
        let expected: std::collections::BTreeMap<_, _> =
            include_str!("../../tests/java/level/live-heightmaps.tsv")
                .lines()
                .map(|line| {
                    let (name, value) = line.split_once('\t').unwrap();
                    (name, value.parse::<i32>().unwrap())
                })
                .collect();
        assert_eq!(level.min_build_height(), expected["minimum"]);
        level
            .set_block_state(p, Block::STONE.default_state())
            .unwrap();
        assert_eq!(
            LiveWorldGenLevel::height_at(&level, Heightmap::OceanFloorWg, 8, 8).unwrap(),
            expected["first-ocean-floor-wg"]
        );
        level
            .set_block(
                p,
                Block::AIR.default_state(),
                UpdateFlags::SKIP_ALL_SIDEEFFECTS,
                UPDATE_LIMIT,
            )
            .unwrap();
        assert_eq!(
            LiveWorldGenLevel::height_at(&level, Heightmap::OceanFloorWg, 8, 8).unwrap(),
            expected["cached-ocean-floor-wg"]
        );
        assert_eq!(
            LiveWorldGenLevel::height_at(&level, Heightmap::WorldSurfaceWg, 8, 8).unwrap(),
            expected["first-world-surface-wg"]
        );
        level
            .set_block(
                p.below(2),
                Block::DIRT.default_state(),
                UpdateFlags::SKIP_ALL_SIDEEFFECTS,
                UPDATE_LIMIT,
            )
            .unwrap();
        assert_eq!(
            LiveWorldGenLevel::height_at(&level, Heightmap::OceanFloor, 8, 8).unwrap(),
            expected["updated-ocean-floor"]
        );
        assert_eq!(
            LiveWorldGenLevel::height_at(&level, Heightmap::WorldSurfaceWg, 8, 8).unwrap(),
            expected["still-cached-world-surface-wg"]
        );
        for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
            level.set_unported_behavior(policy);
            assert!(
                LiveWorldGenLevel::block(&level, [16, 1, 0])
                    .unwrap_err()
                    .0
                    .contains("MissingChunk")
            );
            // Level#getHeight returns minimum Y after its non-loading presence
            // check; the block read above still requires the absent chunk.
            assert_eq!(
                LiveWorldGenLevel::height_at(&level, Heightmap::OceanFloorWg, 16, 0).unwrap(),
                level.min_build_height()
            );
            assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        }
    }
}
