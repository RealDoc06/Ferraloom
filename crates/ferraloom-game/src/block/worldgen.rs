//! WorldGenRegion delegates shared virtual methods to the live block registry.
use super::{BlockMethod, LevelAccessRef, LevelReaderRef};
use crate::level::LevelError;
use ferraloom_data::{BlockState, FluidState};
use ferraloom_util::random::RandomSource;
use ferraloom_util::{BlockPos, Direction, shapes};
use ferraloom_worldgen::{
    Error,
    region::{WorldGenRegion, WorldgenBlockBehavior},
};

/// Stateless game adapter for the worldgen-owned region. Families not registered
/// by the game keep the trait's explicit Unsupported result.
pub struct GameWorldgenBlockBehavior;

fn pos(p: [i32; 3]) -> BlockPos {
    BlockPos::new(p[0], p[1], p[2])
}
#[track_caller]
fn result<T>(
    value: Result<T, LevelError>,
    context: impl FnOnce() -> (Option<BlockState>, BlockPos),
) -> Result<T, Error> {
    match value {
        Ok(value) => Ok(value),
        Err(error) => {
            let (state, position) = context();
            let block = state.map(|state| (state.block().name(), state.id()));
            let caller = std::panic::Location::caller();
            Err(Error(format!(
                "game block behavior: {error:?}; block={block:?}; position={position:?}; call_site={}:{}",
                caller.file(),
                caller.line()
            )))
        }
    }
}
impl WorldgenBlockBehavior for GameWorldgenBlockBehavior {
    fn materialize_block_entity(
        &self,
        p: [i32; 3],
        state: BlockState,
        tag: &ferraloom_nbt::CompoundTag,
        registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) -> Result<Option<Box<dyn ferraloom_worldgen::region::WorldgenBlockEntity>>, Error> {
        let entity = if tag
            .get_string("id")
            .is_some_and(|id| id.to_string_lossy() == "DUMMY")
        {
            if !state.has_block_entity() {
                return Ok(None);
            }
            result(super::entity::new_block_entity(pos(p), state), || {
                (Some(state), pos(p))
            })?
        } else {
            result(
                super::entity::load_static(pos(p), state, tag, registries),
                || (Some(state), pos(p)),
            )?
        };
        Ok(entity.map(|entity| {
            Box::new(entity) as Box<dyn ferraloom_worldgen::region::WorldgenBlockEntity>
        }))
    }
    /// Port of RandomizableContainer#setBlockEntityLootTable. Materialization
    /// and the actual class check finish before the one conditional RNG draw.
    fn set_loot_table(
        &self,
        region: &mut WorldGenRegion<'_>,
        p: [i32; 3],
        table: &ferraloom_worldgen::Identifier,
        random: &mut ferraloom_util::random::WorldgenRandom,
    ) -> Result<(), Error> {
        if let Some(entity) = region.get_or_create_block_entity(p)?
            && entity.is_randomizable()
        {
            entity.set_loot_table(table, random.next_long())?;
        }
        Ok(())
    }
    fn set_loot_table_seed(
        &self,
        region: &mut WorldGenRegion<'_>,
        p: [i32; 3],
        table: &ferraloom_worldgen::Identifier,
        seed: i64,
    ) -> Result<(), Error> {
        if let Some(entity) = region.get_or_create_block_entity(p)? {
            entity.set_loot_table(table, seed)?;
        }
        Ok(())
    }
    fn set_spawner_entity(
        &self,
        region: &mut WorldGenRegion<'_>,
        p: [i32; 3],
        kind: ferraloom_data::EntityType,
        random: &mut ferraloom_util::random::WorldgenRandom,
    ) -> Result<(), Error> {
        if let Some(entity) = region.get_or_create_block_entity(p)? {
            entity.set_entity_id(kind, random)?;
        }
        Ok(())
    }
    fn populate_worldgen_beehive(
        &self,
        region: &mut WorldGenRegion<'_>,
        p: [i32; 3],
        random: &mut ferraloom_util::random::WorldgenRandom,
    ) -> Result<(), Error> {
        if let Some(entity) = region.get_or_create_block_entity(p)? {
            entity.populate_beehive(random)?;
        }
        Ok(())
    }
    /// Port of StructureTemplate#placeInWorld's loadWithComponents stage. The
    /// template seed draw depends on the existing receiver, not its incoming NBT.
    fn load_block_entity(
        &self,
        region: &mut WorldGenRegion<'_>,
        p: [i32; 3],
        mut tag: ferraloom_nbt::CompoundTag,
        random: &mut ferraloom_util::random::WorldgenRandom,
    ) -> Result<(), Error> {
        let registries = region.registry_context();
        if let Some(entity) = region.get_or_create_block_entity(p)? {
            let registries = registries?;
            if entity.is_randomizable() {
                tag.insert(
                    "LootTableSeed",
                    ferraloom_nbt::Tag::Long(random.next_long()),
                );
            }
            entity.load_with_components(&tag, registries)?;
        }
        Ok(())
    }
    fn vine_acceptable_neighbor(
        &self,
        region: &WorldGenRegion<'_>,
        p: [i32; 3],
        direction: Direction,
    ) -> Result<bool, Error> {
        result(
            super::plants::vine_acceptable_neighbor(
                &LevelReaderRef::Worldgen(region),
                pos(p),
                direction,
            ),
            || (region.block(p).ok(), pos(p)),
        )
    }
    fn place_double_plant(
        &self,
        region: &mut WorldGenRegion<'_>,
        state: BlockState,
        p: [i32; 3],
        flags: u32,
    ) -> Result<(), Error> {
        result(
            super::plants::place_double_plant(
                &mut LevelAccessRef::Worldgen(region),
                state,
                pos(p),
                super::UpdateFlags(flags),
            ),
            || (Some(state), pos(p)),
        )
    }
    fn place_mossy_carpet(
        &self,
        region: &mut WorldGenRegion<'_>,
        p: [i32; 3],
        flags: u32,
    ) -> Result<(), Error> {
        result(
            super::plants::place_mossy_carpet(
                &mut LevelAccessRef::Worldgen(region),
                pos(p),
                super::UpdateFlags(flags),
            ),
            || (region.block(p).ok(), pos(p)),
        )
    }
    fn multiface_state_for_placement(
        &self,
        region: &WorldGenRegion<'_>,
        block: ferraloom_data::Block,
        old: BlockState,
        p: [i32; 3],
        face: Direction,
    ) -> Result<Option<BlockState>, Error> {
        result(
            super::plants::multiface::state_for_placement(
                &LevelReaderRef::Worldgen(region),
                block,
                old,
                pos(p),
                face,
            ),
            || (Some(block.default_state()), pos(p)),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn multiface_spread(
        &self,
        region: &mut WorldGenRegion<'_>,
        state: BlockState,
        p: [i32; 3],
        face: Direction,
        random: &mut ferraloom_util::random::WorldgenRandom,
        post: bool,
    ) -> Result<bool, Error> {
        result(
            super::plants::multiface_spreader::spread_from_face_toward_random_direction(
                &mut LevelAccessRef::Worldgen(region),
                state,
                pos(p),
                face,
                random,
                post,
            ),
            || (Some(state), pos(p)),
        )
        .map(|value| value.is_some())
    }
    #[allow(clippy::too_many_arguments)]
    fn sculk_spread(
        &self,
        region: &mut WorldGenRegion<'_>,
        origin: [i32; 3],
        random: &mut ferraloom_util::random::WorldgenRandom,
        charge_count: i32,
        amount_per_charge: i32,
        spread_attempts: i32,
        growth_rounds: i32,
        spread_rounds: i32,
    ) -> Result<(), Error> {
        use ferraloom_worldgen::feature::{FeatureRandom, FeatureRandomSelector};
        result(
            super::plants::sculk::spread_patch(
                &mut LevelAccessRef::Worldgen(region),
                pos(origin),
                &mut FeatureRandomSelector::External(FeatureRandom::Region(random)),
                charge_count,
                amount_per_charge,
                spread_attempts,
                growth_rounds,
                spread_rounds,
            ),
            || (region.block(origin).ok(), pos(origin)),
        )
    }
    fn post_process_pos(
        &self,
        state: BlockState,
        _region: &WorldGenRegion<'_>,
        p: [i32; 3],
    ) -> Result<Option<[i32; 3]>, Error> {
        Ok(state
            .post_process_y_offset()
            .map(|offset| [p[0], p[1].wrapping_add(offset), p[2]]))
    }
    fn is_solid(&self, state: BlockState) -> Result<bool, Error> {
        Ok(state.is_solid())
    }
    fn can_survive(
        &self,
        state: BlockState,
        region: &WorldGenRegion<'_>,
        p: [i32; 3],
    ) -> Result<bool, Error> {
        result(
            LevelReaderRef::Worldgen(region).can_survive(state, pos(p)),
            || (Some(state), pos(p)),
        )
    }
    fn is_outline_full_block(
        &self,
        region: &WorldGenRegion<'_>,
        state: BlockState,
        p: [i32; 3],
    ) -> Result<bool, Error> {
        result(
            LevelReaderRef::Worldgen(region).shape(state, pos(p)),
            || (Some(state), pos(p)),
        )
        .map(|shape| shapes::is_shape_full_block(&shape))
    }
    fn is_collision_full_block(
        &self,
        region: &WorldGenRegion<'_>,
        state: BlockState,
        p: [i32; 3],
    ) -> Result<bool, Error> {
        result(
            LevelReaderRef::Worldgen(region).is_collision_shape_full_block(state, pos(p)),
            || (Some(state), pos(p)),
        )
    }
    fn is_collision_empty(&self, region: &WorldGenRegion<'_>, p: [i32; 3]) -> Result<bool, Error> {
        let reader = LevelReaderRef::Worldgen(region);
        let p = pos(p);
        let state = result(reader.get_block_state(p), || (None, p))?;
        result(reader.collision_shape_without_context(state, p), || {
            (Some(state), p)
        })
        .map(|shape| shape.is_empty())
    }
    fn is_collision_face_full(
        &self,
        region: &WorldGenRegion<'_>,
        state: BlockState,
        p: [i32; 3],
        face: Direction,
    ) -> Result<bool, Error> {
        result(
            LevelReaderRef::Worldgen(region).collision_shape_without_context(state, pos(p)),
            || (Some(state), pos(p)),
        )
        .map(|shape| shapes::is_face_full(&shape, face))
    }
    fn has_sturdy_face(
        &self,
        state: BlockState,
        region: &WorldGenRegion<'_>,
        p: [i32; 3],
        face: Direction,
    ) -> Result<bool, Error> {
        result(
            LevelReaderRef::Worldgen(region).is_face_sturdy(state, pos(p), face),
            || (Some(state), pos(p)),
        )
    }
    fn state_has_sturdy_face(
        &self,
        region: &WorldGenRegion<'_>,
        state: BlockState,
        p: [i32; 3],
        face: Direction,
    ) -> Result<bool, Error> {
        self.has_sturdy_face(state, region, p, face)
    }
    fn update_shape(
        &self,
        region: &mut WorldGenRegion<'_>,
        p: [i32; 3],
        direction: Direction,
        neighbor_p: [i32; 3],
        neighbor: BlockState,
    ) -> Result<BlockState, Error> {
        let mut access = LevelAccessRef::Worldgen(region);
        let state = result(access.get_block_state(pos(p)), || (None, pos(p)))?;
        let behavior = result(
            access
                .as_reader()
                .required_behavior(state.block(), BlockMethod::UpdateShape),
            || (Some(state), pos(p)),
        )?;
        result(
            behavior.update_shape(
                state,
                pos(p),
                direction,
                pos(neighbor_p),
                neighbor,
                &mut access,
            ),
            || (Some(state), pos(p)),
        )
    }
    fn update_from_neighbor_shapes(
        &self,
        region: &mut WorldGenRegion<'_>,
        p: [i32; 3],
        state: BlockState,
    ) -> Result<BlockState, Error> {
        result(
            LevelAccessRef::Worldgen(region).update_from_neighbor_shapes(state, pos(p)),
            || (Some(state), pos(p)),
        )
    }
    fn place_liquid(
        &self,
        region: &mut WorldGenRegion<'_>,
        p: [i32; 3],
        fluid: FluidState,
    ) -> Result<bool, Error> {
        let mut access = LevelAccessRef::Worldgen(region);
        let state = result(access.get_block_state(pos(p)), || (None, pos(p)))?;
        let behavior = result(
            access
                .as_reader()
                .required_behavior(state.block(), BlockMethod::PlaceLiquid),
            || (Some(state), pos(p)),
        )?;
        result(
            behavior.place_liquid(state, pos(p), fluid, &mut access),
            || (Some(state), pos(p)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_data::{
        Block,
        datapack::{Pack, ResourceManager},
        properties,
    };
    use ferraloom_worldgen::{
        generator::{
            GenerationData,
            flat::{FlatLayerInfo, FlatLevelGeneratorSettings, FlatLevelSource},
        },
        region::{ChunkStatus, RegionChunk},
        settings::DimensionType,
    };
    use std::{path::Path, sync::Arc};

    #[test]
    fn generation_uses_the_same_survival_shape_waterlogging_and_tick_dispatch() {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR")
            .expect("builder supplies the vanilla datapack");
        let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
        let data = Arc::new(GenerationData::load(&resources).unwrap());
        let generator = FlatLevelSource::new(
            data.clone(),
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
        let mut chunks = [RegionChunk::new(
            generator.build_terrain([0, 0], dimension.clone()),
            ChunkStatus::Features,
        )];
        let registry_context = crate::level::LevelContext::fixture(0, 63);
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
        region.set_registry_context(registry_context.registry_context().unwrap());
        let p = [8, dimension.min_y + 1, 8];
        let environment = Arc::new(
            ferraloom_worldgen::environment::EnvironmentDefinitions::load(&resources).unwrap(),
        );
        let dimension_environment = environment
            .dimension(
                &resources
                    .require(&"dimension_type/overworld.json".parse().unwrap())
                    .unwrap()
                    .json()
                    .unwrap(),
            )
            .unwrap();
        region
            .set_environment_attributes(environment, &dimension_environment, &data.biome_names)
            .unwrap();
        region.set_has_skylight(dimension.has_skylight);
        assert_eq!(LevelReaderRef::Worldgen(&region).sky_darken(), Ok(0));
        assert_eq!(
            LevelReaderRef::Worldgen(&region).get_dimension_value(
                ferraloom_worldgen::environment::EnvironmentAttributes::SKY_LIGHT_LEVEL
            ),
            Ok(15.0)
        );
        assert_eq!(
            LevelReaderRef::Worldgen(&region).max_local_raw_brightness(pos(p)),
            Ok(15)
        );
        let flower = Block::DANDELION.default_state();
        assert_eq!(
            LevelReaderRef::Worldgen(&region).can_survive(flower, pos(p)),
            Err(LevelError::MissingBlockTags)
        );
        region.set_block_tags(data.block_tags.clone());
        region.set_fluid_tags(data.fluid_tags.clone());
        assert_eq!(
            LevelAccessRef::Worldgen(&mut region).fast_lava(),
            Err(LevelError::UnsupportedWorldgenEnvironment)
        );
        region.set_fast_lava(dimension.fast_lava);
        assert!(!LevelAccessRef::Worldgen(&mut region).fast_lava().unwrap());
        assert!(region.can_survive(flower, p).unwrap());
        region
            .set_block([p[0], p[1] - 1, p[2]], Block::STONE.default_state(), 16)
            .unwrap();
        region.set_block(p, flower, 16).unwrap();
        assert!(
            region
                .update_shape(
                    p,
                    Direction::Down,
                    [p[0], p[1] - 1, p[2]],
                    Block::STONE.default_state()
                )
                .unwrap()
                .is_air()
        );
        assert!(
            GameWorldgenBlockBehavior
                .is_collision_full_block(&region, Block::STONE.default_state(), p)
                .unwrap()
        );
        assert!(
            GameWorldgenBlockBehavior
                .is_collision_empty(&region, [0, p[1], 0])
                .unwrap()
        );
        let sand = [9, p[1], 8];
        region
            .set_block(sand, Block::SAND.default_state(), 16)
            .unwrap();
        region
            .update_shape(
                sand,
                Direction::Down,
                [9, p[1] - 1, 8],
                Block::DIRT.default_state(),
            )
            .unwrap();
        let chain = [10, p[1], 8];
        region
            .set_block(chain, Block::IRON_CHAIN.default_state(), 16)
            .unwrap();
        assert!(
            region
                .place_liquid(chain, super::super::fluid::FlowingFluid::Water.source())
                .unwrap()
        );
        assert_eq!(
            region.block(chain).unwrap().get(properties::WATERLOGGED),
            Some(true)
        );
        let chunk = region.chunk([0, 0], ChunkStatus::Empty).unwrap();
        assert!(chunk.block_ticks.has_scheduled_tick(sand, &Block::SAND));
        assert!(
            chunk
                .fluid_ticks
                .has_scheduled_tick(chain, &ferraloom_data::registries::fluid::Id::WATER)
        );
        assert_eq!(
            LevelReaderRef::Worldgen(&region).get_block_state(BlockPos::new(32, p[1], 8)),
            Err(LevelError::MissingChunk(ferraloom_util::ChunkPos::new(
                2, 0
            )))
        );
        struct PendingWorldgenQuery;
        impl WorldgenBlockBehavior for PendingWorldgenQuery {
            fn post_process_pos(
                &self,
                _: BlockState,
                _: &WorldGenRegion<'_>,
                _: [i32; 3],
            ) -> Result<Option<[i32; 3]>, Error> {
                Err(Error("test: required postProcessPos".into()))
            }
            fn update_shape(
                &self,
                _: &mut WorldGenRegion<'_>,
                _: [i32; 3],
                _: Direction,
                _: [i32; 3],
                _: BlockState,
            ) -> Result<BlockState, Error> {
                Err(Error("test: required updateShape".into()))
            }
        }
        // The preceding checks use the actual game bridge. Required-service
        // failures use an explicit receiver rather than an unported family.
        let registry_context = crate::level::LevelContext::fixture(0, 63);
        let mut region = WorldGenRegion::new(
            &mut chunks,
            [0, 0],
            &[ChunkStatus::Features],
            0,
            None,
            &PendingWorldgenQuery,
            0,
            63,
        )
        .unwrap();
        region.set_registry_context(registry_context.registry_context().unwrap());
        assert_eq!(
            region
                .update_shape(
                    p,
                    Direction::Down,
                    [p[0], p[1] - 1, p[2]],
                    Block::DIRT.default_state()
                )
                .unwrap_err()
                .0,
            "test: required updateShape"
        );
    }
}
