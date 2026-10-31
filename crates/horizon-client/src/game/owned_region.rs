//! Live owned-region access. No canonical borrow crosses a behaviour callback.
mod fixture;

use super::*;
use ferraloom_game::{
    block_phase::BlockPhaseLevel, random_tick_sections::Sections, region::Region,
};
use horizon_world::level::tick_access::{TickAccess, TickScope};
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Mode {
    #[default]
    Off,
    Serial,
}
impl std::str::FromStr for Mode {
    type Err = &'static str;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "off" => Ok(Self::Off),
            "serial" => Ok(Self::Serial),
            _ => Err("--regions must be off or serial"),
        }
    }
}

// A worker owns this value; sharing the adapter's RefCell is never required.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Region<World>>();
};

/// Captured at the same boundary as the existing `GameLevel` frame. Entities
/// are never captured here: queries go to their current serial owner.
struct Frame {
    resources: Arc<Resources>,
    dimension: LevelDimension,
    darken: i32,
    raining: bool,
    thundering: bool,
    rules: horizon_world::level::GameRules,
    difficulty: i32,
    seed: i64,
    tickets: horizon_client::ticket_levels::Levels,
    client_side: bool,
}

enum Request {
    Analog(BlockPos, BlockStateId, Direction),
    ItemFrame(BlockPos, Direction),
    Signal(BlockPos, BlockStateId, Direction),
    SpreadFire(BlockPos),
    Unobstructed(BlockPos, BlockStateId),
    Entities(Aabb, EntityQuery),
    Emit(LevelEvent),
    RemoveBlockEntity(BlockPos, BlockStateId, bool),
    CommandPowered(BlockPos, bool),
}
enum Reply {
    Int(i32),
    OptionalInt(Option<i32>),
    Bool(bool),
    Count(usize),
    Byte(u8),
    Powered(Option<(bool, bool)>),
    Done,
}
type Panic = Box<dyn std::any::Any + Send>;
struct Returned {
    region: Region<World>,
    result: std::result::Result<Reply, Panic>,
}
trait Services {
    /// Returns the actual owner even if the serial callback unwinds. The caller
    /// restores its slot before resuming the panic.
    fn call(&self, region: Region<World>, request: Request) -> Returned;
}

