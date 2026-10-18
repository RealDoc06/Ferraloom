use ferraloom_util::Direction;

/// Port of structure.BoundingBox. Both ends are inclusive block coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundingBox {
    pub min: [i32; 3],
    pub max: [i32; 3],
}
impl BoundingBox {
    pub fn new(a: [i32; 3], b: [i32; 3]) -> Self {
        Self {
            min: std::array::from_fn(|i| a[i].min(b[i])),
            max: std::array::from_fn(|i| a[i].max(b[i])),
        }
    }
    pub const fn point(p: [i32; 3]) -> Self {
        Self { min: p, max: p }
    }
    pub const fn infinite() -> Self {
        Self {
            min: [i32::MIN; 3],
            max: [i32::MAX; 3],
        }
    }
    pub fn contains(self, p: [i32; 3]) -> bool {
        (0..3).all(|i| p[i] >= self.min[i] && p[i] <= self.max[i])
    }
    pub fn intersects(self, other: Self) -> bool {
        (0..3).all(|i| self.max[i] >= other.min[i] && self.min[i] <= other.max[i])
    }
    pub fn intersects_xz(self, min: [i32; 2], max: [i32; 2]) -> bool {
        self.max[0] >= min[0]
            && self.min[0] <= max[0]
            && self.max[2] >= min[1]
            && self.min[2] <= max[1]
    }
    pub fn encapsulating(self, other: Self) -> Self {
        Self {
            min: std::array::from_fn(|i| self.min[i].min(other.min[i])),
            max: std::array::from_fn(|i| self.max[i].max(other.max[i])),
        }
    }
    pub fn moved(self, delta: [i32; 3]) -> Self {
        Self::new(
            std::array::from_fn(|i| self.min[i].wrapping_add(delta[i])),
            std::array::from_fn(|i| self.max[i].wrapping_add(delta[i])),
        )
    }
    pub fn inflated(self, amount: i32) -> Self {
        Self::new(
            self.min.map(|v| v.wrapping_sub(amount)),
            self.max.map(|v| v.wrapping_add(amount)),
        )
    }
    pub fn span(self) -> [i32; 3] {
        std::array::from_fn(|i| self.max[i].wrapping_sub(self.min[i]).wrapping_add(1))
    }
    pub fn center(self) -> [i32; 3] {
        let span = self.span();
        std::array::from_fn(|i| self.min[i].wrapping_add(span[i] / 2))
    }
    /// Port of BoundingBox#forAllCorners, preserving the callback order.
    pub fn corners(self) -> [[i32; 3]; 8] {
        std::array::from_fn(|i| {
            std::array::from_fn(|axis| {
                if i & (1 << axis) == 0 {
                    self.max[axis]
                } else {
                    self.min[axis]
                }
            })
        })
    }
    pub fn is_close_to_chunk(self, chunk: [i32; 2], radius: i32) -> bool {
        let min = chunk.map(|v| v.wrapping_mul(16).wrapping_sub(radius));
        let max = chunk.map(|v| v.wrapping_mul(16).wrapping_add(15).wrapping_add(radius));
        self.intersects_xz(min, max)
    }
    /// Port of BoundingBox#orientBox. Down/Up share vanilla's South default.
    pub fn orient(foot: [i32; 3], offset: [i32; 3], size: [i32; 3], direction: Direction) -> Self {
        let [x, y, z] = foot;
        let [ox, oy, oz] = offset;
        let [w, h, d] = size;
        let (min, max) = match direction {
            Direction::North => (
                [
                    x.wrapping_add(ox),
                    y.wrapping_add(oy),
                    z.wrapping_sub(d).wrapping_add(1).wrapping_add(oz),
                ],
                [
                    x.wrapping_add(w).wrapping_sub(1).wrapping_add(ox),
                    y.wrapping_add(h).wrapping_sub(1).wrapping_add(oy),
                    z.wrapping_add(oz),
                ],
            ),
            Direction::West => (
                [
                    x.wrapping_sub(d).wrapping_add(1).wrapping_add(oz),
                    y.wrapping_add(oy),
                    z.wrapping_add(ox),
                ],
                [
                    x.wrapping_add(oz),
                    y.wrapping_add(h).wrapping_sub(1).wrapping_add(oy),
                    z.wrapping_add(w).wrapping_sub(1).wrapping_add(ox),
                ],
            ),
            Direction::East => (
                [x.wrapping_add(oz), y.wrapping_add(oy), z.wrapping_add(ox)],
                [
                    x.wrapping_add(d).wrapping_sub(1).wrapping_add(oz),
                    y.wrapping_add(h).wrapping_sub(1).wrapping_add(oy),
                    z.wrapping_add(w).wrapping_sub(1).wrapping_add(ox),
                ],
            ),
            _ => (
                [x.wrapping_add(ox), y.wrapping_add(oy), z.wrapping_add(oz)],
                [
                    x.wrapping_add(w).wrapping_sub(1).wrapping_add(ox),
                    y.wrapping_add(h).wrapping_sub(1).wrapping_add(oy),
                    z.wrapping_add(d).wrapping_sub(1).wrapping_add(oz),
                ],
            ),
        };
        Self::new(min, max)
    }
}
