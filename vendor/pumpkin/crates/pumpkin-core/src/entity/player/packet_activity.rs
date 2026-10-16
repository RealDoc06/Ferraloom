//! Java input completion and disconnect ownership.
//!
//! A lease owns work, never a mutex guard. Only explicit mutation spawns and
//! server descendants inherit the current lease; ordinary client output/chunk
//! tasks retain their existing tracker and must not hold input behind client ACKs.

use std::{
    pin::Pin,
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    time::Instant,
};

use crossbeam::queue::{ArrayQueue, SegQueue};
use pumpkin_protocol::RawPacket;
use rayon::prelude::*;
use tokio::sync::Notify;

use super::Player;
use crate::server::Server;

const MAX_QUEUED_PACKETS: usize = 4096;
const MAX_QUEUED_BYTES: usize = 8 * 1024 * 1024;
const PACKETS_PER_EPOCH: usize = 64;
const READY_PLAYERS: usize = 4096;
const PLAYERS_PER_SLICE: usize = 64;

#[derive(Default)]
struct State {
    sealed: bool,
    active: usize,
    dispatch_blockers: usize,
    queued_bytes: usize,
    remaining: usize,
    consuming: bool,
}

#[derive(Default)]
pub(crate) struct PacketActivity {
    state: Mutex<State>,
    idle: Notify,
    player: OnceLock<Weak<Player>>,
    runner: OnceLock<Arc<PacketRunner>>,
    scheduled: AtomicBool,
    runnable: AtomicBool,
    parked: AtomicBool,
    work: Mutex<Vec<Arc<PacketTask>>>,
    world_waiters: Mutex<Vec<Waker>>,
}

/// The ticker's bounded ready lane. One queue entry per session; overflow stays
/// marked on the registered session and is refilled without dropping ownership.
pub(crate) struct PacketRunner {
    ready: ArrayQueue<Arc<PacketActivity>>,
    sessions: Mutex<Vec<Weak<PacketActivity>>>,
    overflow: AtomicBool,
    thread: OnceLock<std::thread::Thread>,
    pub(crate) phase: Mutex<()>,
    stopping: AtomicBool,
    finished: AtomicBool,
}

impl Default for PacketRunner {
    fn default() -> Self {
        Self {
            ready: ArrayQueue::new(READY_PLAYERS),
            sessions: Mutex::default(),
            overflow: AtomicBool::new(false),
            thread: OnceLock::new(),
            phase: Mutex::new(()),
            stopping: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        }
    }
}

struct PacketTask {
    activity: Arc<PacketActivity>,
    ready: AtomicBool,
    explicit_world_stages: bool,
    future: Mutex<Option<Pin<Box<dyn Future<Output = ()> + Send>>>>,
    cancelled: AtomicBool,
    waiter: Mutex<Option<tokio::task::AbortHandle>>,
}

/// The runtime only awaits the result. Aborting that waiter cannot drop the
/// owned mutator. The runner can cancel a readiness-blocked operation on close.
pub(crate) struct PacketCompletion<T> {
    receive: tokio::sync::oneshot::Receiver<T>,
    task: Arc<PacketTask>,
}

impl<T: Send + 'static> PacketCompletion<T> {
    pub(crate) fn spawn_on(
        self,
        tracker: &tokio_util::task::TaskTracker,
        runtime: &tokio::runtime::Handle,
    ) -> tokio::task::JoinHandle<T> {
        let task = self.task.clone();
        let handle = tracker.spawn_on(self, runtime);
        *task
            .waiter
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle.abort_handle());
        if task.cancelled.load(Ordering::Acquire) {
            handle.abort();
        }
        handle
    }
}

impl<T> Future for PacketCompletion<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        match Pin::new(&mut self.receive).poll(cx) {
            Poll::Ready(Ok(value)) => Poll::Ready(value),
            // Cancellation aborts this runtime waiter; it never manufactures an
            // arbitrary F::Output or exposes a panic as normal disconnect work.
            Poll::Ready(Err(_)) | Poll::Pending => Poll::Pending,
        }
    }
}

impl PacketTask {
    /// Only between polls, after close and failed readiness admission. Dropping
    /// this future cannot release leases retained by independently owned children.
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(waiter) = self
            .waiter
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            waiter.abort();
        }
        let future = self
            .future
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        drop(future);
    }
}

impl Wake for PacketTask {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.ready.store(true, Ordering::Release);
        self.activity.schedule();
    }
}

impl PacketRunner {
    pub(crate) fn attach(&self) {
        let _ = self.thread.set(std::thread::current());
    }

    fn wake(&self) {
        if let Some(thread) = self.thread.get() {
            thread.unpark();
        }
    }

    /// STOP stops admissions/ticks, not the executor needed by session drain.
    pub(crate) fn begin_drain(&self) {
        self.stopping.store(true, Ordering::Release);
        for activity in self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter_map(Weak::upgrade)
        {
            activity.seal();
        }
        self.wake();
    }

