use super::{ChunkPyramid, ChunkStatus};
use crate::level::distance_manager::FullChunkStatus;
use ferraloom_util::ChunkPos;

/// Port of ChunkMap#getChunkRangeFuture and its three promotion callers. Each
/// required status is an actual completed runtime hook, not a ticket request.
#[derive(Clone, Debug)]
pub struct ChunkRangeFuture {
    requirements: Vec<(ChunkPos, ChunkStatus)>,
}
impl ChunkRangeFuture {
    pub fn for_full_status(center: ChunkPos, status: FullChunkStatus) -> Self {
        let radius = match status {
            FullChunkStatus::Inaccessible => {
                return Self {
                    requirements: Vec::new(),
                };
            }
            FullChunkStatus::Full | FullChunkStatus::BlockTicking => 1,
            FullChunkStatus::EntityTicking => 2,
        };
        let mut requirements = Vec::new();
        for x in center.x - radius..=center.x + radius {
            for z in center.z - radius..=center.z + radius {
                let distance = x.abs_diff(center.x).max(z.abs_diff(center.z)) as usize;
                let required = if status == FullChunkStatus::Full && distance > 0 {
                    ChunkPyramid::generation()
                        .step_to(ChunkStatus::Full)
                        .accumulated
                        .get(distance)
                        .unwrap()
                } else {
                    ChunkStatus::Full
                };
                requirements.push((ChunkPos::new(x, z), required));
            }
        }
        Self { requirements }
    }
    pub fn is_ready(&self, mut completed: impl FnMut(ChunkPos) -> Option<ChunkStatus>) -> bool {
        !self.requirements.is_empty()
            && self.requirements.iter().all(|&(position, required)| {
                completed(position).is_some_and(|status| status >= required)
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accessible_needs_initialized_neighbors_but_ticking_requires_full_neighbors() {
        let center = ChunkPos::new(0, 0);
        let status = |p| {
            Some(if p == center {
                ChunkStatus::Full
            } else {
                ChunkStatus::InitializeLight
            })
        };
        assert!(ChunkRangeFuture::for_full_status(center, FullChunkStatus::Full).is_ready(status));
        assert!(
            !ChunkRangeFuture::for_full_status(center, FullChunkStatus::BlockTicking)
                .is_ready(status)
        );
        assert!(
            !ChunkRangeFuture::for_full_status(center, FullChunkStatus::Full).is_ready(|p| {
                if p == center {
                    Some(ChunkStatus::Full)
                } else {
                    None
                }
            })
        );
    }
}
