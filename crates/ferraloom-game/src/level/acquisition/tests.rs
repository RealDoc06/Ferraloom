use super::*;
use crate::level::tickets::{TicketStorage, TicketTracker, TicketType};
use std::{
    cell::{OnceCell, RefCell},
    collections::VecDeque,
};

const POS: ChunkPos = ChunkPos::new(0, 0);

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event {
    Ticket(ChunkPos, ChunkStatus),
    Distance,
    Request(ChunkPos, ChunkStatus),
    Poll,
    Wait,
    Forward,
    MainTask(AcquireTaskKind),
}

struct Driver {
    owner: Cell<bool>,
    level: Cell<Option<i32>>,
    promote: Cell<bool>,
    tickets: RefCell<TicketStorage>,
    events: RefCell<Vec<Event>>,
    requests: Cell<u64>,
    remaining: Cell<usize>,
    steps: RefCell<VecDeque<AcquireProgress>>,
    result: Cell<bool>,
    failure: RefCell<Option<LevelError>>,
    kind: Cell<ChunkKind>,
    present: Cell<Option<ChunkReference>>,
    status_allowed: Cell<bool>,
    prepared: RefCell<Option<crate::level::chunk::LevelChunk>>,
    published: OnceCell<crate::level::chunk::LevelChunk>,
    main_tasks: bool,
    main_task_runs: Vec<AcquireTaskKind>,
}

impl Driver {
    fn new(level: Option<i32>) -> Self {
        Self {
            owner: Cell::new(true),
            level: Cell::new(level),
            promote: Cell::new(true),
            tickets: RefCell::new(TicketStorage::default()),
            events: RefCell::new(Vec::new()),
            requests: Cell::new(0),
            remaining: Cell::new(0),
            steps: RefCell::new(VecDeque::new()),
            result: Cell::new(true),
            failure: RefCell::new(None),
            kind: Cell::new(ChunkKind::Full),
            present: Cell::new(None),
            status_allowed: Cell::new(true),
            prepared: RefCell::new(None),
            published: OnceCell::new(),
            main_tasks: false,
            main_task_runs: Vec::new(),
        }
    }

    fn reference(&self, pos: ChunkPos) -> ChunkReference {
        ChunkReference {
            pos,
            generation: 1,
            kind: self.kind.get(),
        }
    }
}

impl AcquireDriver for Driver {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn is_owner_thread(&self) -> bool {
        self.owner.get()
    }
    fn visible_ticket_level(&self, _: ChunkPos) -> Option<i32> {
        self.level.get()
    }
    fn add_unknown_ticket(&self, pos: ChunkPos, status: ChunkStatus) -> Result<(), LevelError> {
        self.events.borrow_mut().push(Event::Ticket(pos, status));
        self.tickets.borrow_mut().add_unknown(pos, status);
        Ok(())
    }
    fn run_distance_manager_updates(&self, _: &crate::level::Level) -> Result<bool, LevelError> {
        self.events.borrow_mut().push(Event::Distance);
        if self.promote.get() {
            self.level.set(self.tickets.borrow().level(POS));
            Ok(true)
        } else {
            Ok(false)
        }
    }
    fn request_status(
        &self,
        _: &crate::level::Level,
        pos: ChunkPos,
        status: ChunkStatus,
    ) -> Result<AcquireHandle, LevelError> {
        self.events.borrow_mut().push(Event::Request(pos, status));
        self.requests.set(self.requests.get() + 1);
        Ok(AcquireHandle {
            pos,
            status,
            request_id: self.requests.get(),
            holder_epoch: 1,
        })
    }
    fn acquisition_result(&self, handle: AcquireHandle) -> Result<AcquireResult, LevelError> {
        if let Some(error) = self.failure.borrow().clone() {
            return Err(error);
        }
        Ok(if self.remaining.get() == 0 {
            AcquireResult::Complete(self.result.get().then(|| self.reference(handle.pos)))
        } else {
            AcquireResult::Pending
        })
    }
    fn poll_acquisition_step(
        &self,
        _: &crate::level::Level,
    ) -> Result<AcquireProgress, LevelError> {
        self.events.borrow_mut().push(Event::Poll);
        let step = self
            .steps
            .borrow_mut()
            .pop_front()
            .unwrap_or(AcquireProgress::Task);
        if step != AcquireProgress::Idle {
            self.remaining.set(self.remaining.get().saturating_sub(1));
            if self.remaining.get() == 0
                && let Some(chunk) = self.prepared.borrow_mut().take()
            {
                assert!(self.published.set(chunk).is_ok());
            }
        }
        Ok(step)
    }
    fn wait_for_progress(&self) -> Result<(), LevelError> {
        self.events.borrow_mut().push(Event::Wait);
        self.remaining.set(0);
        Ok(())
    }
    fn main_thread_acquisition_task(
        &self,
        _: &Level,
        kind: AcquireTaskKind,
    ) -> Result<Option<Box<dyn MainThreadAcquireTask>>, LevelError> {
        Ok(self
            .main_tasks
            .then(|| Box::new(MainTask(kind)) as Box<dyn MainThreadAcquireTask>))
    }
    fn chunk_if_present_unchecked(&self, _: ChunkPos, _: ChunkStatus) -> Option<ChunkReference> {
        self.present.get()
    }
    fn chunk_if_present(&self, pos: ChunkPos, status: ChunkStatus) -> Option<ChunkReference> {
        self.status_allowed
            .get()
            .then(|| self.chunk_if_present_unchecked(pos, status))
            .flatten()
    }
    fn acquire_on_owner(
        &self,
        _: ChunkPos,
        _: ChunkStatus,
        _: bool,
    ) -> Result<Option<ChunkReference>, LevelError> {
        self.events.borrow_mut().push(Event::Forward);
        Ok(self.present.get())
    }
}