struct OwnedLevel<S> {
    region: RefCell<Option<Region<World>>>,
    frame: Frame,
    services: S,
    handoffs: Cell<u64>,
    revision: Cell<(u64, u64, u64, u64)>,
}
impl<S: Services> OwnedLevel<S> {
    fn new(region: Region<World>, frame: Frame, services: S) -> Self {
        Self {
            region: RefCell::new(Some(region)),
            frame,
            services,
            handoffs: Cell::new(0),
            revision: Cell::new((u64::MAX, u64::MAX, u64::MAX, 0)),
        }
    }
    #[inline]
    fn read<R>(&self, read: impl FnOnce(&Region<World>) -> R) -> R {
        let owner = self.region.borrow();
        read(
            owner
                .as_ref()
                .expect("region is lent to the exclusive lane"),
        )
    }
    #[inline]
    fn write<R>(&self, write: impl FnOnce(&mut Region<World>) -> R) -> R {
        let mut owner = self.region.borrow_mut();
        write(
            owner
                .as_mut()
                .expect("region is lent to the exclusive lane"),
        )
    }
    fn exclusive(&self, request: Request) -> Reply {
        let region = self.region.borrow_mut().take().expect("nested region loan");
        let Returned { region, result } = self.services.call(region, request);
        {
            let mut owner = self.region.borrow_mut();
            assert!(
                owner.is_none(),
                "exclusive callback replaced an active owner"
            );
            *owner = Some(region);
        }
        self.handoffs.set(self.handoffs.get().wrapping_add(1));
        result.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    }
}
impl<S: Services> TickAccess for OwnedLevel<S> {
    fn read_ticks<R>(&self, _scope: TickScope, read: impl FnOnce(&Ticks) -> R) -> R {
        // M3 owns the entire dimension, including its one admission budget.
        self.read(|region| read(&region.ticks))
    }
    fn write_ticks<R>(&mut self, _scope: TickScope, write: impl FnOnce(&mut Ticks) -> R) -> R {
        self.write(|region| write(&mut region.ticks))
    }
}
impl<S: Services> LevelAccess for OwnedLevel<S> {
    fn is_client_side(&self) -> bool {
        self.frame.client_side
    }
    fn world_surface(&self, x: i32, z: i32) -> i32 {
        self.read(|r| {
            r.world
                .world_surface(x, z, |s| self.frame.resources.facts().is_air(s))
        })
    }
    fn precipitation(&self, pos: BlockPos) -> Option<horizon_world::level::Precipitation> {
        self.frame
            .resources
            .behaviours()
            .precipitation_info(self, pos)
    }
    fn is_raining_at(&self, pos: BlockPos) -> bool {
        self.frame.resources.behaviours().is_raining_at(self, pos)
    }
    fn block_entity_analog_signal(
        &self,
        pos: BlockPos,
        state: BlockStateId,
        direction: Direction,
    ) -> i32 {
        match self.exclusive(Request::Analog(pos, state, direction)) {
            Reply::Int(v) => v,
            _ => unreachable!(),
        }
    }
    fn item_frame_analog_output(&self, pos: BlockPos, direction: Direction) -> Option<i32> {
        match self.exclusive(Request::ItemFrame(pos, direction)) {
            Reply::OptionalInt(v) => v,
            _ => unreachable!(),
        }
    }
    fn block_entity_signal(&self, pos: BlockPos, state: BlockStateId, direction: Direction) -> i32 {
        match self.exclusive(Request::Signal(pos, state, direction)) {
            Reply::Int(v) => v,
            _ => unreachable!(),
        }
    }
    fn biome_name(&self, pos: BlockPos) -> Option<&str> {
        let id = self.read(|r| {
            r.world
                .chunks
                .get(&(pos.x >> 4, pos.z >> 4))
                .map(|c| c.biome(pos.x, pos.y, pos.z))
        })?;
        Some(&self.frame.resources.biomes().get(id).name)
    }
    fn world_seed(&self) -> i64 {
        self.frame.seed
    }
    fn day_time(&self) -> i64 {
        self.read(|r| r.world.day_time)
    }
    fn game_rules(&self) -> horizon_world::level::GameRules {
        self.frame.rules
    }
    fn difficulty(&self) -> i32 {
        self.frame.difficulty
    }
    fn can_spread_fire_around(&self, pos: BlockPos) -> bool {
        if self.frame.rules.fire_spread_radius_around_player == -1 {
            return true;
        }
        match self.exclusive(Request::SpreadFire(pos)) {
            Reply::Bool(v) => v,
            _ => unreachable!(),
        }
    }
    fn increased_fire_burnout(&self, pos: BlockPos) -> bool {
        self.read(|r| {
            r.world
                .chunks
                .get(&(pos.x >> 4, pos.z >> 4))
                .is_some_and(|c| {
                    self.frame
                        .resources
                        .biomes()
                        .get(c.biome(pos.x, pos.y, pos.z))
                        .increased_fire_burnout
                })
        })
    }
    #[inline]
    fn block_state(&self, pos: BlockPos) -> BlockStateId {
        self.read(|r| r.world.block(pos.x, pos.y, pos.z))
    }
    fn set_raw(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId> {
        self.write(|r| {
            if !r.world.loaded(pos.x, pos.z) || pos.y < MIN_Y || pos.y >= MIN_Y + HEIGHT {
                return None;
            }
            let previous = r.world.block(pos.x, pos.y, pos.z);
            r.world.set(pos.x, pos.y, pos.z, state).ok()?;
            Some(previous)
        })
    }
    fn is_loaded(&self, pos: BlockPos) -> bool {
        self.read(|r| r.world.loaded(pos.x, pos.z))
    }
    fn min_y(&self) -> i32 {
        if self.frame.dimension == LevelDimension::Overworld {
            MIN_Y
        } else {
            0
        }
    }
    fn max_y(&self) -> i32 {
        if self.frame.dimension == LevelDimension::Overworld {
            MIN_Y + HEIGHT
        } else {
            256
        }
    }
    #[inline]
    fn with_random<R>(
        &mut self,
        draw: impl FnOnce(&mut horizon_core::random::LegacyRandom) -> R,
    ) -> R {
        self.write(|r| draw(&mut r.world.random))
    }
    fn game_time(&self) -> i64 {
        self.read(|r| r.world.game_time)
    }
    fn block_light(&self, pos: BlockPos) -> i32 {
        self.read(|r| i32::from(r.world.light(pos.x, pos.y, pos.z) & 15))
    }
    fn sky_light(&self, pos: BlockPos) -> i32 {
        self.read(|r| i32::from(r.world.light(pos.x, pos.y, pos.z) >> 4))
    }
    fn sky_darken(&self) -> i32 {
        self.frame.darken
    }
    fn sun_angle(&self, _pos: BlockPos) -> f32 {
        let environment = match self.frame.dimension {
            LevelDimension::Overworld => self.frame.resources.environment(),
            LevelDimension::Nether => self.frame.resources.nether_environment(),
            LevelDimension::End => self.frame.resources.end_environment(),
        };
        environment.sample(self.day_time(), None, None).sun_angle
    }
    fn is_raining(&self) -> bool {
        self.frame.raining
    }
    fn is_thundering(&self) -> bool {
        self.frame.thundering
    }
    fn dimension(&self) -> LevelDimension {
        self.frame.dimension
    }
    fn is_unobstructed(&self, pos: BlockPos, state: BlockStateId) -> bool {
        match self.exclusive(Request::Unobstructed(pos, state)) {
            Reply::Bool(v) => v,
            _ => unreachable!(),
        }
    }
    fn is_ticking_chunk(&self, chunk: horizon_core::position::ChunkPos) -> bool {
        self.frame.tickets.block_ticking(chunk.x, chunk.z)
            && self.read(|r| ticking_chunk(&r.world, chunk.x, chunk.z))
    }
    fn count_entities(&self, area: Aabb, query: EntityQuery) -> usize {
        match self.exclusive(Request::Entities(area, query)) {
            Reply::Count(v) => v,
            _ => unreachable!(),
        }
    }
    fn emit(&mut self, event: LevelEvent) {
        let immediate = matches!(
            &event,
            LevelEvent::DropResources { .. }
                | LevelEvent::BlockEvent {
                    kind: "pop_block_item",
                    ..
                }
                | LevelEvent::Player {
                    effect: PlayerEffect::DropItem(_) | PlayerEffect::InteractLoot(_),
                    ..
                }
        );
        if immediate {
            match self.exclusive(Request::Emit(event)) {
                Reply::Done => {}
                _ => unreachable!(),
            }
        } else if !matches!(&event, LevelEvent::Sound { .. } if self.frame.client_side) {
            self.write(|r| r.world.level_events.push(event));
        }
    }
    fn remove_block_entity(&mut self, pos: BlockPos, old: BlockStateId, side_effects: bool) -> u8 {
        match self.exclusive(Request::RemoveBlockEntity(pos, old, side_effects)) {
            Reply::Byte(v) => v,
            _ => unreachable!(),
        }
    }
    fn load_block_entity(&mut self, pos: BlockPos, mut nbt: horizon_world::nbt::Compound) {
        use horizon_world::nbt::Tag;
        for (k, v) in [("x", pos.x), ("y", pos.y), ("z", pos.z)] {
            nbt.insert(k.into(), Tag::Int(v));
        }
        if let Some((key, entity)) = horizon_client::save::decode_block_entity(&Tag::Compound(nbt))
        {
            self.write(|r| r.world.block_entities.insert(key, entity));
        }
    }
    fn set_command_block_powered(&mut self, pos: BlockPos, powered: bool) -> Option<(bool, bool)> {
        match self.exclusive(Request::CommandPowered(pos, powered)) {
            Reply::Powered(v) => v,
            _ => unreachable!(),
        }
    }
}
impl<S: Services> BlockPhaseLevel for OwnedLevel<S> {
    type Chunk = horizon_client::chunk::Chunk;
    fn block_revision(&self) -> u64 {
        let (revision, membership) =
            self.read(|r| (r.world.revision, r.world.chunks.membership_revision()));
        let (previous, previous_membership, previous_handoffs, token) = self.revision.get();
        if (revision, membership, self.handoffs.get())
            != (previous, previous_membership, previous_handoffs)
        {
            let token = token.wrapping_add(1);
            self.revision
                .set((revision, membership, self.handoffs.get(), token));
            token
        } else {
            token
        }
    }
    fn resident_chunk(&self, chunk: horizon_core::position::ChunkPos) -> Option<Arc<Self::Chunk>> {
        self.read(|r| r.world.chunks.get(&(chunk.x, chunk.z)).cloned())
    }
    fn random_tick_sections(&mut self, chunk: horizon_core::position::ChunkPos) -> Sections {
        self.write(|r| {
            r.world
                .chunks
                .get(&(chunk.x, chunk.z))
                .map_or_else(Sections::default, |column| {
                    r.world.random_tick_sections.get(
                        (chunk.x, chunk.z),
                        column,
                        self.frame.resources.facts(),
                    )
                })
        })
    }
}

/// Only the coordinator has this handle. Worker services use owned requests
/// instead; no `Game`, global entity borrow or coordinator cell is shared.
struct SerialServices<'g, 'r> {
    game: RefCell<&'g mut Game<'r>>,
}
impl Services for SerialServices<'_, '_> {
    fn call(&self, region: Region<World>, request: Request) -> Returned {
        let mut game = self.game.borrow_mut();
        let id = region.id;
        game.restore_block_region(region);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            game.with_level_unflushed(|_, level, _| match request {
                Request::Analog(pos, state, direction) => {
                    Reply::Int(level.block_entity_analog_signal(pos, state, direction))
                }
                Request::ItemFrame(pos, direction) => {
                    Reply::OptionalInt(level.item_frame_analog_output(pos, direction))
                }
                Request::Signal(pos, state, direction) => {
                    Reply::Int(level.block_entity_signal(pos, state, direction))
                }
                Request::SpreadFire(pos) => Reply::Bool(level.can_spread_fire_around(pos)),
                Request::Unobstructed(pos, state) => Reply::Bool(level.is_unobstructed(pos, state)),
                Request::Entities(area, query) => Reply::Count(level.count_entities(area, query)),
                Request::Emit(event) => {
                    level.emit(event);
                    Reply::Done
                }
                Request::RemoveBlockEntity(pos, state, effects) => {
                    Reply::Byte(level.remove_block_entity(pos, state, effects))
                }
                Request::CommandPowered(pos, powered) => {
                    Reply::Powered(level.set_command_block_powered(pos, powered))
                }
            })
        }));
        Returned {
            region: game.take_block_region(id),
            result,
        }
    }
}
impl Game<'_> {
    fn take_block_region(&mut self, id: ferraloom_game::region::RegionId) -> Region<World> {
        // Check both first: a failed ownership request cannot strand one field.
        self.world
            .as_ref()
            .unwrap_or_else(|error| panic!("{error}"));
        self.server_level
            .block_ticks
            .as_ref()
            .unwrap_or_else(|error| panic!("{error}"));
        Region::new(
            id,
            self.world.take().unwrap(),
            self.server_level.block_ticks.take().unwrap(),
        )
    }
    fn restore_block_region(&mut self, region: Region<World>) {
        assert!(
            self.world.as_ref().is_err() && self.server_level.block_ticks.as_ref().is_err(),
            "cannot overwrite resident canonical state"
        );
        self.world
            .restore(region.world)
            .unwrap_or_else(|(error, _)| panic!("{error}"));
        self.server_level
            .block_ticks
            .restore(region.ticks)
            .unwrap_or_else(|(error, _)| panic!("{error}"));
    }
    fn block_region_frame(&self) -> Frame {
        let (rain, thunder) = self.world.weather.levels(1.0);
        Frame {
            resources: Arc::clone(&self.res),
            dimension: match self.dim {
                Dim::Overworld => LevelDimension::Overworld,
                Dim::Nether => LevelDimension::Nether,
                Dim::End => LevelDimension::End,
            },
            darken: self.sky_darken(),
            raining: rain > 0.2,
            thundering: thunder > 0.9,
            rules: self.level_rules(),
            difficulty: i32::from(self.ui.difficulty),
            seed: self.level.seed,
            tickets: self.server_level.tickets.levels(),
            client_side: self.remote.is_some(),
        }
    }
    pub(super) fn tick_region_block_phase(&mut self, ticking: Vec<(i32, i32)>) {
        if self.options.regions == Mode::Off {
            self.with_level(|b, level, ticks| {
                ferraloom_game::block_phase::tick_random_chunks(
                    b,
                    level,
                    ticks,
                    ticking
                        .into_iter()
                        .map(|(x, z)| horizon_core::position::ChunkPos::new(x, z)),
                );
                b.run_block_events(level, ticks);
                b.tick_block_entities(level, ticks);
            });
        } else {
            self.tick_owned_random_chunks(
                ticking
                    .into_iter()
                    .map(|(x, z)| horizon_core::position::ChunkPos::new(x, z))
                    .collect(),
            );
            // The owned kernel does not flush. Preserve the combined phase's
            // existing boundary after block events and moving-piston tickers.
            self.with_level(|b, level, ticks| {
                b.run_block_events(level, ticks);
                b.tick_block_entities(level, ticks);
            });
        }
    }
    pub(super) fn tick_owned_random_chunks(
        &mut self,
        chunks: Vec<horizon_core::position::ChunkPos>,
    ) {
        let frame = self.block_region_frame();
        let resources = Arc::clone(&frame.resources);
        let region = self.take_block_region(ferraloom_game::region::RegionId::REFERENCE);
        let mut level = OwnedLevel::new(
            region,
            frame,
            SerialServices {
                game: RefCell::new(self),
            },
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ferraloom_game::block_phase::tick_random_chunks_with_context(
                resources.behaviours(),
                &mut level,
                chunks,
            );
        }));
        let OwnedLevel {
            region, services, ..
        } = level;
        services
            .game
            .into_inner()
            .restore_block_region(region.into_inner().expect("active region did not return"));
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }
}
