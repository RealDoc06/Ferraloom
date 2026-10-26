//! ChunkGenerationTask's resumable serial dispatch, separate from owned work.
mod batch;
#[cfg(test)]
mod native_tests;
use batch::OrderedBatch;

use super::{GenerationCompletion, GenerationRevision, GenerationState, profile};
use crate::{jobs::JobPool, world::RegistryAdapter};
use anyhow::{Context, Result};
use ferraloom_game::level::generation::{
    ChunkGenerationTask, ChunkStatus, ChunkStatusTask, GenerationChunkHolder, PreparedCenterStep,
    ProtoChunk,
};
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
    fn prepare_center_inputs(
        &mut self,
        requested: GenerationStep,
    ) -> (BTreeSet<i64>, Result<PreparedCenterStep>) {
        let mut touched = BTreeSet::new();
        let result = (|| {
            let holder = self
                .holders
                .get(&requested.position.pack())
                .context("generation holder missing")?;
            if let Some(error) = &holder.failure {
                anyhow::bail!("generation dependency failed: {error:?}");
            }
            let step = holder.step_to(requested.status).clone();
            self.invalidate_step_terrain(requested.position, &step);
            let neighborhood = self.step_neighborhood(requested.position, &step)?;
            let mut owned = neighborhood
                .iter()
                .map(|position| {
                    self.holders
                        .remove(&position.pack())
                        .expect("checked dependency")
                })
                .collect::<Vec<_>>();
            let mut regions = owned
                .iter_mut()
                .map(|holder| std::mem::replace(&mut holder.chunk.region, self.placeholder.clone()))
                .collect::<Vec<_>>();
            let prepared = self
                .context
                .prepare_center_step(
                    &mut regions,
                    [requested.position.x, requested.position.z],
                    &step,
                )
                .map_err(anyhow::Error::new);
            for ((position, mut holder), region) in neighborhood.into_iter().zip(owned).zip(regions)
            {
                holder.chunk.region = region;
                self.holders.insert(position.pack(), holder);
                touched.insert(position.pack());
            }
            prepared
        })();
        (touched, result)
    }
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

