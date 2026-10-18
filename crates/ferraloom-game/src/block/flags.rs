//! Port of Block.UpdateFlags. Unknown bits survive recursive shape updates.
use std::ops::{BitAnd, BitOr, Not};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UpdateFlags(pub u32);
impl UpdateFlags {
    pub const NEIGHBORS: Self = Self(1);
    pub const CLIENTS: Self = Self(2);
    pub const INVISIBLE: Self = Self(4);
    pub const IMMEDIATE: Self = Self(8);
    pub const KNOWN_SHAPE: Self = Self(16);
    pub const SUPPRESS_DROPS: Self = Self(32);
    pub const MOVE_BY_PISTON: Self = Self(64);
    pub const SKIP_SHAPE_UPDATE_ON_WIRE: Self = Self(128);
    pub const SKIP_BLOCK_ENTITY_SIDEEFFECTS: Self = Self(256);
    pub const SKIP_ON_PLACE: Self = Self(512);
    pub const NONE: Self = Self(260);
    pub const ALL: Self = Self(3);
    pub const ALL_IMMEDIATE: Self = Self(11);
    pub const SKIP_ALL_SIDEEFFECTS: Self = Self(816);
    pub const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }
    pub const fn without(self, flags: Self) -> Self {
        Self(self.0 & !flags.0)
    }
}
impl BitOr for UpdateFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
impl BitAnd for UpdateFlags {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}
impl Not for UpdateFlags {
    type Output = Self;
    fn not(self) -> Self {
        Self(!self.0)
    }
}
pub const UPDATE_LIMIT: i32 = 512;
