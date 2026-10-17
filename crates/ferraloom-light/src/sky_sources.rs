use crate::{LightWorld, shape_occludes};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::core::{BlockPos, ChunkPos, Direction};

/// Port of `ChunkSkyLightSources`. Values are lowest unobstructed source edges,
/// not the highest non-air block. Open columns extend to negative infinity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChunkSkyLightSources {
    min_y: i32,
    heights: [i32; 256],
}
impl ChunkSkyLightSources {
    pub fn new(min_y: i32) -> Self {
        Self {
            min_y: min_y - 1,
            heights: [min_y - 1; 256],
        }
    }
    pub fn fill_from<W: LightWorld + ?Sized>(&mut self, world: &W, chunk: ChunkPos) {
        for z in 0..16 {
            for x in 0..16 {
                let mut top = Block::AIR.default_state();
                let mut lowest = self.min_y;
                for y in (world.min_section() * 16..world.max_section() * 16).rev() {
                    let bottom = world.block_state(chunk.block_at(x, y, z));
                    if edge_occluded(top, bottom) {
                        lowest = y + 1;
                        break;
                    }
                    top = bottom;
                }
                self.heights[(z * 16 + x) as usize] = lowest;
            }
        }
    }
    /// Call after the snapshot's block change and before checking its light.
    pub fn update<W: LightWorld + ?Sized>(&mut self, world: &W, pos: BlockPos) -> bool {
        let index = ((pos.z & 15) * 16 + (pos.x & 15)) as usize;
        let old = self.heights[index];
        if pos.y + 1 < old {
            return false;
        }
        let middle = world.block_state(pos);
        for (top_pos, top, bottom_pos, bottom) in [
            (pos.above(1), world.block_state(pos.above(1)), pos, middle),
            (pos, middle, pos.below(1), world.block_state(pos.below(1))),
        ] {
            let edge_y = top_pos.y;
            if edge_occluded(top, bottom) {
                if edge_y > old {
                    self.heights[index] = edge_y;
                    return true;
                }
            } else if edge_y == old {
                let mut top = bottom;
                let mut lowest = self.min_y;
                for y in (self.min_y..bottom_pos.y).rev() {
                    let bottom = world.block_state(BlockPos::new(pos.x, y, pos.z));
                    if edge_occluded(top, bottom) {
                        lowest = y + 1;
                        break;
                    }
                    top = bottom;
                }
                self.heights[index] = lowest;
                return true;
            }
        }
        false
    }
    pub fn lowest_source_y(&self, x: u8, z: u8) -> i32 {
        assert!(x < 16 && z < 16);
        self.extend(self.heights[usize::from(z) * 16 + usize::from(x)])
    }
    pub fn highest_lowest_source_y(&self) -> i32 {
        self.extend(*self.heights.iter().max().unwrap())
    }
    fn extend(&self, y: i32) -> i32 {
        if y == self.min_y { i32::MIN } else { y }
    }
}
fn edge_occluded(top: BlockState, bottom: BlockState) -> bool {
    bottom.data().opacity != 0 || shape_occludes(top, bottom, Direction::Down)
}