    pub(crate) fn finish_drain(&self) {
        self.finished.store(true, Ordering::Release);
        while let Some(activity) = self.ready.pop() {
            activity.scheduled.store(false, Ordering::Release);
        }
        self.wake();
    }

    pub(crate) fn is_draining(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    /// Readiness changes and new epochs retry parked world admissions. Pending
    /// futures are still polled only after their own waker has marked them ready.
    pub(crate) fn retry_worlds(&self) {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        sessions.retain(|weak| {
            let Some(activity) = weak.upgrade() else {
                return false;
            };
            for waker in std::mem::take(
                &mut *activity
                    .world_waiters
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            ) {
                waker.wake();
            }
            if activity.runnable.load(Ordering::Acquire)
                || activity.parked.swap(false, Ordering::AcqRel)
            {
                activity.schedule();
            }
            true
        });
        self.wake();
    }

    /// A finite wave joins the existing Rayon pool before returning to the
    /// gameplay deadline. Callbacks must return; arbitrary plugin blocking or
    /// raw workers cannot be made preemptible by this native packet lane.
    pub(crate) fn run_ready(&self, server: &Arc<Server>, deadline: Option<Instant>) -> bool {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return false;
        }
        let _phase = self
            .phase
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.overflow.swap(false, Ordering::AcqRel) {
            self.retry_worlds();
        }
        let mut wave = Vec::with_capacity(PLAYERS_PER_SLICE);
        for _ in 0..PLAYERS_PER_SLICE {
            let Some(activity) = self.ready.pop() else {
                break;
            };
            wave.push(activity);
        }
        let worked = !wave.is_empty();
        wave.into_par_iter().for_each(|activity| {
            let _runtime = server.runtime.enter();
            // Keep scheduled set until this poll finishes. Wakes racing it set
            // runnable; the final swap below cannot lose them.
            activity.runnable.store(false, Ordering::Release);
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                activity.runnable.store(true, Ordering::Release);
            } else {
                activity.run_slice();
            }
            activity.scheduled.store(false, Ordering::Release);
            if activity.runnable.load(Ordering::Acquire) {
                activity.schedule();
            }
        });
        worked
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AdmissionError {
    Closed,
    Full,
}

/// Clones retain the same accepted operation, including after admission seals.
#[derive(Clone)]
pub(crate) struct ActivityLease(Arc<Work>);

struct Work {
    activity: Arc<PacketActivity>,
    blocks_dispatch: bool,
    // A command targeting another player owns both players until its child ends.
    parent: Option<ActivityLease>,
}

tokio::task_local! {
    static CURRENT_ACTIVITY: ActivityLease;
}

impl PacketActivity {
    pub(crate) fn is_bound(&self) -> bool {
        self.runner.get().is_some()
    }

    fn is_stopping(&self) -> bool {
        crate::STOP_INTERRUPT.is_cancelled()
            || self.runner.get().is_some_and(|runner| runner.is_draining())
    }

    pub(crate) fn bind(self: &Arc<Self>, player: &Arc<Player>, runner: &Arc<PacketRunner>) {
        if self.player.set(Arc::downgrade(player)).is_ok() {
            let _ = self.runner.set(runner.clone());
            runner
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(Arc::downgrade(self));
            if self.is_stopping() {
                self.seal();
            }
        }
    }

    pub(crate) fn schedule(self: &Arc<Self>) {
        self.runnable.store(true, Ordering::Release);
        let Some(runner) = self.runner.get() else {
            return;
        };
        if runner.is_finished() {
            return;
        }
        if !self.scheduled.swap(true, Ordering::AcqRel) && runner.ready.push(self.clone()).is_err()
        {
            self.scheduled.store(false, Ordering::Release);
            runner.overflow.store(true, Ordering::Release);
        }
        runner.wake();
    }

    pub(crate) fn park(&self) {
        self.parked.store(true, Ordering::Release);
    }

    fn park_world(&self, waker: Waker) {
        let mut waiters = self
            .world_waiters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !waiters.iter().any(|old| old.will_wake(&waker)) {
            waiters.push(waker);
        }
    }

    pub(crate) fn begin_epoch(&self) {
        let mut state = self.state();
        if state.dispatch_blockers == 0 {
            state.remaining = PACKETS_PER_EPOCH;
        }
    }

    pub(crate) fn begin_consumer(self: &Arc<Self>) -> Option<Consumer> {
        let mut state = self.state();
        if state.sealed || state.consuming || self.is_stopping() {
            return None;
        }
        state.consuming = true;
        let lease = self.lease(&mut state, false, None);
        Some(Consumer {
            activity: self.clone(),
            _lease: lease,
        })
    }