struct CenterWork {
    step: GenerationStep,
    holder: GenerationChunkHolder<ProtoChunk>,
    touched: BTreeSet<i64>,
    input: Result<PreparedCenterStep>,
}
struct CenterOutput {
    step: GenerationStep,
    holder: GenerationChunkHolder<ProtoChunk>,
    touched: BTreeSet<i64>,
    result: Result<()>,
    #[cfg(test)]
    interval: (std::time::Instant, std::time::Instant),
}
impl CenterWork {
    fn execute(
        mut self,
        context: &ferraloom_game::level::generation::WorldGenContext,
        request: u64,
    ) -> CenterOutput {
        let mut timing = profile::Span::new("generation_step", Some((request, self.step.position)))
            .status(self.step.status);
        #[cfg(test)]
        let started = std::time::Instant::now();
        let result = timing
            .measure("apply", || {
                self.input?
                    .apply(context, &mut self.holder.chunk.region)
                    .map_err(anyhow::Error::new)
            })
            .with_context(|| {
                format!(
                    "generation chunk={:?} status={:?}",
                    self.step.position, self.step.status
                )
            });
        CenterOutput {
            step: self.step,
            holder: self.holder,
            touched: self.touched,
            result,
            #[cfg(test)]
            interval: (started, std::time::Instant::now()),
        }
    }
}

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
            Ok(Some(step)) if self.center_is_parallel(step) => {
                let pool = self.pool.clone();
                drop(pool.submit(move || self.dispatch_center_batch(step)));
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
    /// actor; completion queues work without relying on the parked server executor.
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
        let mut timing = self.progress.take_step_timing();
        let wait = profile::Span::new("chunk_read_wait", Some((self.request.id, position)));
        read.on_complete(move |loaded| {
            drop(wait);
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

    fn center_is_parallel(&self, step: GenerationStep) -> bool {
        if self.pool.parallelism() < 2 {
            return false;
        }
        let Some(holder) = self.state.holders.get(&step.position.pack()) else {
            return false;
        };
        match holder.step_to(step.status).task {
            ChunkStatusTask::GenerateBiomes => true,
            ChunkStatusTask::BuildTerrain => matches!(
                self.state.context.generator.as_ref(),
                ferraloom_worldgen::generator::ChunkGenerator::Noise(_)
            ),
            _ => false,
        }
    }

    fn dispatch_center_batch(mut self, first: GenerationStep) {
        let mut steps = vec![first];
        // Stay within this layer. No later status or synchronous feature visit
        // can observe in-flight centers or mutable structure-piece geometry.
        while steps.len() < self.pool.parallelism() {
            let position = self
                .progress
                .layer
                .as_ref()
                .expect("center layer")
                .positions
                .as_slice()
                .first()
                .copied();
            let Some(position) = position else { break };
            let step = GenerationStep {
                position,
                status: first.status,
            };
            let completed = self
                .state
                .holders
                .get(&position.pack())
                .is_some_and(|holder| holder.completed >= first.status);
            if !completed && !self.center_is_parallel(step) {
                break;
            }
            self.progress.layer.as_mut().unwrap().positions.next();
            if !completed {
                steps.push(step);
            }
        }
        // Freeze all read inputs before removing any writable center. Terrain
        // jobs can share immutable geometry but never another center's payload.
        let prepared = steps
            .iter()
            .map(|&step| self.state.prepare_center_inputs(step))
            .collect::<Vec<_>>();
        let mut work = Vec::new();
        for (step, (touched, input)) in steps.into_iter().zip(prepared) {
            let holder = self
                .state
                .holders
                .remove(&step.position.pack())
                .expect("ready center is resident");
            work.push(CenterWork {
                step,
                holder,
                touched,
                input,
            });
        }
        let context = self.state.context.clone();
        let request = self.request.id;
        let pool = self.pool.clone();
        let batch = OrderedBatch::new(work.len(), self);
        for (ordinal, work) in work.into_iter().enumerate() {
            let batch = batch.clone();
            let context = context.clone();
            drop(pool.submit(move || {
                let output = work.execute(&context, request);
                if let Some((execution, outputs)) = batch.complete(ordinal, output) {
                    execution.integrate_centers(outputs);
                }
            }));
        }
        if let Some((execution, outputs)) = batch.release() {
            execution.integrate_centers(outputs);
        }
    }

    fn integrate_centers(mut self, outputs: Vec<CenterOutput>) {
        let mut first_failure = None;
        for output in outputs {
            #[cfg(test)]
            self.state.center_intervals.push((
                output.step.status,
                output.interval.0,
                output.interval.1,
            ));
            let CenterOutput {
                step,
                mut holder,
                touched,
                result,
                #[cfg(test)]
                    interval: _,
            } = output;
            self.progress.effects.touched.extend(touched);
            match result {
                Ok(()) => holder.complete(step.status),
                Err(error) => {
                    holder.failure =
                        Some(ferraloom_game::level::generation::GenerationError::Failed {
                            position: step.position,
                            status: step.status,
                            message: error.to_string(),
                        });
                    if first_failure.is_none() {
                        first_failure = Some(error);
                    }
                }
            }
            self.state.holders.insert(step.position.pack(), holder);
        }
        match first_failure {
            Some(error) => self.queue_finish(Some(error)),
            None => self.dispatch_next(),
        }
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
        if let Some((_, read)) = self.reads.pop_front() {
            read.on_complete(move |_| {
                // Schedule even ready completions rather than recursively
                // executing cleanup on the IO actor or registering thread.
                let pool = self.pool.clone();
                drop(pool.submit(move || self.queue_finish(failure)));
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
