//! Owned ChunkGenerationTask lifetimes, separate from per-holder status futures.
use ferraloom_game::level::{
    chunk_map::ChunkMap,
    generation::{ChunkPyramid, ChunkStatus, GenerationTaskToken},
};
use ferraloom_util::ChunkPos;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct HolderClaim {
    pos: ChunkPos,
    epoch: u64,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Integration {
    Waiting,
    Active,
    Complete,
}

/// Native task identity guards stale results; holder epochs guard final removal
/// and recreation. A cancelled task still owns its submitted layer until return.
pub(super) struct GenerationJob {
    pub pos: ChunkPos,
    pub epoch: u64,
    pub token: GenerationTaskToken,
    claims: Vec<HolderClaim>,
    cancelled: bool,
    worker_settled: bool,
    integration: Integration,
}

#[derive(Default)]
pub(super) struct GenerationJobs(BTreeMap<u64, GenerationJob>);

impl GenerationJobs {
    /// Port of ChunkGenerationTask#create's worst-case generation cache claim.
    /// Loading can execute a smaller footprint, but retains this native claim.
    pub fn claim(&mut self, chunks: &mut ChunkMap, pos: ChunkPos, token: GenerationTaskToken) {
        assert!(!self.0.contains_key(&token.id), "duplicate generation task");
        let radius = ChunkPyramid::generation()
            .step_to(token.target)
            .accumulated_radius_of(ChunkStatus::Empty)
            .unwrap_or(0) as i32;
        let mut claims = Vec::new();
        for x in pos.x - radius..=pos.x + radius {
            for z in pos.z - radius..=pos.z + radius {
                let holder = chunks.ensure_holder(ChunkPos::new(x, z));
                holder.generation.increase_generation_ref_count();
                claims.push(HolderClaim {
                    pos: holder.position,
                    epoch: holder.epoch,
                });
            }
        }
        let epoch = chunks.holders[&pos.pack()].epoch;
        self.0.insert(
            token.id,
            GenerationJob {
                pos,
                epoch,
                token,
                claims,
                cancelled: false,
                worker_settled: false,
                integration: Integration::Waiting,
            },
        );
    }

    pub fn cancel(&mut self, task: u64) {
        if let Some(job) = self.0.get_mut(&task) {
            job.cancelled = true;
        }
    }

    pub fn worker_settled(&mut self, task: u64) {
        if let Some(job) = self.0.get_mut(&task) {
            job.worker_settled = true;
        }
    }

    pub fn is_current(&self, chunks: &ChunkMap, task: u64) -> bool {
        self.0.get(&task).is_some_and(|job| {
            !job.cancelled
                && chunks.holders.get(&job.pos.pack()).is_some_and(|holder| {
                    holder.epoch == job.epoch && holder.generation.current_task() == Some(job.token)
                })
        })
    }

    /// The continuation stays reachable in the backend while an owned action
    /// runs. Active publication pins its neighborhood through nested pumping.
    pub fn begin_integration(&mut self, task: u64) {
        let job = self.0.get_mut(&task).expect("unknown generation task");
        assert!(
            job.integration != Integration::Active,
            "recursive task execution"
        );
        job.integration = Integration::Active;
    }

    pub fn suspend_integration(&mut self, task: u64) {
        let job = self.0.get_mut(&task).expect("unknown generation task");
        assert!(job.integration == Integration::Active);
        job.integration = Integration::Waiting;
    }

    pub fn complete_integration(&mut self, task: u64) {
        let job = self.0.get_mut(&task).expect("unknown generation task");
        job.integration = Integration::Complete;
    }

    pub fn get(&self, task: u64) -> Option<&GenerationJob> {
        self.0.get(&task)
    }

    /// Native releaseClaim plus removeTask compares task-object identity, so an
    /// older completion cannot clear a replacement task on the same holder.
    pub fn release_finished(&mut self, chunks: &mut ChunkMap) {
        let finished = self
            .0
            .iter()
            .filter_map(|(&id, job)| {
                (job.worker_settled
                    && (job.integration == Integration::Complete
                        || (job.cancelled && job.integration != Integration::Active)))
                    .then_some(id)
            })
            .collect::<Vec<_>>();
        for id in finished {
            let job = self.0.remove(&id).unwrap();
            let center = chunks
                .holders
                .get_mut(&job.pos.pack())
                .expect("claimed center was removed");
            assert_eq!(center.epoch, job.epoch, "claimed center was recreated");
            center.generation.remove_task(job.token);
            for claim in job.claims {
                let holder = chunks
                    .holders
                    .get_mut(&claim.pos.pack())
                    .expect("claimed dependency was removed");
                assert_eq!(
                    holder.epoch, claim.epoch,
                    "claimed dependency was recreated"
                );
                holder.generation.decrease_generation_ref_count();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_worker_and_active_publication_keep_claims_without_clearing_replacement() {
        let mut chunks = ChunkMap::default();
        let pos = ChunkPos::ZERO;
        chunks.ensure_holder(pos).ticket_level = 33;
        chunks.update_highest_allowed_status(pos);
        let old = chunks
            .schedule_generation_task(pos, ChunkStatus::Biomes)
            .task
            .schedule
            .unwrap();
        let mut jobs = GenerationJobs::default();
        jobs.claim(&mut chunks, pos, old);
        let full = chunks
            .schedule_generation_task(pos, ChunkStatus::Full)
            .task
            .schedule
            .unwrap();
        jobs.claim(&mut chunks, pos, full);
        jobs.cancel(old.id);
        jobs.release_finished(&mut chunks);
        assert!(jobs.get(old.id).is_some());
        assert!(
            !chunks.holders[&pos.pack()]
                .generation
                .generation_references_released()
        );
        jobs.worker_settled(old.id);
        jobs.release_finished(&mut chunks);
        assert!(jobs.get(old.id).is_none());
        assert_eq!(
            chunks.holders[&pos.pack()].generation.current_task(),
            Some(full)
        );

        jobs.begin_integration(full.id);
        jobs.worker_settled(full.id);
        jobs.cancel(full.id);
        jobs.release_finished(&mut chunks);
        assert!(jobs.get(full.id).is_some());
        jobs.suspend_integration(full.id);
        jobs.release_finished(&mut chunks);
        assert!(jobs.get(full.id).is_none());
        assert!(
            chunks
                .holders
                .values()
                .all(|holder| { holder.generation.generation_references_released() })
        );
        assert_eq!(chunks.holders[&pos.pack()].generation.current_task(), None);
    }
}
