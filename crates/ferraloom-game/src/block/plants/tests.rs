use super::*;
use crate::{
    Behaviors,
    block::{BlockBehavior, LevelAccessRef, LevelReaderRef},
    level::{Level, Region, chunk::LevelChunk, effects::LevelEffect},
};
use ferraloom_data::properties as p;
use ferraloom_util::{BlockPos, ChunkPos, Direction};
use std::sync::Arc;
static EMPTY: Behaviors = Behaviors::EMPTY;
fn level() -> Level {
    let mut level = Level::new(Region::new(&EMPTY));
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            0..2,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                )),
                ferraloom_chunk::BiomeId(0),
            ),
        ))
        .unwrap();
    level
}
#[test]
fn upper_double_plant_requires_its_own_lower_half() {
    let mut level = level();
    let pos = BlockPos::new(8, 5, 8);
    let upper = Block::TALL_GRASS
        .default_state()
        .with(p::HALF, p::Half::Upper)
        .unwrap();
    for (below, expected) in [
        (Block::TALL_GRASS.default_state(), true),
        (upper, false),
        (Block::LARGE_FERN.default_state(), false),
        (Block::STONE.default_state(), false),
    ] {
        level.set_block_state(pos.below(1), below).unwrap();
        assert_eq!(
            survival::PLANT
                .can_survive(upper, pos, &LevelReaderRef::Server(&level))
                .unwrap(),
            expected,
            "{below:?}"
        );
    }
}
#[test]
fn multiface_neighbor_loss_removes_only_the_unsupported_face() {
    let mut level = level();
    let pos = BlockPos::new(8, 5, 8);
    let north =
        multiface::with_face(Block::GLOW_LICHEN.default_state(), Direction::North, true).unwrap();
    let both = multiface::with_face(north, Direction::South, true).unwrap();
    for (input, expected) in [
        (
            both,
            multiface::with_face(both, Direction::North, false).unwrap(),
        ),
        (north, Block::AIR.default_state()),
    ] {
        let output = multiface::MULTIFACE
            .update_shape(
                input,
                pos,
                Direction::North,
                pos.north(1),
                Block::AIR.default_state(),
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(output, expected);
    }
}
#[test]
fn decaying_waterlogged_leaves_drop_and_restore_water_without_destroy_particles() {
    let mut level = level();
    let pos = BlockPos::new(8, 5, 8);
    let decaying = Block::OAK_LEAVES
        .default_state()
        .with(p::WATERLOGGED, true)
        .unwrap();
    // Isolate this callback: vanilla removeBlock delegates fluid replacement to setBlock.
    for state in [
        decaying.with(p::PERSISTENT, true).unwrap(),
        decaying.with(p::DISTANCE, 6).unwrap(),
    ] {
        level.set_block_state(pos, state).unwrap();
        leaves::LEAVES.random_tick(state, pos, &mut level).unwrap();
        assert_eq!(level.get_block_state(pos).unwrap(), state);
    }
    // Dry leaves exercise the same no-particle removal without starting fluid flow.
    let dry = decaying.with(p::WATERLOGGED, false).unwrap();
    level.set_block_state(pos, dry).unwrap();
    leaves::LEAVES.random_tick(dry, pos, &mut level).unwrap();
    assert_eq!(
        level.get_block_state(pos).unwrap(),
        Block::AIR.default_state()
    );
    let effects = level.take_effects();
    assert!(
        effects
            .iter()
            .any(|e| matches!(e,LevelEffect::DropResources{state,..} if *state==dry))
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, LevelEffect::DestroyParticles { .. }))
    );
}
#[test]
fn shared_raw_brightness_requires_light_context() {
    let level = level();
    assert!(
        LevelReaderRef::Server(&level)
            .get_raw_brightness(BlockPos::new(8, 5, 8), 0)
            .is_err()
    );
}