    fn run_slice(self: &Arc<Self>) {
        let Some(player) = self.player.get().and_then(Weak::upgrade) else {
            return;
        };
        let task = self
            .work
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|task| task.ready.load(Ordering::Acquire))
            .cloned();
        if let Some(task) = &task {
            task.ready.store(false, Ordering::Release);
        }
        let world = player.world();
        let _callbacks = if task
            .as_ref()
            .is_some_and(|task| !task.explicit_world_stages)
        {
            let Some(callbacks) =
                world.enter_packet_callbacks(Some(player.living_entity.entity.chunk_pos.load()))
            else {
                if self.state().sealed {
                    if let Some(task) = task {
                        task.cancel();
                        self.retire_task(&task);
                        self.schedule();
                    }
                    return;
                }
                if let Some(task) = task {
                    self.park_world(Waker::from(task));
                }
                // Other ready children may reconcile readiness or finish a
                // transfer. A parked parent must not hide those continuations.
                self.schedule();
                return;
            };
            Some(callbacks)
        } else {
            None
        };
        if let Some(task) = task {
            self.poll_task(&task, &player);
        } else {
            player.run_packet_slice();
        }
        let ready = self
            .work
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .any(|task| task.ready.load(Ordering::Acquire));
        let state = self.state();
        let input = !state.sealed
            && !self.is_stopping()
            && state.remaining > 0
            && state.dispatch_blockers == 0
            && !player.inbound_packets.is_empty();
        drop(state);
        if ready || (input && !self.parked.load(Ordering::Acquire)) {
            self.schedule();
        }
    }

    fn poll_task(&self, task: &Arc<PacketTask>, player: &Player) {
        let waker = Waker::from(task.clone());
        let mut context = Context::from_waker(&waker);
        let future = task
            .future
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let Some(mut future) = future else { return };
        // These futures previously had Tokio's panic boundary. Keep the drain
        // executor alive if one unwinds; independently owned children still run.
        let polled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            future.as_mut().poll(&mut context)
        }));
        if matches!(polled, Ok(Poll::Pending)) {
            *task
                .future
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(future);
        } else {
            if polled.is_err() {
                tracing::error!(
                    "Java packet continuation panicked for {}",
                    player.gameprofile.id
                );
                crate::SERVER_EXIT_CODE.store(1, Ordering::Release);
                task.cancel();
                if let Some(client) = player.client.java() {
                    client.close();
                }
            }
            // Completion never calls Player::tick. A child's retained lease
            // still blocks the FIFO and final saves after this future drops.
            drop(future);
            self.retire_task(task);
        }
    }

    fn retire_task(&self, task: &Arc<PacketTask>) {
        let waker = Waker::from(task.clone());
        self.world_waiters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|waiting| !waiting.will_wake(&waker));
        self.work
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|owned| !Arc::ptr_eq(owned, task));
    }

    /// Register the owned future before spawning its result waiter. Aborting
    /// that waiter cannot cancel a mutation or release its drain lease early.
    fn submit<F>(
        self: &Arc<Self>,
        lease: ActivityLease,
        work: F,
        explicit_world_stages: bool,
    ) -> PacketCompletion<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        let future = lease.scope(async move {
            let _ = send.send(work.await);
        });
        let task = Arc::new(PacketTask {
            activity: self.clone(),
            ready: AtomicBool::new(true),
            explicit_world_stages,
            future: Mutex::new(Some(Box::pin(future))),
            cancelled: AtomicBool::new(false),
            waiter: Mutex::new(None),
        });
        let completion = PacketCompletion {
            receive,
            task: task.clone(),
        };
        self.work
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(task);
        self.schedule();
        completion
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn lease(
        self: &Arc<Self>,
        state: &mut State,
        blocks_dispatch: bool,
        parent: Option<ActivityLease>,
    ) -> ActivityLease {
        state.active += 1;
        state.dispatch_blockers += usize::from(blocks_dispatch);
        ActivityLease(Arc::new(Work {
            activity: self.clone(),
            blocks_dispatch,
            parent,
        }))
    }

    /// The Java reader uses this before publishing a complete frame. Bound both
    /// tiny-packet overhead and payload retention while an earlier handler waits.
    pub(crate) fn enqueue(
        &self,
        queue: &SegQueue<RawPacket>,
        packet: RawPacket,
    ) -> Result<(), AdmissionError> {
        let mut state = self.state();
        if state.sealed || self.is_stopping() {
            return Err(AdmissionError::Closed);
        }
        if queue.len() >= MAX_QUEUED_PACKETS
            || packet.payload.len() > MAX_QUEUED_BYTES.saturating_sub(state.queued_bytes)
        {
            return Err(AdmissionError::Full);
        }
        state.queued_bytes += packet.payload.len();
        queue.push(packet);
        Ok(())
    }

    /// Reserve before dequeue and before any packet callback. Busy means return
    /// to the world tick, never wait on a Tokio task from the Rayon barrier.
    pub(crate) fn next_packet(
        self: &Arc<Self>,
        queue: &SegQueue<RawPacket>,
    ) -> Option<(ActivityLease, RawPacket)> {
        let mut state = self.state();
        if state.sealed
            || self.is_stopping()
            || state.dispatch_blockers != 0
            || state.remaining == 0
        {
            return None;
        }
        let packet = queue.pop()?;
        let lease = self.lease(&mut state, true, None);
        state.remaining -= 1;
        state.queued_bytes = state.queued_bytes.saturating_sub(packet.payload.len());
        Some((lease, packet))
    }

    /// Tick work and generic task registration may proceed while input is busy.
    /// Close waits for their synchronous work/registration before draining tasks
    /// and saving. These leases do not propagate into ordinary output tasks.
    pub(crate) fn enter_non_packet(self: &Arc<Self>) -> Option<ActivityLease> {
        let mut state = self.state();
        if state.sealed || self.is_stopping() {
            return None;
        }
        Some(self.lease(&mut state, false, None))
    }

    /// Explicit mutation descendants may continue after seal only when their
    /// ancestry already owns this player. An unrelated late root is refused.
    /// External operations on an open player join its activity; this does not
    /// define a total order between commands sent by different players.
    pub(crate) fn enter_mutation(self: &Arc<Self>) -> Option<ActivityLease> {
        let parent = ActivityLease::current();
        if let Some(parent) = &parent
            && parent.contains(self)
        {
            return Some(parent.clone());
        }
        let mut state = self.state();
        if state.sealed || self.is_stopping() {
            return None;
        }
        Some(self.lease(&mut state, true, parent))
    }

    pub(crate) fn seal(self: &Arc<Self>) {
        let mut state = self.state();
        if state.sealed {
            return;
        }
        state.sealed = true;
        if let Some(player) = self.player.get().and_then(Weak::upgrade) {
            while player.inbound_packets.pop().is_some() {}
        }
        state.queued_bytes = 0;
        drop(state);
        // One final poll can observe close, or terminate if world admission is
        // permanently unavailable. Runnable descendants retain separate leases.
        for task in self
            .work
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
        {
            task.ready.store(true, Ordering::Release);
        }
        self.schedule();
    }

    /// Called by the outer session lifecycle, never by work holding a lease.
    pub(crate) async fn drain(self: &Arc<Self>) {
        self.seal();
        self.wait_idle().await;
    }

    async fn wait_idle(&self) {
        loop {
            let notified = self.idle.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.state().active == 0 {
                return;
            }
            notified.await;
        }
    }
}