struct MainTask(AcquireTaskKind);
impl MainThreadAcquireTask for MainTask {
    fn run(self: Box<Self>, level: &mut Level) -> Result<AcquireProgress, LevelError> {
        // This exclusive driver access proves the planning borrow has ended.
        let driver = level
            .acquire_driver_mut()
            .unwrap()
            .as_any_mut()
            .downcast_mut::<Driver>()
            .unwrap();
        driver.main_task_runs.push(self.0);
        driver.events.get_mut().push(Event::MainTask(self.0));
        let driver = level.acquire_driver().unwrap();
        shared_acquisition_step(driver, level, self.0)
    }
}

#[test]
fn mutable_acquisition_reuses_ticket_future_and_cache_policy() {
    let mut level = Level::new(
        crate::level::Region::new(&crate::Behaviors::EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    let mut driver = Driver::new(None);
    driver.main_tasks = true;
    driver.remaining.set(1);
    level
        .install_acquire_driver(Box::new(driver), -64..320)
        .unwrap();
    let first = level.get_chunk_mut(POS, ChunkStatus::Full, true).unwrap();
    assert_eq!(
        level.get_chunk_mut(POS, ChunkStatus::Full, true).unwrap(),
        first
    );
    let driver = level
        .acquire_driver()
        .unwrap()
        .as_any()
        .downcast_ref::<Driver>()
        .unwrap();
    assert_eq!(
        driver.main_task_runs,
        [AcquireTaskKind::DistanceUpdates, AcquireTaskKind::Poll]
    );
    assert_eq!(
        *driver.events.borrow(),
        [
            Event::Ticket(POS, ChunkStatus::Full),
            Event::MainTask(AcquireTaskKind::DistanceUpdates),
            Event::Distance,
            Event::Request(POS, ChunkStatus::Full),
            Event::MainTask(AcquireTaskKind::Poll),
            Event::Poll,
        ]
    );
    assert_eq!(driver.requests.get(), 1);
}

#[test]
fn mutable_poll_never_adds_tickets_requests_or_waits() {
    let mut level = Level::new(
        crate::level::Region::new(&crate::Behaviors::EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    let mut driver = Driver::new(Some(33));
    driver.main_tasks = true;
    driver.remaining.set(2);
    driver
        .steps
        .get_mut()
        .push_back(AcquireProgress::DistanceUpdates);
    level
        .install_acquire_driver(Box::new(driver), -64..320)
        .unwrap();
    level.acquisition.cache(POS, ChunkStatus::Full, None);
    assert_eq!(
        level.poll_chunk_acquisition_mut().unwrap(),
        AcquireProgress::DistanceUpdates
    );
    assert!(level.acquisition.cache.get().iter().all(Option::is_none));
    let driver = level
        .acquire_driver()
        .unwrap()
        .as_any()
        .downcast_ref::<Driver>()
        .unwrap();
    assert_eq!(driver.remaining.get(), 1);
    assert_eq!(
        *driver.events.borrow(),
        [Event::MainTask(AcquireTaskKind::Poll), Event::Poll]
    );
    assert_eq!(driver.requests.get(), 0);
    assert!(driver.tickets.borrow().level(POS).is_none());
}

#[test]
fn shared_acquisition_never_executes_a_mutable_task() {
    let mut level = Level::new(
        crate::level::Region::new(&crate::Behaviors::EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    let mut driver = Driver::new(Some(33));
    driver.main_tasks = true;
    driver.remaining.set(1);
    level
        .install_acquire_driver(Box::new(driver), -64..320)
        .unwrap();
    assert!(
        level
            .get_chunk(POS, ChunkStatus::Full, true)
            .unwrap()
            .is_some()
    );
    let driver = level
        .acquire_driver()
        .unwrap()
        .as_any()
        .downcast_ref::<Driver>()
        .unwrap();
    assert!(driver.main_task_runs.is_empty());
    assert_eq!(
        *driver.events.borrow(),
        [
            Event::Ticket(POS, ChunkStatus::Full),
            Event::Request(POS, ChunkStatus::Full),
            Event::Poll
        ]
    );
}

#[test]
fn four_entry_cache_does_not_promote_hits_and_keys_by_status() {
    let level = crate::level::Level::new(
        crate::level::Region::new(&crate::Behaviors::EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    let cache = ChunkAcquisition::default();
    let driver = Driver::new(Some(33));
    for x in 0..4 {
        cache
            .get_chunk(
                &driver,
                &level,
                ChunkPos::new(x, 0),
                ChunkStatus::Full,
                true,
            )
            .unwrap();
    }
    cache
        .get_chunk(&driver, &level, POS, ChunkStatus::Full, true)
        .unwrap();
    assert_eq!(driver.requests.get(), 4);
    cache
        .get_chunk(
            &driver,
            &level,
            ChunkPos::new(4, 0),
            ChunkStatus::Full,
            true,
        )
        .unwrap();
    cache
        .get_chunk(&driver, &level, POS, ChunkStatus::Full, true)
        .unwrap();
    assert_eq!(driver.requests.get(), 6);
    cache
        .get_chunk(&driver, &level, POS, ChunkStatus::Biomes, true)
        .unwrap();
    assert_eq!(driver.requests.get(), 7);
    assert_eq!(
        driver
            .tickets
            .borrow()
            .source_level(POS, TicketTracker::Loading),
        33
    );
    assert_eq!(
        driver
            .events
            .borrow()
            .iter()
            .filter(|e| matches!(e, Event::Ticket(..)))
            .count(),
        7
    );
    assert!(
        !driver
            .events
            .borrow()
            .iter()
            .any(|e| matches!(e, Event::Poll | Event::Distance))
    );
}

#[test]
fn cached_absence_is_retained_for_false_and_bypassed_for_true() {
    let level = crate::level::Level::new(
        crate::level::Region::new(&crate::Behaviors::EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    let cache = ChunkAcquisition::default();
    let driver = Driver::new(None);
    assert_eq!(
        cache.get_chunk(&driver, &level, POS, ChunkStatus::Full, false),
        Ok(None)
    );
    driver.level.set(Some(33));
    driver.present.set(Some(driver.reference(POS)));
    assert_eq!(cache.get_chunk_now(&driver, POS), None);
    assert_eq!(
        cache.get_chunk(&driver, &level, POS, ChunkStatus::Full, false),
        Ok(None)
    );
    assert!(driver.events.borrow().is_empty());
    assert_eq!(
        cache.get_chunk(&driver, &level, POS, ChunkStatus::Full, true),
        Ok(Some(driver.reference(POS)))
    );
    assert_eq!(
        *driver.events.borrow(),
        [
            Event::Ticket(POS, ChunkStatus::Full),
            Event::Request(POS, ChunkStatus::Full)
        ]
    );
    cache.clear_cache();
    cache
        .get_chunk(&driver, &level, POS, ChunkStatus::Full, false)
        .unwrap();
    assert_eq!(driver.requests.get(), 2);
}

#[test]
fn managed_distance_updates_invalidate_other_cached_absence() {
    let level = crate::level::Level::new(
        crate::level::Region::new(&crate::Behaviors::EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    let cache = ChunkAcquisition::default();
    let driver = Driver::new(None);
    assert_eq!(
        cache.get_chunk(&driver, &level, POS, ChunkStatus::Full, false),
        Ok(None)
    );
    driver.level.set(Some(33));
    driver.remaining.set(1);
    driver
        .steps
        .borrow_mut()
        .push_back(AcquireProgress::DistanceUpdates);
    cache
        .get_chunk(
            &driver,
            &level,
            ChunkPos::new(1, 0),
            ChunkStatus::Full,
            false,
        )
        .unwrap();
    assert!(
        cache
            .get_chunk(&driver, &level, POS, ChunkStatus::Full, false)
            .unwrap()
            .is_some()
    );
    assert_eq!(driver.requests.get(), 2);
}

#[test]
fn false_can_managed_block_without_a_ticket_and_wait_only_after_idle() {
    let level = crate::level::Level::new(
        crate::level::Region::new(&crate::Behaviors::EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    let cache = ChunkAcquisition::default();
    let driver = Driver::new(Some(33));
    driver.remaining.set(2);
    driver
        .steps
        .borrow_mut()
        .extend([AcquireProgress::Task, AcquireProgress::Idle]);
    assert_eq!(
        cache.get_chunk(&driver, &level, POS, ChunkStatus::Full, false),
        Ok(Some(driver.reference(POS)))
    );
    assert_eq!(
        *driver.events.borrow(),
        [
            Event::Request(POS, ChunkStatus::Full),
            Event::Poll,
            Event::Poll,
            Event::Wait
        ]
    );
    assert_eq!(driver.tickets.borrow().level(POS), None);
}

#[test]
fn distance_promotion_and_prepared_chunk_publication_finish_before_return() {
    use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
    use ferraloom_data::Block;
    use std::sync::Arc;
    let level = crate::level::Level::new(
        crate::level::Region::new(&crate::Behaviors::EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    let cache = ChunkAcquisition::default();
    let driver = Driver::new(None);
    let section = LevelChunkSection::new(
        Strategy::block_states(),
        Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
        BiomeId(0),
    );
    let mut chunk = crate::level::chunk::LevelChunk::new(POS, 0..1, section);
    let pos = ferraloom_util::BlockPos::new(0, 0, 0);
    chunk
        .set_block_state(pos, Block::STONE.default_state())
        .unwrap();
    driver.prepared.replace(Some(chunk));
    driver.remaining.set(1);
    assert_eq!(
        cache.get_chunk(&driver, &level, POS, ChunkStatus::Full, true),
        Ok(Some(driver.reference(POS)))
    );
    assert_eq!(
        driver.published.get().unwrap().get_block_state(pos),
        Block::STONE.default_state()
    );
    assert_eq!(
        *driver.events.borrow(),
        [
            Event::Ticket(POS, ChunkStatus::Full),
            Event::Distance,
            Event::Request(POS, ChunkStatus::Full),
            Event::Poll
        ]
    );
}

#[test]
fn required_absence_and_terminal_failure_do_not_become_cached_success() {
    let mut level = crate::level::Level::new(
        crate::level::Region::new(&crate::Behaviors::EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    for policy in [
        crate::block::UnportedBehavior::Strict,
        crate::block::UnportedBehavior::Warn,
    ] {
        level.set_unported_behavior(policy);
        let cache = ChunkAcquisition::default();
        let driver = Driver::new(None);
        driver.promote.set(false);
        assert_eq!(
            cache.get_chunk(&driver, &level, POS, ChunkStatus::Full, true),
            Err(LevelError::ChunkAcquisition(
                POS,
                ChunkStatus::Full,
                AcquireFailure::MissingHolder
            ))
        );
        driver.level.set(Some(33));
        driver.result.set(false);
        assert_eq!(
            cache.get_chunk(&driver, &level, POS, ChunkStatus::Full, true),
            Err(LevelError::ChunkAcquisition(
                POS,
                ChunkStatus::Full,
                AcquireFailure::MissingResult
            ))
        );
        let failure =
            LevelError::ChunkAcquisition(POS, ChunkStatus::Full, AcquireFailure::Cancelled);
        *driver.failure.borrow_mut() = Some(failure.clone());
        assert_eq!(
            cache.get_chunk(&driver, &level, POS, ChunkStatus::Full, false),
            Err(failure)
        );
        *driver.failure.borrow_mut() = None;
        driver.result.set(true);
        assert!(
            cache
                .get_chunk(&driver, &level, POS, ChunkStatus::Full, false)
                .unwrap()
                .is_some()
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
}

#[test]
fn now_presence_lighting_and_off_owner_contracts_are_distinct() {
    let level = crate::level::Level::new(
        crate::level::Region::new(&crate::Behaviors::EMPTY),
        crate::level::LevelContext::fixture(0, 63),
    );
    let cache = ChunkAcquisition::default();
    let driver = Driver::new(Some(33));
    assert!(cache.has_chunk(&driver, POS));
    assert_eq!(cache.get_chunk_now(&driver, POS), None);
    assert!(driver.events.borrow().is_empty());
    driver.present.set(Some(driver.reference(POS)));
    assert_eq!(cache.get_chunk_now(&driver, POS), driver.present.get());
    driver.present.set(None);
    assert!(cache.get_chunk_now(&driver, POS).is_some());
    cache.clear_cache();
    driver.level.set(Some(44));
    driver.kind.set(ChunkKind::Proto);
    driver.present.set(Some(driver.reference(POS)));
    assert!(!cache.has_chunk(&driver, POS));
    assert_eq!(cache.get_chunk_now(&driver, POS), None);
    assert_eq!(
        cache.get_chunk_for_lighting(&driver, POS),
        driver.present.get()
    );
    cache.clear_cache();
    driver.kind.set(ChunkKind::Full);
    driver.present.set(Some(driver.reference(POS)));
    // Highest allowed status and ticket level have distinct update points.
    assert!(cache.get_chunk_now(&driver, POS).is_some());
    cache.clear_cache();
    driver.status_allowed.set(false);
    assert_eq!(cache.get_chunk_now(&driver, POS), None);
    driver.owner.set(false);
    assert_eq!(cache.get_chunk_now(&driver, POS), None);
    assert_eq!(
        cache.get_chunk(&driver, &level, POS, ChunkStatus::Full, true),
        Ok(driver.present.get())
    );
    assert_eq!(*driver.events.borrow(), [Event::Forward]);
}

#[test]
fn unknown_tickets_use_native_status_levels_and_signed_refresh_expiry() {
    let statuses = [
        ChunkStatus::Empty,
        ChunkStatus::StructureStarts,
        ChunkStatus::StructureReferences,
        ChunkStatus::Biomes,
        ChunkStatus::Terrain,
        ChunkStatus::Features,
        ChunkStatus::InitializeLight,
        ChunkStatus::Light,
        ChunkStatus::Spawn,
        ChunkStatus::Full,
    ];
    let native = include_str!("../../../tests/java/level/acquisition.tsv");
    for (status, line) in statuses
        .into_iter()
        .zip(native.lines().filter(|line| line.starts_with("level\t")))
    {
        let expected: i32 = line.rsplit('\t').next().unwrap().parse().unwrap();
        assert_eq!(ticket_level(status), expected);
    }
    let mut tickets = TicketStorage::default();
    tickets.add_unknown(POS, ChunkStatus::Full);
    assert_eq!(tickets.source_level(POS, TicketTracker::Loading), 33);
    assert_eq!(
        tickets.source_level(POS, TicketTracker::Simulation),
        super::super::distance_manager::MAX_LOADED_LEVEL + 1
    );
    assert!(!tickets.should_keep_dimension_active());
    let mut present = vec![tickets.contains_type(POS, TicketType::Unknown)];
    tickets.purge_stale_with_readiness(1, |_| false);
    present.push(tickets.contains_type(POS, TicketType::Unknown));
    tickets.add_unknown(POS, ChunkStatus::Full);
    present.push(tickets.contains_type(POS, TicketType::Unknown));
    for tick in 2..=3 {
        tickets.purge_stale_with_readiness(tick, |_| false);
        present.push(tickets.contains_type(POS, TicketType::Unknown));
    }
    let trace = present
        .into_iter()
        .map(|p| if p { "1" } else { "0" })
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        native
            .lines()
            .find_map(|line| line.strip_prefix("unknown-refresh\t"))
            .unwrap(),
        trace
    );
}
