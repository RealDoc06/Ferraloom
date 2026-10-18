use super::{BoundingBox, piece::StructurePieceData};
use crate::{
    Error, Identifier,
    region::{Heightmap, WorldGenRegion},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{Axis, Direction};

/// Port of ScatteredFeaturePiece's dimensions and cached ground adjustment.
#[derive(Clone, Debug)]
pub struct ScatteredFeaturePiece {
    pub data: StructurePieceData,
    pub size: [i32; 3],
    pub height_position: i32,
}
impl ScatteredFeaturePiece {
    pub fn new(position: [i32; 3], size: [i32; 3], direction: Direction) -> Self {
        let extent = if direction.axis() == Axis::Z {
            size
        } else {
            [size[2], size[1], size[0]]
        };
        Self {
            data: StructurePieceData::new(
                BoundingBox::new(
                    position,
                    std::array::from_fn(|i| position[i] + extent[i] - 1),
                ),
                0,
                Some(direction),
            ),
            size,
            height_position: -1,
        }
    }
    pub fn to_nbt(&self, id: &Identifier) -> CompoundTag {
        let mut tag = self.data.to_nbt(id);
        for (key, index) in [("Width", 0), ("Height", 1), ("Depth", 2)] {
            tag.insert(key, Tag::Int(self.size[index]));
        }
        tag.insert("HPos", Tag::Int(self.height_position));
        tag
    }
    pub fn from_nbt(tag: &CompoundTag) -> Result<Self, Error> {
        Ok(Self {
            data: StructurePieceData::from_nbt(tag)?,
            size: [
                tag.get_int("Width").unwrap_or(0),
                tag.get_int("Height").unwrap_or(0),
                tag.get_int("Depth").unwrap_or(0),
            ],
            height_position: tag.get_int("HPos").unwrap_or(0),
        })
    }
    /// Port of updateAverageGroundHeight, including clipping samples at y=64.
    pub fn update_average_ground_height(
        &mut self,
        region: &WorldGenRegion<'_>,
        bounds: BoundingBox,
        offset: i32,
    ) -> Result<bool, Error> {
        if self.height_position >= 0 {
            return Ok(true);
        }
        let mut total = 0i32;
        let mut count = 0;
        for z in self.data.bounds.min[2]..=self.data.bounds.max[2] {
            for x in self.data.bounds.min[0]..=self.data.bounds.max[0] {
                if bounds.contains([x, 64, z]) {
                    total = total.wrapping_add(region.height_at(
                        Heightmap::MotionBlockingNoLeaves,
                        x,
                        z,
                    )?);
                    count += 1;
                }
            }
        }
        if count == 0 {
            return Ok(false);
        }
        self.height_position = total / count;
        self.data.bounds = self.data.bounds.moved([
            0,
            self.height_position - self.data.bounds.min[1] + offset,
            0,
        ]);
        Ok(true)
    }
    /// Port of updateHeightPositionToLowestGroundHeight.
    pub fn update_lowest_ground_height(
        &mut self,
        region: &WorldGenRegion<'_>,
        offset: i32,
    ) -> Result<bool, Error> {
        if self.height_position >= 0 {
            return Ok(true);
        }
        let mut lowest = region.min_y() + region.height();
        let mut found = false;
        for z in self.data.bounds.min[2]..=self.data.bounds.max[2] {
            for x in self.data.bounds.min[0]..=self.data.bounds.max[0] {
                lowest = lowest.min(region.height_at(Heightmap::MotionBlockingNoLeaves, x, z)?);
                found = true;
            }
        }
        if !found {
            return Ok(false);
        }
        self.height_position = lowest;
        self.data.bounds =
            self.data
                .bounds
                .moved([0, lowest - self.data.bounds.min[1] + offset, 0]);
        Ok(true)
    }
}
