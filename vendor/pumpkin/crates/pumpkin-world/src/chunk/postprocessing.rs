use super::ChunkData;
use pumpkin_util::math::{position::BlockPos, vector2::Vector2};
use std::sync::atomic::Ordering;

/// Vanilla's packed local coordinate, independent of the dimension's minimum Y.
#[must_use]
pub const fn pack_position(pos: &BlockPos) -> i16 {
    ((pos.0.x & 15) | ((pos.0.y & 15) << 4) | ((pos.0.z & 15) << 8)) as i16
}

#[must_use]
pub fn unpack_position(packed: i16, section_y: i32, chunk: Vector2<i32>) -> BlockPos {
    let packed = i32::from(packed);
    BlockPos::new(
        (chunk.x << 4) + (packed & 15),
        (section_y << 4) + ((packed >> 4) & 15),
        (chunk.y << 4) + ((packed >> 8) & 15),
    )
}

impl ChunkData {
    pub fn mark_pos_for_postprocessing(&self, pos: &BlockPos) {
        if pos.0.x >> 4 != self.x || pos.0.z >> 4 != self.z {
            return;
        }
        let section = (pos.0.y >> 4) - (self.section.min_y >> 4);
        let Ok(section) = usize::try_from(section) else {
            return;
        };
        if section >= self.section.count {
            return;
        }
        let mut pending = self
            .post_processing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if pending.len() < self.section.count {
            pending.resize_with(self.section.count, Vec::new);
        }
        pending[section].push(pack_position(pos));
        self.post_processing_ready.store(false, Ordering::Release);
        self.dirty.store(true, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_post_processing_ready(&self) -> bool {
        self.post_processing_ready.load(Ordering::Acquire)
    }

    /// Called with the level's activation consumer serialized and its FULL halo pinned.
    /// Keep each mark persisted until its update succeeds. New marks append after the
    /// snapshot and are left for the next pass; callbacks run without the queue lock.
    pub fn process_post_processing(&self, mut process: impl FnMut(BlockPos) -> bool) -> bool {
        let snapshot = self
            .post_processing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let mut succeeded = true;
        for (section, positions) in snapshot.iter().enumerate() {
            let mut consumed = 0;
            for &packed in positions {
                if section >= self.section.count
                    || !process(unpack_position(
                        packed,
                        (self.section.min_y >> 4) + section as i32,
                        Vector2::new(self.x, self.z),
                    ))
                {
                    succeeded = false;
                    break;
                }
                consumed += 1;
            }
            if consumed > 0 {
                self.post_processing
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)[section]
                    .drain(..consumed);
                self.dirty.store(true, Ordering::Relaxed);
            }
            if !succeeded {
                break;
            }
        }
        let pending = self
            .post_processing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ready = succeeded && pending.iter().all(Vec::is_empty);
        self.post_processing_ready.store(ready, Ordering::Release);
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_coordinates_use_dimension_relative_sections_and_local_nibbles() {
        let chunk = ChunkData::empty(-2, -1);
        let bottom = BlockPos::new(-17, -64, -1);
        let next = BlockPos::new(-32, -47, -16);
        chunk.mark_pos_for_postprocessing(&bottom);
        chunk.mark_pos_for_postprocessing(&next);
        chunk.mark_pos_for_postprocessing(&BlockPos::new(-17, -65, -1));
        chunk.mark_pos_for_postprocessing(&BlockPos::new(0, -64, -1));
        let pending = chunk.post_processing.lock().unwrap();
        assert_eq!(pending[0], vec![0xf0f]);
        assert_eq!(pending[1], vec![0x010]);
        assert_eq!(
            unpack_position(pending[0][0], -4, Vector2::new(-2, -1)),
            bottom
        );
        assert_eq!(
            unpack_position(pending[1][0], -3, Vector2::new(-2, -1)),
            next
        );
    }

    #[test]
    fn failed_update_keeps_unconsumed_marks_and_success_is_one_shot() {
        let chunk = ChunkData::empty(0, 0);
        let first = BlockPos::new(1, -64, 2);
        let second = BlockPos::new(2, -64, 2);
        chunk.mark_pos_for_postprocessing(&first);
        chunk.mark_pos_for_postprocessing(&second);
        chunk.dirty.store(false, Ordering::Relaxed);
        let mut visited = Vec::new();
        assert!(!chunk.process_post_processing(|pos| {
            visited.push(pos);
            pos == first
        }));
        assert_eq!(visited, vec![first, second]);
        assert!(chunk.dirty.load(Ordering::Relaxed));
        assert!(!chunk.is_post_processing_ready());
        visited.clear();
        assert!(chunk.process_post_processing(|pos| {
            visited.push(pos);
            true
        }));
        assert_eq!(visited, vec![second]);
        assert!(chunk.process_post_processing(|_| panic!("consumed mark replayed")));
        chunk.mark_pos_for_postprocessing(&first);
        assert!(!chunk.is_post_processing_ready());
    }
}
