//! Port of EyeblossomBlock's environment-driven transitions and delayed neighbors.
use super::super::{BlockBehavior, LevelAccessRef, UpdateFlags};
use crate::level::{Level, LevelError, environment::AttributeKey};
use ferraloom_data::{
    Block, BlockState,
    registries::{
        environment_attribute::Id as EnvironmentAttribute, game_event::Id as GameEvent,
        sound_event::Id as SoundEvent,
    },
};
use ferraloom_protocol::{
    codec::VarInt,
    entity_data::{Particle, Trail},
    play::SoundSource,
};
use ferraloom_util::{BlockPos, ChunkPos, Vec3, random::RandomSource};
use ferraloom_worldgen::environment::TriState;

const EYEBLOSSOM_OPEN: AttributeKey<TriState> =
    AttributeKey::new(EnvironmentAttribute::GAMEPLAY_EYEBLOSSOM_OPEN);

pub struct Eyeblossom;
pub static EYEBLOSSOM: Eyeblossom = Eyeblossom;

/// Port of the matching traversal in BlockScanUtils#sectionBasedScan. Sections
/// advance X, Z, Y; cells within each section advance Y, Z, X. Neighbor ticks
/// consume RNG in this order, including when the flower range crosses sections.
pub(super) fn for_each_matching(
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
    mut consumer: impl FnMut(&mut Level, BlockPos) -> Result<(), LevelError>,
) -> Result<(), LevelError> {
    let from = pos.offset(-3, -2, -3);
    let to = pos.offset(3, 2, 3);
    for cx in (from.x >> 4)..=(to.x >> 4) {
        for cz in (from.z >> 4)..=(to.z >> 4) {
            let chunk_pos = ChunkPos::new(cx, cz);
            let chunk = level
                .chunk(chunk_pos)
                .ok_or(LevelError::MissingChunk(chunk_pos))?;
            let min_section = (from.y >> 4).max(chunk.min_y() >> 4);
            let max_section = (to.y >> 4).min((chunk.max_y() - 1) >> 4);
            for cy in min_section..=max_section {
                for y in from.y.max(cy << 4)..=to.y.min((cy << 4) + 15) {
                    for z in from.z.max(cz << 4)..=to.z.min((cz << 4) + 15) {
                        for x in from.x.max(cx << 4)..=to.x.min((cx << 4) + 15) {
                            let at = BlockPos::new(x, y, z);
                            if level.get_block_state(at)? == state {
                                consumer(level, at)?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn change_state(
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
    scheduled: bool,
) -> Result<(), LevelError> {
    let open = state.block() == Block::OPEN_EYEBLOSSOM;
    let should_open = match level.get_value(EYEBLOSSOM_OPEN, pos)? {
        TriState::True => true,
        TriState::False => false,
        TriState::Default => open,
    };
    if should_open == open {
        return Ok(());
    }
    let transformed = if open {
        Block::CLOSED_EYEBLOSSOM
    } else {
        Block::OPEN_EYEBLOSSOM
    };
    LevelAccessRef::Server(level).set_block(pos, transformed.default_state(), UpdateFlags::ALL)?;
    let start = Vec3::at_center_of(pos.into());
    level.game_event(GameEvent::BLOCK_CHANGE, start, None, Some(state))?;
    let lifetime = 0.5 + level.random().next_double();
    let velocity = Vec3::new(
        level.random().next_double() - 0.5,
        level.random().next_double() + 1.0,
        level.random().next_double() - 0.5,
    );
    let target = start + velocity.scale(lifetime);
    level.send_particles(
        Particle::Trail(Trail {
            target: ferraloom_protocol::entity_data::Vec3 {
                x: target.x,
                y: target.y,
                z: target.z,
            },
            color: if open { 6250335 } else { 16545810 },
            duration: VarInt((20.0 * lifetime) as i32),
        }),
        start,
        1,
        Vec3::ZERO,
        0.0,
    )?;
    for_each_matching(state, pos, level, |level, nearby| {
        let dx = f64::from(pos.x - nearby.x);
        let dy = f64::from(pos.y - nearby.y);
        let dz = f64::from(pos.z - nearby.z);
        let distance = (dx * dx + dy * dy + dz * dz).sqrt();
        let minimum = (distance * 5.0) as i32;
        let maximum = (distance * 10.0) as i32;
        let delay = minimum + level.random().next_int_bound(maximum - minimum + 1);
        level.schedule_block_tick_after(nearby, state.block(), delay)
    })?;
    let sound = match (open, scheduled) {
        (false, false) => SoundEvent::BLOCK_EYEBLOSSOM_OPEN_LONG,
        (false, true) => SoundEvent::BLOCK_EYEBLOSSOM_OPEN,
        (true, false) => SoundEvent::BLOCK_EYEBLOSSOM_CLOSE_LONG,
        (true, true) => SoundEvent::BLOCK_EYEBLOSSOM_CLOSE,
    };
    level.play_sound(None, start, sound, SoundSource::Blocks, 1.0, 1.0)
}
impl BlockBehavior for Eyeblossom {
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        change_state(state, pos, level, false)
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        change_state(state, pos, level, true)
    }
}
