//! Compatibility paths for the shared live collision adapters.
pub use ferraloom_game::collisions::{
    EmptyCollisions, EntityCollisions, LookupCollisions, PowderSnowSubject, load,
    no_collision_empty, no_collision_or_liquid, with_piston_noclip,
};

/// The shared adapter over Horizon's current resident world.
pub type SceneCollisions<'w, 'r> =
    ferraloom_game::collisions::SceneCollisions<'w, 'r, crate::world::World>;

impl ferraloom_game::collisions::ResidentWorld for crate::world::World {
    fn chunk_sections(
        &self,
        pos: horizon_core::position::ChunkPos,
    ) -> Option<&[std::sync::Arc<crate::chunk::Section>]> {
        self.chunks
            .get(&(pos.x, pos.z))
            .map(|chunk| chunk.sections.as_slice())
    }
    fn block(&self, x: i32, y: i32, z: i32) -> horizon_world::registry::BlockStateId {
        self.block(x, y, z)
    }
    fn moving_piston(
        &self,
        pos: [i32; 3],
    ) -> Option<horizon_world::piston_shape::MovingPistonState> {
        self.moving_pistons
            .iter()
            .find(|(p, _)| *p == pos)
            .map(|(_, piston)| *piston)
    }
}
