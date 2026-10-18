//! Owner-thread state of vanilla's GenerationChunkHolder. Worker tasks carry
//! owned neighborhoods; this state owns their status futures and cancellation.
use super::{ChunkStatus, STATUSES};
use crate::level::acquisition::{AcquireFailure, ChunkReference};

const STATUS_COUNT: usize = STATUSES.len();

/// Identifies a status future, independently of replacement generation tasks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationFutureId(pub u64);
impl GenerationFutureId {
    pub const UNLOADED: Self = Self(0);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenerationFutureResult {
    Pending,
    Complete(Option<ChunkReference>),
    Failed(AcquireFailure),
}

#[derive(Clone, Copy, Debug)]
struct StatusFuture {
    id: GenerationFutureId,
    result: GenerationFutureResult,
}

/// Native task-object identity and target. Replacing it cancels the old task,
/// while futures already returned to callers retain their identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationTaskToken {
    pub id: u64,
    pub target: ChunkStatus,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GenerationTaskChange {
    pub cancel: Option<GenerationTaskToken>,
    pub schedule: Option<GenerationTaskToken>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationSchedule {
    pub future: GenerationFutureId,
    pub task: GenerationTaskChange,
}

/// Port of GenerationChunkHolder's futures, startedWork, highestAllowedStatus,
/// current task and generation references. Stored once on its owning holder.
#[derive(Debug, Default)]
pub struct HolderGeneration {
    highest_allowed: Option<ChunkStatus>,
    started_work: Option<ChunkStatus>,
    futures: [Option<StatusFuture>; STATUS_COUNT],
    future_versions: [u64; STATUS_COUNT],
    task: Option<GenerationTaskToken>,
    generation_ref_count: u32,
}

impl HolderGeneration {
    pub fn highest_allowed_status(&self) -> Option<ChunkStatus> {
        self.highest_allowed
    }
    pub fn started_work(&self) -> Option<ChunkStatus> {
        self.started_work
    }
    pub fn latest_completed_status(&self) -> Option<ChunkStatus> {
        STATUSES
            .iter()
            .rev()
            .copied()
            .find(|&status| self.chunk_if_present_unchecked(status).is_some())
    }
    pub fn current_task(&self) -> Option<GenerationTaskToken> {
        self.task
    }
    pub fn is_allowed(&self, status: ChunkStatus) -> bool {
        self.highest_allowed
            .is_some_and(|highest| status <= highest)
    }

    /// Port of scheduleChunkGenerationTask: an upgrade replaces the task, not
    /// a pending future. A completed or disallowed future schedules no work.
    pub fn schedule(
        &mut self,
        status: ChunkStatus,
        next_task: &mut impl FnMut(ChunkStatus) -> GenerationTaskToken,
    ) -> GenerationSchedule {
        if !self.is_allowed(status) {
            return GenerationSchedule {
                future: GenerationFutureId::UNLOADED,
                task: GenerationTaskChange::default(),
            };
        }
        let future = self.get_or_create_future(status);
        let pending = self.futures[status as usize]
            .is_some_and(|future| future.result == GenerationFutureResult::Pending);
        let task = if pending && self.task.is_none_or(|task| status > task.target) {
            self.reschedule(Some(status), next_task)
        } else {
            GenerationTaskChange::default()
        };
        GenerationSchedule { future, task }
    }

    /// Port of updateHighestAllowedStatus. Pending futures above a downgrade
    /// complete UNLOADED and clear; completed results remain cached unchecked.
    pub fn update_highest_allowed_status(
        &mut self,
        highest: Option<ChunkStatus>,
        next_task: &mut impl FnMut(ChunkStatus) -> GenerationTaskToken,
    ) -> GenerationTaskChange {
        let old = self.highest_allowed;
        self.highest_allowed = highest;
        if old.is_none_or(|old| highest.is_some_and(|highest| highest >= old)) {
            return GenerationTaskChange::default();
        }
        let start = highest.map_or(0, |status| status as usize + 1);
        for index in start..=old.unwrap() as usize {
            if self.futures[index]
                .is_some_and(|future| future.result == GenerationFutureResult::Pending)
            {
                self.futures[index] = None;
            }
        }
        if self.task.is_some() {
            self.reschedule(self.highest_pending(highest), next_task)
        } else {
            GenerationTaskChange::default()
        }
    }

    fn highest_pending(&self, highest: Option<ChunkStatus>) -> Option<ChunkStatus> {
        let highest = highest?;
        STATUSES[..=highest as usize]
            .iter()
            .rev()
            .copied()
            .take_while(|&status| self.started_work.is_none_or(|started| status > started))
            .find(|&status| self.futures[status as usize].is_some())
    }

    fn reschedule(
        &mut self,
        target: Option<ChunkStatus>,
        next_task: &mut impl FnMut(ChunkStatus) -> GenerationTaskToken,
    ) -> GenerationTaskChange {
        let schedule = target.map(next_task);
        let cancel = std::mem::replace(&mut self.task, schedule);
        GenerationTaskChange { cancel, schedule }
    }

    fn get_or_create_future(&mut self, status: ChunkStatus) -> GenerationFutureId {
        let index = status as usize;
        if let Some(future) = self.futures[index] {
            return future.id;
        }
        let version = self.future_versions[index];
        let id = GenerationFutureId(
            version
                .checked_mul(STATUS_COUNT as u64)
                .and_then(|value| value.checked_add(index as u64 + 1))
                .expect("chunk status future identities exhausted"),
        );
        self.future_versions[index] = version
            .checked_add(1)
            .expect("chunk status future versions exhausted");
        self.futures[index] = Some(StatusFuture {
            id,
            result: GenerationFutureResult::Pending,
        });
        id
    }

    /// Cleared old futures are known to be UNLOADED. Versioned IDs preserve that
    /// outcome without retaining an unbounded history of cancelled futures.
    pub fn result(
        &self,
        status: ChunkStatus,
        id: GenerationFutureId,
    ) -> Option<GenerationFutureResult> {
        if id == GenerationFutureId::UNLOADED {
            return Some(GenerationFutureResult::Complete(None));
        }
        let index = status as usize;
        if (id.0 - 1) % STATUS_COUNT as u64 != index as u64 {
            return None;
        }
        if let Some(future) = self.futures[index]
            && future.id == id
        {
            return Some(future.result);
        }
        let version = (id.0 - 1) / STATUS_COUNT as u64;
        (version < self.future_versions[index]).then_some(GenerationFutureResult::Complete(None))
    }

    pub fn future_id(&self, status: ChunkStatus) -> Option<GenerationFutureId> {
        self.futures[status as usize].map(|future| future.id)
    }

    /// Port of acquireStatusBump: only the next native step starts new work.
    pub fn acquire_status_bump(&mut self, status: ChunkStatus) -> bool {
        if self.started_work.is_some_and(|started| status <= started) {
            return false;
        }
        let parent = (status != ChunkStatus::Empty).then(|| STATUSES[status as usize - 1]);
        assert_eq!(
            self.started_work, parent,
            "generation step skipped its parent"
        );
        self.started_work = Some(status);
        true
    }

    pub fn complete(&mut self, status: ChunkStatus, chunk: ChunkReference) {
        self.get_or_create_future(status);
        let future = self.futures[status as usize].as_mut().unwrap();
        assert_eq!(future.result, GenerationFutureResult::Pending);
        future.result = GenerationFutureResult::Complete(Some(chunk));
    }

    pub fn fail_pending(&mut self, failure: AcquireFailure) {
        for future in self.futures.iter_mut().flatten() {
            if future.result == GenerationFutureResult::Pending {
                future.result = GenerationFutureResult::Failed(failure);
            }
        }
    }

    /// Port of replaceProtoChunk. FULL conversion updates every earlier future
    /// to the same published LevelChunk through its read-only imposter identity.
    pub fn replace_proto_chunk(&mut self, chunk: ChunkReference) {
        for future in &mut self.futures[..ChunkStatus::Full as usize] {
            let future = future.as_mut().expect("FULL requires every parent future");
            assert!(matches!(
                future.result,
                GenerationFutureResult::Complete(Some(_))
            ));
            future.result = GenerationFutureResult::Complete(Some(chunk));
        }
    }

    pub fn chunk_if_present_unchecked(&self, status: ChunkStatus) -> Option<ChunkReference> {
        match self.futures[status as usize]?.result {
            GenerationFutureResult::Complete(chunk) => chunk,
            GenerationFutureResult::Pending | GenerationFutureResult::Failed(_) => None,
        }
    }
    pub fn chunk_if_present(&self, status: ChunkStatus) -> Option<ChunkReference> {
        self.is_allowed(status)
            .then(|| self.chunk_if_present_unchecked(status))
            .flatten()
    }

    pub fn remove_task(&mut self, task: GenerationTaskToken) -> bool {
        if self.task != Some(task) {
            return false;
        }
        self.task = None;
        true
    }

    pub fn increase_generation_ref_count(&mut self) {
        self.generation_ref_count = self
            .generation_ref_count
            .checked_add(1)
            .expect("chunk generation reference count exhausted");
    }
    pub fn decrease_generation_ref_count(&mut self) {
        self.generation_ref_count = self
            .generation_ref_count
            .checked_sub(1)
            .expect("generation release without a claim");
    }
    pub fn generation_references_released(&self) -> bool {
        self.generation_ref_count == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::acquisition::ChunkKind;
    use ferraloom_util::ChunkPos;

    #[test]
    fn replacing_tasks_preserves_futures_and_downgrades_clear_only_pending_statuses() {
        let mut holder = HolderGeneration::default();
        let mut next = 0;
        let mut task = |target| {
            let id = next;
            next += 1;
            GenerationTaskToken { id, target }
        };
        let absent = holder.schedule(ChunkStatus::Full, &mut task);
        assert_eq!(absent.future, GenerationFutureId::UNLOADED);
        assert_eq!(absent.task, GenerationTaskChange::default());
        holder.update_highest_allowed_status(Some(ChunkStatus::Full), &mut task);
        let biomes = holder.schedule(ChunkStatus::Biomes, &mut task);
        let terrain = holder.schedule(ChunkStatus::Terrain, &mut task);
        let full = holder.schedule(ChunkStatus::Full, &mut task);
        assert_eq!(terrain.task.cancel, biomes.task.schedule);
        assert_eq!(full.task.cancel, terrain.task.schedule);
        assert_eq!(holder.future_id(ChunkStatus::Biomes), Some(biomes.future));

        let proto = ChunkReference {
            pos: ChunkPos::ZERO,
            generation: 7,
            kind: ChunkKind::Proto,
        };
        for status in STATUSES[..=ChunkStatus::Biomes as usize].iter().copied() {
            assert!(holder.acquire_status_bump(status));
            holder.complete(status, proto);
        }
        assert!(!holder.acquire_status_bump(ChunkStatus::Biomes));
        let completed = holder.schedule(ChunkStatus::Biomes, &mut task);
        assert_eq!(completed.future, biomes.future);
        assert_eq!(completed.task, GenerationTaskChange::default());

        let downgrade = holder.update_highest_allowed_status(Some(ChunkStatus::Terrain), &mut task);
        assert_eq!(downgrade.cancel, full.task.schedule);
        assert_eq!(downgrade.schedule.unwrap().target, ChunkStatus::Terrain);
        assert_eq!(
            holder.result(ChunkStatus::Full, full.future),
            Some(GenerationFutureResult::Complete(None))
        );
        assert_eq!(
            holder.result(ChunkStatus::Biomes, biomes.future),
            Some(GenerationFutureResult::Complete(Some(proto)))
        );
        assert!(!holder.remove_task(full.task.schedule.unwrap()));
        assert_eq!(holder.current_task(), downgrade.schedule);

        holder.increase_generation_ref_count();
        holder.increase_generation_ref_count();
        holder.decrease_generation_ref_count();
        assert!(!holder.generation_references_released());
        holder.decrease_generation_ref_count();
        assert!(holder.generation_references_released());
        assert!(holder.remove_task(downgrade.schedule.unwrap()));

        holder.update_highest_allowed_status(Some(ChunkStatus::Full), &mut task);
        let retry = holder.schedule(ChunkStatus::Full, &mut task);
        assert_ne!(retry.future, full.future);
        assert_eq!(holder.future_id(ChunkStatus::Biomes), Some(biomes.future));
        assert_eq!(
            holder.result(ChunkStatus::Full, full.future),
            Some(GenerationFutureResult::Complete(None))
        );
        assert_eq!(holder.result(ChunkStatus::Terrain, biomes.future), None);
    }

    #[test]
    fn full_replaces_parent_results_and_demotion_keeps_unchecked_completed_chunks() {
        let mut holder = HolderGeneration::default();
        let mut task = |target| GenerationTaskToken { id: 1, target };
        holder.update_highest_allowed_status(Some(ChunkStatus::Full), &mut task);
        let pending = holder.schedule(ChunkStatus::Full, &mut task);
        let proto = ChunkReference {
            pos: ChunkPos::ZERO,
            generation: 8,
            kind: ChunkKind::Proto,
        };
        for status in STATUSES[..ChunkStatus::Full as usize].iter().copied() {
            assert!(holder.acquire_status_bump(status));
            holder.complete(status, proto);
        }
        let full = ChunkReference {
            kind: ChunkKind::Full,
            generation: 9,
            ..proto
        };
        holder.replace_proto_chunk(full);
        assert!(holder.acquire_status_bump(ChunkStatus::Full));
        holder.complete(ChunkStatus::Full, full);
        assert!(holder.remove_task(pending.task.schedule.unwrap()));
        holder.update_highest_allowed_status(None, &mut task);
        assert_eq!(holder.chunk_if_present(ChunkStatus::Full), None);
        for status in STATUSES {
            assert_eq!(holder.chunk_if_present_unchecked(status), Some(full));
        }
        assert_eq!(
            holder.result(ChunkStatus::Full, pending.future),
            Some(GenerationFutureResult::Complete(Some(full)))
        );
    }
}
