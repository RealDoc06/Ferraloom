//! Native BlockState shape caches, kept separate from legacy AABB lookup tables.
use std::sync::OnceLock;

use crate::BlockState;
use ferraloom_util::{
    Direction, VoxelShape,
    shapes::{self, BitSetDiscreteVoxelShape, Coordinates, VoxelShapeKind},
};

/// Identity of a native vanilla shape. Equal snapshots can still have distinct
/// identities, which preserves Shapes' reference-identity shortcuts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NativeShapeId(u16);

impl NativeShapeId {
    pub fn get(self) -> &'static VoxelShape {
        let index = self.0 as usize;
        NATIVE_SHAPES[index].get_or_init(|| SNAPSHOTS[SHAPE_SNAPSHOTS[index] as usize].build())
    }
}

/// Empty-world baselines for collision/outline and vanilla's actual occlusion
/// caches. Dynamic states require live game queries for collision and outline.
#[derive(Clone, Copy, Debug)]
pub struct BlockStateShapes {
    pub collision: NativeShapeId,
    pub outline: NativeShapeId,
    pub occlusion: NativeShapeId,
    /// Direction's vanilla ordinal order: down, up, north, south, west, east.
    pub face_occlusion: [NativeShapeId; 6],
    pub context_dependent: bool,
}

impl BlockState {
    /// Extracted baselines, including flagged states. Simulation should use
    /// collision_shape/shape so a context-dependent baseline cannot slip through.
    pub fn cached_shapes(self) -> &'static BlockStateShapes {
        &STATE_SHAPES[self.id() as usize]
    }

    /// Port of BlockStateBase#getCollisionShape's static cache. None requires
    /// a live level, position or entity collision context in game code.
    pub fn collision_shape(self) -> Option<&'static VoxelShape> {
        let data = self.cached_shapes();
        (!data.context_dependent).then(|| data.collision.get())
    }

    /// Empty-context outline, available only for context-independent states.
    pub fn shape(self) -> Option<&'static VoxelShape> {
        let data = self.cached_shapes();
        (!data.context_dependent).then(|| data.outline.get())
    }

    /// Port of BlockStateBase#getOcclusionShape; this is always a state cache.
    pub fn occlusion_shape(self) -> &'static VoxelShape {
        self.cached_shapes().occlusion.get()
    }

    /// Port of BlockStateBase#getFaceOcclusionShape. Vanilla canonicalizes all
    /// six entries of a fully occluding state to Shapes.block().
    pub fn face_occlusion_shape(self, direction: Direction) -> &'static VoxelShape {
        self.cached_shapes().face_occlusion[direction as usize].get()
    }
}

#[derive(Clone, Copy)]
enum NativeCoordinates {
    Cube(usize),
    Array(&'static [u64]),
}
impl NativeCoordinates {
    fn build(self) -> Coordinates {
        match self {
            Self::Cube(size) => Coordinates::cube(size),
            Self::Array(bits) => {
                Coordinates::array(bits.iter().copied().map(f64::from_bits).collect::<Vec<_>>())
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Singleton {
    None,
    Block,
    Empty,
}

struct NativeSnapshot {
    kind: VoxelShapeKind,
    singleton: Singleton,
    size: [i32; 3],
    bounds: [[i32; 3]; 2],
    coordinates: [NativeCoordinates; 3],
    bits: &'static [u64],
}
impl NativeSnapshot {
    fn build(&self) -> VoxelShape {
        match self.singleton {
            Singleton::Block => shapes::block(),
            Singleton::Empty => shapes::empty(),
            Singleton::None => VoxelShape::from_snapshot(
                BitSetDiscreteVoxelShape::from_snapshot(self.size, self.bits, self.bounds).into(),
                self.coordinates.map(NativeCoordinates::build),
                self.kind,
            ),
        }
    }
}

include!(concat!(env!("OUT_DIR"), "/voxel_shapes.rs"));

#[cfg(test)]
mod tests;
