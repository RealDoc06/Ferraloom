use ferraloom_util::BlockPos;

/// Port of BlockPos#manhattanOrdered. Owned positions replace the native mutable
/// cursor; positive Z precedes its negative mirror at every Manhattan depth.
#[derive(Clone)]
pub struct ManhattanPositions {
    origin: BlockPos,
    reach_xz: i32,
    reach_y: i32,
    max_depth: i32,
    depth: i32,
    max_x: i32,
    max_y: i32,
    x: i32,
    y: i32,
    mirror: Option<BlockPos>,
}
impl ManhattanPositions {
    pub(super) fn new(origin: BlockPos, reach_xz: i32, reach_y: i32, max_depth: i32) -> Self {
        Self {
            origin,
            reach_xz,
            reach_y,
            max_depth,
            depth: 0,
            max_x: 0,
            max_y: 0,
            x: 0,
            y: 0,
            mirror: None,
        }
    }
}
impl Iterator for ManhattanPositions {
    type Item = BlockPos;
    fn next(&mut self) -> Option<BlockPos> {
        if let Some(pos) = self.mirror.take() {
            return Some(pos);
        }
        loop {
            if self.y > self.max_y {
                self.x += 1;
                if self.x > self.max_x {
                    self.depth += 1;
                    if self.depth > self.max_depth {
                        return None;
                    }
                    self.max_x = self.reach_xz.min(self.depth);
                    self.x = -self.max_x;
                }
                self.max_y = self.reach_y.min(self.depth - self.x.abs());
                self.y = -self.max_y;
            }
            let z = self.depth - self.x.abs() - self.y.abs();
            let pos = self.origin.offset(self.x, self.y, z);
            self.y += 1;
            if z <= self.reach_xz {
                if z != 0 {
                    self.mirror = Some(self.origin.offset(self.x, self.y - 1, -z));
                }
                return Some(pos);
            }
        }
    }
}
