use super::*;
use ferraloom_game::{
    Behaviors,
    level::{LevelContext, Region, acquisition::ChunkKind},
};
use std::time::Duration;

struct InlineProbe {
    owner: PhaseOwner,
    forwarded: Cell<u32>,
}
impl AcquireDriver for InlineProbe {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn is_owner_thread(&self) -> bool {
        self.owner.is_current()
    }
    fn visible_ticket_level(&self, _: ChunkPos) -> Option<i32> {
        Some(36)
    }
    fn add_unknown_ticket(&self, _: ChunkPos, _: ChunkStatus) -> Result<(), LevelError> {
        Ok(())
    }
    fn run_distance_manager_updates(&self, _: &Level) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn request_status(
        &self,
        _: &Level,
        pos: ChunkPos,
        status: ChunkStatus,
    ) -> Result<AcquireHandle, LevelError> {
        Ok(AcquireHandle {
            pos,
            status,
            request_id: 1,
            holder_epoch: 1,
        })
    }
    fn acquisition_result(&self, handle: AcquireHandle) -> Result<AcquireResult, LevelError> {
        Ok(AcquireResult::Complete(Some(ChunkReference {
            pos: handle.pos,
            generation: 1,
            kind: ChunkKind::Proto,
        })))
    }
    fn poll_acquisition_step(&self, _: &Level) -> Result<AcquireProgress, LevelError> {
        panic!("completed acquisition must return inline")
    }
    fn wait_for_progress(&self) -> Result<(), LevelError> {
        panic!("completed acquisition must not wait")
    }
    fn chunk_if_present(&self, _: ChunkPos, _: ChunkStatus) -> Option<ChunkReference> {
        None
    }
    fn chunk_if_present_unchecked(&self, _: ChunkPos, _: ChunkStatus) -> Option<ChunkReference> {
        None
    }
    fn acquire_on_owner(
        &self,
        pos: ChunkPos,
        status: ChunkStatus,
        _: bool,
    ) -> Result<Option<ChunkReference>, LevelError> {
        self.forwarded.set(self.forwarded.get() + 1);
        Err(LevelError::ChunkAcquisition(
            pos,
            status,
            AcquireFailure::DriverUnavailable,
        ))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn acquisition_stays_inline_when_the_phase_task_moves_between_workers() {
    let (blocked, wait_blocked) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let owner = PhaseOwner::current();
        tokio::task::yield_now().await;
        let before = thread::current().id();
        let (resume, wait_resume) = tokio::sync::oneshot::channel();
        let (release, wait_release) = mpsc::channel();
        // A locally spawned LIFO task occupies this executor worker while the
        // server is suspended. Waking the server from outside that worker makes
        // its continuation run on the other worker, with the same Tokio task ID.
        let blocker = tokio::spawn(async move {
            let worker = thread::current().id();
            let _ = blocked.send((before, worker, resume, release));
            wait_release.recv_timeout(Duration::from_secs(10)).unwrap();
        });
        wait_resume.await.unwrap();
        let after = thread::current().id();
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
        level
            .install_acquire_driver(
                Box::new(InlineProbe {
                    owner,
                    forwarded: Cell::new(0),
                }),
                0..16,
            )
            .unwrap();
        let acquired = level.get_chunk(ChunkPos::ZERO, ChunkStatus::Biomes, true);
        let forwarded = level
            .acquire_driver()
            .unwrap()
            .as_any()
            .downcast_ref::<InlineProbe>()
            .unwrap()
            .forwarded
            .get();
        (after, acquired, forwarded, blocker)
    });
    let (before, blocked_worker, resume, release) =
        tokio::time::timeout(Duration::from_secs(10), wait_blocked)
            .await
            .unwrap()
            .unwrap();
    resume.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(10), server).await;
    // Release the occupied worker before asserting, including on a failed test.
    let _ = release.send(());
    let (after, acquired, forwarded, blocker) = result.unwrap().unwrap();
    blocker.await.unwrap();
    assert_eq!(before, blocked_worker);
    assert_ne!(before, after);
    assert_eq!(forwarded, 0);
    assert_eq!(acquired.unwrap().unwrap().kind, ChunkKind::Proto);
}
