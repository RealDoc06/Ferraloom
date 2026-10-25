//! Synthetic real listeners observe synchronous calls in native golden fixtures.
use super::*;
use crate::level::effects::LevelEffect;

#[derive(Clone, Debug)]
pub(crate) enum Observation {
    Effect(LevelEffect),
    GameEvent {
        event: GameEvent,
        position: Vec3,
        source: Option<EntityRef>,
        affected_state: Option<BlockState>,
    },
}
#[derive(Clone, Debug)]
pub(super) enum Action {
    Register(usize),
    Unregister(usize),
    Reenter(Vec3),
    Detach(ChunkPos),
    Fail,
}
pub(super) struct TestListenerState {
    pub behavior: &'static dyn GameEventListener,
    pub pos: BlockPos,
    pub radius: i32,
    pub label: String,
    pub actions: Vec<Action>,
    recorder: bool,
}
#[derive(Default)]
pub(super) struct ListenerTests {
    pub listeners: Vec<TestListenerState>,
    pub trace: Vec<String>,
    events: Vec<(usize, Observation)>,
}
struct Recorder(DeliveryMode);
static IMMEDIATE: Recorder = Recorder(DeliveryMode::Unspecified);
static DISTANCE: Recorder = Recorder(DeliveryMode::ByDistance);
impl GameEventListener for Recorder {
    fn get_listener_source(
        &self,
        listener: GameEventListenerRef,
        level: &Level,
    ) -> Result<PositionSource, LevelError> {
        let GameEventListenerRef::Test(id) = listener else {
            unreachable!()
        };
        Ok(PositionSource::Block(
            level.region().game_event_listeners.borrow().tests.listeners[id].pos,
        ))
    }
    fn get_listener_radius(
        &self,
        listener: GameEventListenerRef,
        level: &Level,
    ) -> Result<i32, LevelError> {
        let GameEventListenerRef::Test(id) = listener else {
            unreachable!()
        };
        Ok(level.region().game_event_listeners.borrow().tests.listeners[id].radius)
    }
    fn get_delivery_mode(&self) -> DeliveryMode {
        self.0
    }
    fn handle_game_event(
        &self,
        listener: GameEventListenerRef,
        level: &mut Level,
        event: GameEvent,
        context: GameEventContext,
        position: Vec3,
    ) -> Result<bool, LevelError> {
        let GameEventListenerRef::Test(id) = listener else {
            unreachable!()
        };
        let effect_count = level.region().effects.len();
        let tests = &mut level.region_mut().game_event_listeners.get_mut().tests;
        let data = &mut tests.listeners[id];
        if data.recorder {
            let source = BlockPos::containing(position.x, position.y, position.z);
            if source.x >> 4 != data.pos.x >> 4
                || source.y >> 4 != data.pos.y >> 4
                || source.z >> 4 != data.pos.z >> 4
            {
                return Ok(false);
            }
            tests.events.push((
                effect_count,
                Observation::GameEvent {
                    event,
                    position,
                    source: context.source_entity,
                    affected_state: context.affected_state,
                },
            ));
        } else {
            tests.trace.push(data.label.clone());
        }
        let actions = std::mem::take(&mut data.actions);
        for action in actions {
            match action {
                Action::Register(other) => {
                    let pos =
                        level.region().game_event_listeners.borrow().tests.listeners[other].pos;
                    level.register_listener(GameEventListenerRef::Test(other), pos)?;
                }
                Action::Unregister(other) => {
                    let pos =
                        level.region().game_event_listeners.borrow().tests.listeners[other].pos;
                    level.unregister_listener(GameEventListenerRef::Test(other), pos)?;
                }
                Action::Reenter(source) => level.game_event(
                    event,
                    source,
                    context.source_entity,
                    context.affected_state,
                )?,
                Action::Detach(chunk) => detach_chunk_listeners(chunk, level)?,
                Action::Fail => return Err(LevelError::ConcurrentGameEventListenerModification),
            }
        }
        Ok(true)
    }
}
pub(super) fn add(
    level: &mut Level,
    pos: BlockPos,
    radius: i32,
    mode: DeliveryMode,
    label: &str,
) -> usize {
    let tests = &mut level.region_mut().game_event_listeners.get_mut().tests;
    let id = tests.listeners.len();
    tests.listeners.push(TestListenerState {
        behavior: if mode == DeliveryMode::ByDistance {
            &DISTANCE
        } else {
            &IMMEDIATE
        },
        pos,
        radius,
        label: label.into(),
        actions: Vec::new(),
        recorder: false,
    });
    id
}

/// Install section-local listeners in an explicitly loaded native-level fixture.
/// Their callbacks record at the actual dispatch point, never at an effect drain.
pub(crate) fn observe(level: &mut Level) {
    let mut positions = Vec::new();
    for (_, chunk) in level.region_mut().chunks.iter_mut() {
        chunk.loaded.set(true);
        let key = chunk.pos();
        for y in (chunk.min_y() >> 4)..(chunk.max_y() >> 4) {
            positions.push(BlockPos::new(key.x * 16 + 8, y * 16 + 8, key.z * 16 + 8));
        }
    }
    for pos in positions {
        let id = add(level, pos, 32, DeliveryMode::Unspecified, "recorder");
        level
            .region_mut()
            .game_event_listeners
            .get_mut()
            .tests
            .listeners[id]
            .recorder = true;
        level
            .register_listener(GameEventListenerRef::Test(id), pos)
            .unwrap();
    }
}
pub(crate) fn take(level: &mut Level) -> Vec<Observation> {
    let records = std::mem::take(
        &mut level
            .region_mut()
            .game_event_listeners
            .get_mut()
            .tests
            .events,
    );
    let effects = level.take_effects();
    let mut records = records.into_iter().peekable();
    let mut observed = Vec::new();
    for (index, effect) in effects
        .into_iter()
        .map(Some)
        .chain(std::iter::once(None))
        .enumerate()
    {
        while records.peek().is_some_and(|(count, _)| *count == index) {
            observed.push(records.next().unwrap().1);
        }
        if let Some(effect) = effect {
            observed.push(Observation::Effect(effect));
        }
    }
    assert!(
        records.next().is_none(),
        "fixture cleared effects without clearing its synchronous listener observations"
    );
    observed
}

/// Native VibrationSystem.User#onDataChanged observation, before setChanged.
pub(crate) fn record_vibration_change(level: &mut Level, entry: String) {
    level
        .region_mut()
        .game_event_listeners
        .get_mut()
        .tests
        .trace
        .push(entry);
}
pub(crate) fn take_vibration_changes(level: &mut Level) -> Vec<String> {
    std::mem::take(
        &mut level
            .region_mut()
            .game_event_listeners
            .get_mut()
            .tests
            .trace,
    )
}
