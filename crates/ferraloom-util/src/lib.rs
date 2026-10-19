//! Vanilla 26.3 math, geometry, random sources and noise primitives.
//!
//! Integer operations deliberately preserve Java overflow and shift behavior.
//! Floating-point expressions retain vanilla's evaluation order and precision.

pub mod collections;
pub mod core;
pub mod java;
pub mod mth;
pub mod noise;
pub mod phys;
pub mod random;
pub mod shapes;
pub mod uuid;

pub use core::{
    Axis, AxisDirection, BlockPos, ChunkPos, Direction, Mirror, MutableBlockPos, Plane, QuartPos,
    Rotation, SectionPos, Vec3i,
};
pub use phys::{Aabb, BlockHitResult, Vec3};
pub use random::{
    LegacyRandomSource, RandomSource, SingleThreadedRandomSource, WorldgenRandom,
    XoroshiroRandomSource,
};
pub use shapes::{BooleanOp, CollisionContext, VoxelShape};
pub use uuid::Uuid;

#[cfg(test)]
mod tests;
#[cfg(test)]
#[allow(clippy::type_complexity)]
mod vanilla_vectors;