impl ActivityLease {
    pub(crate) fn is_sealed(&self) -> bool {
        self.0.activity.state().sealed
    }
    pub(crate) fn park_world(&self, waker: &Waker) {
        self.0.activity.park_world(waker.clone());
    }
    pub(crate) fn current() -> Option<Self> {
        CURRENT_ACTIVITY.try_with(Clone::clone).ok()
    }

    fn contains(&self, activity: &Arc<PacketActivity>) -> bool {
        let mut current = Some(self);
        while let Some(lease) = current {
            if Arc::ptr_eq(&lease.0.activity, activity) {
                return true;
            }
            current = lease.0.parent.as_ref();
        }
        false
    }

    pub(crate) fn sync_scope<R>(self, work: impl FnOnce() -> R) -> R {
        CURRENT_ACTIVITY.sync_scope(self, work)
    }

    pub(crate) async fn scope<F: Future>(self, work: F) -> F::Output {
        CURRENT_ACTIVITY.scope(self, work).await
    }

    pub(crate) fn submit<F>(self, work: F) -> PacketCompletion<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.0.activity.clone().submit(self, work, false)
    }

    /// Transfer/respawn stages and chunk reconciliation coordinate world
    /// lifetime explicitly, with no callback guard around reconcile waits.
    pub(crate) fn submit_world_stages<F>(self, work: F) -> PacketCompletion<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.0.activity.clone().submit(self, work, true)
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let mut state = self.activity.state();
        state.active -= 1;
        state.dispatch_blockers -= usize::from(self.blocks_dispatch);
        let idle = state.active == 0;
        let ready = self.blocks_dispatch
            && state.dispatch_blockers == 0
            && !state.sealed
            && state.remaining > 0;
        drop(state);
        if idle {
            self.activity.idle.notify_waiters();
        }
        if ready {
            self.activity.schedule();
        }
    }
}

pub(crate) struct Consumer {
    activity: Arc<PacketActivity>,
    _lease: ActivityLease,
}

impl Drop for Consumer {
    fn drop(&mut self) {
        self.activity.state().consuming = false;
    }
}

#[cfg(test)]
#[path = "packet_tests.rs"]
mod tests;
