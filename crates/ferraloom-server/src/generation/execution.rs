//! ChunkGenerationTask's resumable serial dispatch, separate from owned work.
use super::{GenerationCompletion, GenerationRevision, GenerationState, profile};
use crate::{jobs::JobPool, world::RegistryAdapter};
use anyhow::{Context, Result};
use ferraloom_game::level::generation::{ChunkGenerationTask, ChunkStatus};
use ferraloom_storage::IoWorker;
use ferraloom_util::ChunkPos;
use std::sync::Arc;
use std::{
    collections::{BTreeSet, VecDeque},
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::sync::oneshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GenerationStep {
    pub position: ChunkPos,
    pub status: ChunkStatus,
}
#[derive(Default)]
pub(crate) struct GenerationStepEffects {
    pub touched: BTreeSet<i64>,
    pub skips: Vec<(String, String)>,
}
struct Layer {
    status: ChunkStatus,
    positions: std::vec::IntoIter<ChunkPos>,
    timing: Option<profile::Span>,
}
/// The owner retains the task, remaining layer and accumulated effects while a
/// worker owns the cache for one step. Physical completion cannot select a visit.
pub(crate) struct GenerationProgress {
    pub task: ChunkGenerationTask,
    pub effects: GenerationStepEffects,
    layer: Option<Layer>,
    _timing: profile::Span,
}
impl GenerationProgress {
    /// Cancellation is observed between layers, matching the original executor.
    /// Cached completed steps are skipped here, without submitting empty jobs.
    pub fn next_step(
        &mut self,
        state: &GenerationState,
        cancelled: Option<&AtomicBool>,
    ) -> Result<Option<GenerationStep>> {
        loop {
            if let Some(layer) = &mut self.layer {
                for position in layer.positions.by_ref() {
                    let needed = if layer.status == ChunkStatus::Empty {
                        state.failed_loads.contains_key(&position.pack())
                            || !state.holders.contains_key(&position.pack())
                    } else {
                        state
                            .holders
                            .get(&position.pack())
                            .is_none_or(|holder| holder.completed < layer.status)
                    };
                    if needed {
                        return Ok(Some(GenerationStep {
                            position,
                            status: layer.status,
                        }));
                    }
                }
            }
            self.layer = None;
            if cancelled.is_some_and(|token| token.load(Ordering::Relaxed)) {
                self.task.cancel();
            }
            let Some(layer) = self
                .task
                .next_layer(|pos| {
                    state
                        .holders
                        .get(&pos.pack())
                        .map(|holder| holder.persisted)
                })
                .map_err(|error| anyhow::anyhow!("{error:?}"))?
            else {
                return Ok(None);
            };
            let mut timing =
                profile::Span::new("status_layer", Some((self.task.request, self.task.center)))
                    .status(layer.status);
            timing.items(layer.positions.len());
            self.layer = Some(Layer {
                status: layer.status,
                positions: layer.positions.into_iter(),
                timing: Some(timing),
            });
        }
    }
    pub fn take_step_timing(&mut self) -> profile::Span {
        self.layer
            .as_mut()
            .expect("planned layer")
            .timing
            .take()
            .expect("step is not in flight")
    }
    pub fn return_step_timing(&mut self, timing: profile::Span) {
        self.layer.as_mut().expect("scheduled layer").timing = Some(timing);
    }
    pub fn append_effects(&mut self, effects: GenerationStepEffects) {
        self.effects.touched.extend(effects.touched);
        self.effects.skips.extend(effects.skips);
    }
}
impl GenerationState {
    pub(crate) fn begin_generation(
        &mut self,
        request: u64,
        pos: ChunkPos,
        target: ChunkStatus,
    ) -> Result<GenerationProgress> {
        self.revision = GenerationRevision(
            self.revision
                .0
                .checked_add(1)
                .context("generation revision exhausted")?,
        );
        Ok(GenerationProgress {
            task: ChunkGenerationTask::new(request, pos, target),
            effects: GenerationStepEffects::default(),
            layer: None,
            _timing: profile::Span::new("generate", Some((request, pos))).status(target),
        })
    }
}
struct GenerationStepResult {
    timing: profile::Span,
    effects: GenerationStepEffects,
    result: Result<()>,
}

type PendingChunkRead = (
    ChunkPos,
    ferraloom_storage::IoTask<Option<ferraloom_nbt::CompoundTag>>,
);

/// One serial request continuation, owned by exactly one queued/active unit.
/// The FULL response remains reachable without requiring a tick between steps.
struct GenerationExecution {
    state: GenerationState,
    progress: GenerationProgress,
    request: super::GenerationRequest,
    retained: BTreeSet<i64>,
    registry: Arc<RegistryAdapter>,
    io: Arc<IoWorker>,
    pool: Arc<JobPool>,
    done: oneshot::Sender<GenerationCompletion>,
    job_timing: profile::Span,
    executor: tokio::runtime::Handle,
    reads: VecDeque<PendingChunkRead>,
}
pub(crate) fn dispatch_generation(
    mut state: GenerationState,
    request: super::GenerationRequest,
    retained: BTreeSet<i64>,
    registry: Arc<RegistryAdapter>,
    io: Arc<IoWorker>,
    pool: Arc<JobPool>,
) -> Result<oneshot::Receiver<GenerationCompletion>> {
    let executor = tokio::runtime::Handle::try_current()
        .context("generation dispatch requires the server executor")?;
    let progress = state.begin_generation(request.id, request.pos, request.status)?;
    let job_timing = profile::Span::new("generation_job", Some((request.id, request.pos)))
        .status(request.status);
    let (done, result) = oneshot::channel();
    GenerationExecution {
        state,
        progress,
        request,
        retained,
        registry,
        io,
        pool,
        done,
        job_timing,
        executor,
        reads: VecDeque::new(),
    }
    .dispatch_next();
    Ok(result)
}
impl GenerationExecution {
    fn dispatch_next(mut self) {
        match self
            .progress
            .next_step(&self.state, Some(&self.request.cancelled))
        {
            Ok(Some(step))
                if step.status == ChunkStatus::Empty
                    && !self.state.failed_loads.contains_key(&step.position.pack()) =>
            {
                self.dispatch_read(step);
            }
            Ok(Some(step)) => {
                let pool = self.pool.clone();
                // The request has its own response channel. Each unit returns
                // owned state/effects directly to the next continuation.
                drop(pool.submit(move || {
                    let (execution, result) = self.execute(step);
                    execution.integrate_step(result);
                }));
            }
            Ok(None) => self.queue_finish(None),
            Err(error) => self.queue_finish(Some(error)),
        }
    }
    /// Read ahead only inside the current EMPTY layer. IO runs on its existing
    /// actor; waiting yields the server executor rather than occupying a worker.
    /// Decode/publication still follow the layer's X-major/Z-minor order.
    fn dispatch_read(mut self, step: GenerationStep) {
        const READ_WINDOW: usize = 8;
        if self.reads.is_empty() {
            self.reads
                .push_back((step.position, self.io.load(step.position)));
        }
        let layer = self.progress.layer.as_ref().expect("EMPTY layer");
        for position in layer.positions.clone() {
            if self.reads.len() == READ_WINDOW {
                break;
            }
            if !self.state.holders.contains_key(&position.pack())
                && !self.state.failed_loads.contains_key(&position.pack())
                && !self.reads.iter().any(|(queued, _)| *queued == position)
            {
                self.reads.push_back((position, self.io.load(position)));
            }
        }
        let (position, read) = self.reads.pop_front().expect("current read");
        assert_eq!(position, step.position);
        let executor = self.executor.clone();
        executor.spawn(async move {
            let mut timing = self.progress.take_step_timing();
            let loaded = {
                let _wait =
                    profile::Span::new("chunk_read_wait", Some((self.request.id, position)));
                read.await
            };
            let pool = self.pool.clone();
            drop(pool.submit(move || {
                let _work =
                    profile::Span::new("generation_step", Some((self.request.id, position)))
                        .status(ChunkStatus::Empty);
                let result =
                    self.state
                        .integrate_chunk_read(position, loaded, &self.registry, &mut timing);
                self.integrate_step(GenerationStepResult {
                    timing,
                    effects: GenerationStepEffects::default(),
                    result,
                });
            }));
        });
    }
    fn execute(mut self, step: GenerationStep) -> (Self, GenerationStepResult) {
        let _work = profile::Span::new("generation_step", Some((self.request.id, step.position)))
            .status(step.status);
        let mut timing = self.progress.take_step_timing();
        let mut effects = GenerationStepEffects::default();
        let result =
            self.state
                .execute_step(step, &self.registry, &self.io, &mut effects, &mut timing);
        (
            self,
            GenerationStepResult {
                timing,
                effects,
                result,
            },
        )
    }
    fn integrate_step(mut self, result: GenerationStepResult) {
        self.progress.return_step_timing(result.timing);
        self.progress.append_effects(result.effects);
        match result.result {
            Ok(()) => self.dispatch_next(),
            Err(error) => self.queue_finish(Some(error)),
        }
    }
    fn queue_finish(mut self, failure: Option<anyhow::Error>) {
        if !self.reads.is_empty() {
            let executor = self.executor.clone();
            executor.spawn(async move {
                // Issued reads stay reachable until settled, including failure
                // cleanup. Never publish a later read ahead of the failed one.
                while let Some((_, read)) = self.reads.pop_front() {
                    let _ = read.await;
                }
                self.queue_finish(failure);
            });
            return;
        }
        let pool = self.pool.clone();
        drop(pool.submit(move || self.finish(failure)));
    }
    fn finish(self, failure: Option<anyhow::Error>) {
        let Self {
            mut state,
            mut progress,
            request,
            retained,
            registry,
            io,
            pool,
            done,
            job_timing,
            executor: _,
            reads: _,
        } = self;
        let result = match failure {
            Some(error) => Err(error),
            None => state.capture_generation(&mut progress, &registry),
        };
        let mut result = state.finish_light(result);
        drop(progress); // Keep save/cleanup outside the generation interval.
        let cleanup = if request.cancelled.load(Ordering::Relaxed) {
            state.save_proto_chunks(&registry, &io).and_then(|()| {
                if let Ok(output) = &result {
                    io.store(request.pos, Some(output.terrain.disk.write()?))
                        .wait()?;
                }
                Ok(())
            })
        } else if result.is_ok() {
            state.save_unclaimed_protos(&retained, &registry, &io)
        } else {
            Ok(())
        };
        if let Ok(output) = &mut result {
            output.statuses = state.completed_statuses();
        }
        // IO ownership must return before a ready completion can trigger close.
        drop((retained, registry, io, pool, job_timing));
        let _ = done.send(GenerationCompletion {
            state,
            result,
            request,
            cleanup,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_nbt::{Compression, NbtIo};
    use ferraloom_storage::{RegionFileVersion, SerializableChunkData};
    use std::sync::mpsc;

    fn bytes(disk: &SerializableChunkData) -> Vec<u8> {
        let mut bytes = Vec::new();
        NbtIo::write_compound(&mut bytes, Compression::None, &disk.write().unwrap()).unwrap();
        bytes
    }
    fn request(cancelled: Arc<AtomicBool>) -> super::super::GenerationRequest {
        let (reply, _answer) = oneshot::channel();
        super::super::GenerationRequest {
            id: 7,
            pos: ChunkPos::ZERO,
            status: ChunkStatus::Full,
            reply,
            cancelled,
        }
    }
    fn retained() -> BTreeSet<i64> {
        (-11..=11)
            .flat_map(|x| (-11..=11).map(move |z| ChunkPos::new(x, z).pack()))
            .collect()
    }
    #[test]
    fn step_resubmission_yields_the_worker_and_preserves_chunk_bytes() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _entered = runtime.enter();
        let (mut inline, registry) = super::super::relight_tests::fixture();
        let state = GenerationState::new(inline.context.clone(), inline.light_pool.clone());
        let registry = Arc::new(registry);
        let directory = tempfile::tempdir().unwrap();
        let mut reference_io = IoWorker::new(
            directory.path().join("inline"),
            RegionFileVersion::Deflate,
            false,
        )
        .unwrap();
        let expected = inline
            .generate(
                7,
                ChunkPos::ZERO,
                ChunkStatus::Full,
                &registry,
                &reference_io,
                None,
            )
            .unwrap();
        let mut io = Arc::new(
            IoWorker::new(
                directory.path().join("stepped"),
                RegionFileVersion::Deflate,
                false,
            )
            .unwrap(),
        );
        let pool = Arc::new(JobPool::new("step-dispatch-test", 1).unwrap());
        let (start, gate) = mpsc::channel();
        let initial = pool.submit(move || gate.recv().unwrap());
        let mut completion = dispatch_generation(
            state,
            request(Arc::new(AtomicBool::new(false))),
            retained(),
            registry,
            io.clone(),
            pool.clone(),
        )
        .unwrap();
        let (entered, entry) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let marker = pool.submit(move || {
            entered.send(()).unwrap();
            gate.recv().unwrap();
        });
        start.send(()).unwrap();
        initial.blocking_recv().unwrap();
        entry.recv().unwrap();
        // The first step resubmits behind this marker. A whole-request worker
        // would instead have settled FULL before this marker could start.
        assert!(matches!(
            completion.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        release.send(()).unwrap();
        marker.blocking_recv().unwrap();
        let actual = completion.blocking_recv().unwrap();
        actual.cleanup.unwrap();
        let output = actual.result.unwrap();
        assert_eq!(bytes(&output.terrain.disk), bytes(&expected.terrain.disk));
        assert_eq!(output.statuses, expected.statuses);
        assert_eq!(output.neighbors.len(), expected.neighbors.len());
        for ((pos, disk), (expected_pos, expected_disk)) in
            output.neighbors.iter().zip(&expected.neighbors)
        {
            assert_eq!(pos, expected_pos);
            assert_eq!(bytes(disk), bytes(expected_disk));
        }
        assert_eq!(output.skips, expected.skips);
        Arc::get_mut(&mut io).unwrap().shutdown().wait().unwrap();
        reference_io.shutdown().wait().unwrap();
    }
    #[test]
    fn cancellation_finishes_the_started_layer_and_returns_the_owned_cache() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _entered = runtime.enter();
        let (state, registry) = super::super::relight_tests::fixture();
        let directory = tempfile::tempdir().unwrap();
        let mut io =
            Arc::new(IoWorker::new(directory.path(), RegionFileVersion::Deflate, false).unwrap());
        let pool = Arc::new(JobPool::new("step-cancel-test", 1).unwrap());
        let (start, gate) = mpsc::channel();
        let initial = pool.submit(move || gate.recv().unwrap());
        let cancelled = Arc::new(AtomicBool::new(false));
        let completion = dispatch_generation(
            state,
            request(cancelled.clone()),
            retained(),
            Arc::new(registry),
            io.clone(),
            pool.clone(),
        )
        .unwrap();
        let (entered, entry) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let marker = pool.submit(move || {
            entered.send(()).unwrap();
            gate.recv().unwrap();
        });
        start.send(()).unwrap();
        initial.blocking_recv().unwrap();
        entry.recv().unwrap();
        cancelled.store(true, Ordering::Relaxed);
        release.send(()).unwrap();
        marker.blocking_recv().unwrap();
        let completion = completion.blocking_recv().unwrap();
        let Err(error) = completion.result else {
            panic!("cancelled request unexpectedly succeeded");
        };
        assert!(error.to_string().contains("Cancelled"));
        completion.cleanup.unwrap();
        assert_eq!(completion.state.holders.len(), 9);
        assert!(
            completion
                .state
                .holders
                .values()
                .all(|holder| holder.completed == ChunkStatus::Empty)
        );
        Arc::get_mut(&mut io).unwrap().shutdown().wait().unwrap();
    }
}
