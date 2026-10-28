//! Living-entity fallback of ServerLevel#findLightningTargetAround.
use super::{Level, LevelError};
use crate::{block::LevelReaderRef, entity::EntityData};
use ferraloom_util::{Aabb, BlockPos, random::RandomSource};

impl Level {
    /// Port of the living-target portion of ServerLevel#findLightningTargetAround,
    /// after the heightmap center has been computed and POI rod lookup is empty.
    /// The caller must perform that lookup first; this is not a rod-free shortcut.
    pub fn find_lightning_target_among_entities(
        &mut self,
        center: BlockPos,
    ) -> Result<BlockPos, LevelError> {
        let search = Aabb::new(
            f64::from(center.x),
            f64::from(center.y),
            f64::from(center.z),
            f64::from(center.x) + 1.0,
            f64::from(self.max_build_height()) + 2.0,
            f64::from(center.z) + 1.0,
        )
        .inflate_uniform(3.0);
        let mut candidates = Vec::new();
        for entity in self.entities_in(search, |_, state| state.as_living().is_some()) {
            if self.entity_behavior(entity)?.is_alive(entity, self)? {
                let pos = self.entity::<EntityData>(entity)?.block_position;
                if LevelReaderRef::Server(self).can_see_sky(pos)? {
                    candidates.push(pos);
                }
            }
        }
        if !candidates.is_empty() {
            let index = self.random().next_int_bound(candidates.len() as i32) as usize;
            return Ok(candidates[index]);
        }
        Ok(if center.y == self.min_build_height() - 1 {
            center.above(2)
        } else {
            center
        })
    }
}
