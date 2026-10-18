use crate::Axis;

/// Port of OctahedralGroup's signed axis permutations, including reflections.
/// Each output axis selects an input axis and optionally reverses its direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OctahedralGroup {
    permutation: [Axis; 3],
    inversions: [bool; 3],
}

impl OctahedralGroup {
    pub const IDENTITY: Self = Self {
        permutation: [Axis::X, Axis::Y, Axis::Z],
        inversions: [false; 3],
    };
    pub const BLOCK_ROT_X_90: Self = Self {
        permutation: [Axis::X, Axis::Z, Axis::Y],
        inversions: [false, false, true],
    };
    pub const BLOCK_ROT_X_180: Self = Self {
        permutation: [Axis::X, Axis::Y, Axis::Z],
        inversions: [false, true, true],
    };
    pub const BLOCK_ROT_X_270: Self = Self {
        permutation: [Axis::X, Axis::Z, Axis::Y],
        inversions: [false, true, false],
    };
    pub const BLOCK_ROT_Y_90: Self = Self {
        permutation: [Axis::Z, Axis::Y, Axis::X],
        inversions: [true, false, false],
    };
    pub const BLOCK_ROT_Y_180: Self = Self {
        permutation: [Axis::X, Axis::Y, Axis::Z],
        inversions: [true, false, true],
    };
    pub const BLOCK_ROT_Y_270: Self = Self {
        permutation: [Axis::Z, Axis::Y, Axis::X],
        inversions: [false, false, true],
    };
    pub const BLOCK_ROT_Z_90: Self = Self {
        permutation: [Axis::Y, Axis::X, Axis::Z],
        inversions: [false, true, false],
    };
    pub const BLOCK_ROT_Z_180: Self = Self {
        permutation: [Axis::X, Axis::Y, Axis::Z],
        inversions: [true, true, false],
    };
    pub const BLOCK_ROT_Z_270: Self = Self {
        permutation: [Axis::Y, Axis::X, Axis::Z],
        inversions: [true, false, false],
    };

    pub fn new(permutation: [Axis; 3], inversions: [bool; 3]) -> Self {
        assert!(
            permutation[0] != permutation[1]
                && permutation[1] != permutation[2]
                && permutation[0] != permutation[2]
        );
        Self {
            permutation,
            inversions,
        }
    }
    pub fn permutation(self) -> [Axis; 3] {
        self.permutation
    }
    pub fn inverts(self, axis: Axis) -> bool {
        self.inversions[axis as usize]
    }
    pub fn compose(self, that: Self) -> Self {
        Self {
            permutation: self.permutation.map(|a| that.permutation[a as usize]),
            inversions: std::array::from_fn(|i| {
                self.inversions[i] ^ that.inversions[self.permutation[i] as usize]
            }),
        }
    }
    pub fn inverse(self) -> Self {
        let permutation = Axis::VALUES.map(|a| {
            Axis::VALUES[self
                .permutation
                .iter()
                .position(|&p| p == a)
                .expect("permutation")]
        });
        Self {
            permutation,
            inversions: permutation.map(|a| self.inversions[a as usize]),
        }
    }
}
