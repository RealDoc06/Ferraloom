//! Port of GrassBlock's bounded random walk and biome bone-meal features.
use super::super::{LevelAccessRef, LevelReaderRef};
use super::{BonemealSource, bonemeal::inside_height, bonemealable_behavior};
use crate::level::{Level, LevelError};
use ferraloom_data::Block;
use ferraloom_util::{BlockPos, random::RandomSource};
use ferraloom_worldgen::feature::{FeatureContext, FeatureRandomSelector, WorldGenLevel};

pub(super) fn perform(
    pos: BlockPos,
    access: &mut LevelAccessRef<'_, '_>,
    source: BonemealSource,
) -> Result<(), LevelError> {
    let LevelAccessRef::Server(level) = access else {
        return Err(LevelError::UnsupportedWorldgenBehavior);
    };
    'attempt: for attempt in 0..128 {
        let mut test = pos.above(1);
        for _ in 0..attempt / 16 {
            let dx = level.random().next_int_bound(3) - 1;
            let dy = (level.random().next_int_bound(3) - 1) * level.random().next_int_bound(3) / 2;
            let dz = level.random().next_int_bound(3) - 1;
            test = test.offset(dx, dy, dz);
            let reader = LevelReaderRef::Server(level);
            if reader.get_block_state(test.below(1))?.block() != Block::GRASS_BLOCK
                || reader.is_collision_shape_full_block(reader.get_block_state(test)?, test)?
            {
                continue 'attempt;
            }
        }
        place_effect(test, level, source)?;
    }
    Ok(())
}

fn place_effect(
    pos: BlockPos,
    level: &mut Level,
    source: BonemealSource,
) -> Result<(), LevelError> {
    let services = level.context().clone();
    let grass_id = "grass_bonemeal"
        .parse()
        .expect("builtin placed feature identifier");
    let grass_feature = services.features.placed.get(&grass_id).cloned();
    // Keep this receiver even if growing short grass replaces it during the
    // first branch. Vanilla's second branch tests the original state.
    let state = level.get_block_state(pos)?;
    if state.block() == Block::SHORT_GRASS && level.random().next_float() < 0.1_f32 {
        let behavior = bonemealable_behavior(Block::SHORT_GRASS).expect("short grass bonemeal");
        let mut access = LevelAccessRef::Server(level);
        if behavior.is_valid_bonemeal_target(state, pos, &mut access, source)? {
            behavior.perform_bonemeal(state, pos, &mut access, source)?;
        }
    }
    if state.is_air() && inside_height(&LevelReaderRef::Server(level), pos) {
        let flower = if level.random().next_float() < 0.125_f32 {
            let biome = level.biome_at(pos)?;
            let features = services
                .features
                .bone_meal_features(biome)
                .map_err(|error| {
                    eprintln!("grass bonemeal at {pos:?}: {error}");
                    LevelError::WorldgenMutationFailed { pos, source: error }
                })?;
            if features.is_empty() {
                return Ok(());
            }
            Some(features[level.random().next_int_bound(features.len() as i32) as usize].clone())
        } else {
            None
        };
        let mut context = FeatureContext {
            level: WorldGenLevel::Live(level),
            random: FeatureRandomSelector::Canonical,
            origin: [pos.x, pos.y, pos.z],
            generator: services.generator.access(),
            registry: &services.features,
        };
        let result = if let Some(feature) = flower {
            feature.place(&mut context)
        } else if let Some(feature) = grass_feature {
            feature.place(&mut context, false)
        } else {
            return Ok(());
        };
        result.map_err(|error| {
            eprintln!("grass bonemeal feature at {pos:?}: {error}");
            LevelError::WorldgenMutationFailed { pos, source: error }
        })?;
    }
    Ok(())
}
