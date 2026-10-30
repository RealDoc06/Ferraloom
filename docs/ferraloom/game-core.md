# Game core

Decision for coordinator review. Vanilla 26.3 is the behavior specification;
`--regions off` is the exact execution reference. Parallel mode preserves each
region's vanilla phase order and admits the within-phase interleaving described
below. This document does not claim that a fixed spatial margin proves parity.

## Modules and shared chunk data

Keep the tightly coupled simulation in `ferraloom-game`, with these public roots:

- `level::{server_level, region, chunk, entity, lighting, material, redstone,
  game_event, saved_data}`: `Level`, `ServerLevel`, chunk ownership, entity
  section indexes, fluids and neighbor updates.
- `block::{state, entity, ...}`: `BlockBehaviour`, concrete block classes and
  block entities. Registry `Block` and `BlockState` remain `ferraloom-data` types.
- `entity::{ai, animal, monster, player, ...}`: vanilla hierarchy and subclasses.
- `item`, `inventory`, `crafting`, `loot`, `advancements`, `scoreboard`,
  `commands`, `ticks`, `gametest`: corresponding vanilla packages/concepts.

The small foundation currently puts `Region`/`Level` in `level/mod.rs`; split
that file as implementations arrive. Do not create empty modules in advance.
`ferraloom-server` owns the runtime, server loop, workers, connections, auth and
configuration; `game::entity::player` owns `ServerPlayer` gameplay. Commands and
packet handlers call game code, never socket APIs.

The leaf crate **`ferraloom-chunk`** depends only on util/data: generic
`PalettedContainer<T>`, palette strategies, packed bit storage and
`LevelChunkSection` (block/biome containers and derived counts). Game, worldgen,
storage and protocol depend on it. NBT codecs belong in storage; wire codecs in
protocol. `LevelChunk`, tickets, block entities and behavior stay in game.
The foundation reexports and uses that shared section type. Its palettes support
single, linear, hash-map and global strategies, with separate disk packing and
raw memory views. Dynamic biome IDs use the dimension's live registry, not a
pinned built-in-only list. Do not introduce a second block-state representation. A storage
dependency would invert the graph; protocol depending on game creates a cycle;
putting simulation helpers into generated data mixes unrelated ownership.

All facts use the merged `ferraloom-data`: `Block::default_state()`, validated
`BlockState`, typed properties, `Item`/default components and `EntityType` facts.
Behavior tables index their protocol IDs. Context-dependent shapes explicitly
query the level; extracted baseline shapes are not universal collision answers.
All coordinates and RNG algorithms come from `ferraloom-util`.

## World ownership and boundaries

`Server` owns dimensions in vanilla iteration order. Each `ServerLevel` owns
dimension metadata, `ServerChunkCache`/`ChunkMap`, `TicketStorage`,
`DistanceManager`, ownership routing and movable `Region` values. A region owns:

- Loaded `LevelChunk`s keyed by `ChunkPos`; sections indexed relative to the
  dimension's minimum section. Chunk status, ticking eligibility and ownership
  are separate. A loaded chunk need not tick blocks or entities.
- Canonical entities and section/spatial indexes, plus an insertion-ordered
  `EntityTickList`. Lookup-map iteration never substitutes for tick order.
- Per-chunk block entities and their pending/rebindable ticker handles; block
  and fluid `LevelChunkTicks` containers; ordered, deduplicated block events;
  neighbor update stacks, POIs and region-local dirty/output buffers.

Use **8 chunks / 128 blocks** as the baseline dependency margin. This covers
the ordinary natural-spawn neighborhood (`DistanceManager` uses radius 8), the
64-block block-event audience, and short movement/redstone interactions. It is
not an upper bound: `PathNavigation` uses live follow-range/path-length values,
explosions have runtime radius, motion can be command-supplied, and synchronous
neighbor chains can cross arbitrarily many loaded chunks.

Before a tick, form connected components of loaded dependency closures: inflate
active content by eight chunks, include touched loaded neighbors, transitively
union overlapping closures, and enlarge for actual movement sweeps, navigation
query bounds, explosion bounds and references that can be observed this tick.
Every loaded chunk has one owner; halos are ownership, not stale copies.
Local code reads its owner's live data. Missing ownership is never air.
Synchronous `getChunk(..., true)` is a dependency-closure event: load and merge
under exclusive ownership, then resume with the actual chunk. Do not return an
empty chunk, silently skip work, or delay a synchronous neighbor query.

Merge before dispatch or in the exclusive lane, preserving original entity/tick
ordinals, containers and pending events; fork random streams as specified below.
Split only at the end of
a complete tick, when the recomputed closure graph disconnects and no active
stack, passenger tree, pending synchronous operation or job pins both sides.
No inactivity timer is required for correctness; later hysteresis is a tuning
choice. Fixed square regions cut machines in half; read-only ghost borders
cannot reproduce immediate neighbor mutations.

## Tick phases and exclusive work

The scheduler is in server. Region methods are plain single-threaded `&mut`
code. **All regions join at each phase boundary.** A region requesting exclusive
work during phase k parks at the precise call site; other regions finish phase
k (or park on their own exclusive request). The scheduler grants one request at
a time, lets its owner borrow all required returned regions, then resumes it.
No worker advances to k+1 until every region finishes k. Requests have stable
phase/region/operation ordinals; no arrival-time race chooses the winner.

The server phase driver calls a concrete `Level` adapter owning `Option<Region>`,
not a method holding a borrow of the canonical region for the whole phase.
World calls can take that owned region out of the adapter after state borrows
end. Parked stacks retain handles/scalars and the adapter, not references into
canonical state. Other regions return owned values to the scheduler, which
lends them to the selected worker's dimension/server adapter. No simulation
field needs a mutex. The coordinator can wait on channels outside game logic.
A second parked request likewise yields its region before the first receives it. Server
integration must implement these continuations before enabling regions; the
foundation does not pretend to implement a parallel scheduler.

This defines a valid sequential **within-phase** interleaving. It does not
preserve vanilla's total order across regions: a scheduled-tick callback can
observe a different subset of another region's completed ticks. Such an
operation enters the exclusive lane, but the earlier interleaving remains
observable. This is the approved parallel-mode difference. Single-region mode
never parks and follows vanilla's original order. Pure messages cannot preserve
synchronous return values, callbacks or multi-target command semantics.

Port these phases literally, including freeze/debug/empty-world gates:

1. `MinecraftServer#processPacketsAndTick`: begin frame, process queued packets,
   then `tickServer`. Packet application precedes functions and level ticking.
2. `tickServer`: empty-server pause counter; on pause threshold autosave once,
   tick connections and return. Otherwise increment tick count, tick rate
   manager, then `tickChildren`.
3. `tickChildren`: suspend player flushing; command functions; server clocks
   when running normally; game-time sync every 20 ticks; effective respawn data.
   These are serial server phases, retaining vanilla dimension order.
4. For each dimension, `ServerLevel#tick` follows the phases below. Finish that
   dimension before beginning the next, even with regions enabled. Cross-dimension
   parallelism is a phase-5 option, decided by measurement rather than assumed.
5. After all levels: connections; `PlayerList#tick`; debug subscribers; GameTests
   when running normally; GUI/tickables; each player's chunk batch and resume
   flushing; activity monitor. `ServerGamePacketListenerImpl#tick` calls
   `ServerPlayer#doTick` here. Do not move that call into the earlier entity phase;
   `ServerPlayer#tick` in the entity phase is a different method.
6. Back in `tickServer`: status refresh; autosave countdown/save; tick statistics.
   Task polling between ticks stays separate from the tick body.

The dimension sequence is:

1. Set handling-tick, invalidate environment cache. If running: world border,
   then weather. Check sleep quorum/wake players and move the default world
   clock to the wake marker if allowed; update sky brightness. If running,
   `tickTime` increments game time and executes scheduled functions. These are
   dimension/server serial phases. 26.3 uses `ServerClockManager`, not an invented
   per-region day-time counter.
2. If not debug and running: scheduled block ticks (dimension-wide cap 65,536),
   then fluid ticks (separate cap 65,536). Each is its own region phase. The
   coordinator grants the single dimension budget in reference order; it is
   never 65,536 per region. Per-chunk queue order is time/priority/sub-tick;
   due container selection is priority/sub-tick (`LevelTicks`, `ScheduledTick`).
   Collect before execute; deduplicate by position/type; check current block
   type before dispatch. Loaded saved ticks also need vanilla's negative
   sub-tick initialization. Preserve `willTickThisTick`/area-copy semantics.
3. Raids when running: serial dimension phase, with live access to relevant
   regions. Raids change entities and POIs, not just a global counter.
4. `ServerChunkCache#tick`: conditional stale-ticket purge; distance updates;
   chunks; `ChunkMap#tick()`; POI/unload work (`ChunkMap#tick(haveTime)`); clear
   source cache. Preserve those subphase boundaries.
   Chunk ticking first builds the dimension-wide natural spawn state/local caps;
   filters and shuffles spawning chunks; increments inhabited time, ticks thunder
   and natural spawning in that order; iterates block-ticking chunks for weather
   and random ticks; then custom spawners. Broadcast changed chunks follows.
   In parallel mode build exact dimension-wide `SpawnState` counts at the
   chunk-tick barrier while all regions are parked. Partition each category's
   remaining global budget by each region's share of spawnable chunks: integer
   quotas first, remaining units by largest fractional remainder, then region
   ordinal. Regions use those quotas and their local per-player caps without a
   shared counter during spawning. Players whose spawn neighborhoods overlap
   have merged dependency closures. Local caps can further suppress a quota;
   unused quota is not redistributed during this phase. This avoids overshooting
   global caps, but can reduce spawns when another region could have used spare
   quota. Do not assign each region a full vanilla global cap. Single-region
   mode uses vanilla's unpartitioned `SpawnState`/category eligibility exactly.
   Regional RNG streams handle shuffle/random ticking independently.
   `ChunkMap#tick()` updates player chunk tracking, entity
   movement/tracker packets, then moved-player audiences. This precedes entities.
5. Block events when running: insertion-ordered set, drain newly enqueued events
   in the same phase; retain events in non-ticking chunks for later. Clear
   handling-tick; reset empty time for active tickets, increment when running.
6. If empty time <300: dragon fight when running; `EntityTickList` iteration;
   block entities. For each entity: removed/frozen checks, despawn check,
   player-or-entity-ticking eligibility, vehicle validation, `commonTick`, virtual
   `tick`, then recursive passenger `commonTick`/`rideTick`. Snapshot iteration
   membership so additions wait, but consult live removed flags. Block-entity
   phase appends pending tickers, removes invalid tickers and ticks eligible
   ones when running. New tickers created during iteration wait until next time.
7. Entity management (`PersistentEntitySectionManager`: integrate loads, process
   unloads); debug neighbor listener/synchronizers; random-state cache collection.

Vanilla collection traversal order is part of each port, including fastutil hash
iteration where used. Do not replace it with Rust `HashMap` iteration or sorting
coordinates. The skeleton's maps are lookup-only, not a completed chunk tracker.

## Entities and reentrant calls

Canonical `Cow` embeds `AbstractCow -> Animal -> AgeableMob -> PathfinderMob ->
Mob -> LivingEntity -> EntityData`. These are the actual 26.3 parents; notably
`Cow` does not extend `Animal` directly. Navigation/goals belong to `Mob`.
`EntityData` is the Rust state name because the virtual-method trait is `Entity`.
Store concrete canonical states in `EntityState` enum variants. Extend the enum
and typed projection when porting a new class; generate mechanical projections
when repetition merits it. Avoid `Any` downcasts and a flat optional-component
bag that admits impossible subclasses.

One stateless `Entity` dispatcher per type implements virtual methods; static
registry table indexes `EntityType`. Each method receives an `EntityRef` and
`&mut Level`, the concrete region-scoped access type. Calls use mutable **handles**, for example
`target.hurt_server(level, source, damage)`. A handle never owns canonical data.
`Level` supplies `level.entity::<Cow>(id)` / `entity_mut::<Cow>(id)`;
`Cow::as_living_mut()` and `EntityState::as_living_mut()` provide parent views.
Methods copy scalars/handles, end the borrow, invoke world/virtual methods, then
reborrow. Methods returning owned query result lists preserve vanilla's capture
point; do not iterate borrowed entity storage across callbacks.

Port subclass overrides into its dispatcher. An inherited method calls its
parent's free implementation with the **same derived dispatcher**; a virtual
call inside that helper uses that dispatcher, while a Java `super.foo` explicitly
selects the parent's implementation. Never call an extracted parent's dispatcher
for `super` if that would lose virtual overrides. A class data accessor is not
virtual. Mutable dispatcher fields are forbidden: they would break recursion.

The skeleton ports the unarmored cooldown branch of `LivingEntity#hurtServer`:
cooldown >10 ignores damage <=lastHurt, otherwise applies only the increase;
outside that window it resets cooldown/hurt timers and applies full damage.
Its test override reenters the attacking entity during the victim's damage
method, then places/schedules a block. Both frames observe the same canonical
health. The fixture's damage reflection is a borrow-model stress case, not a
claim that cows attack, reflect damage or place blocks in vanilla.

Whole-entity take-out leaves the active actor missing or snapshots stale;
restoring it overwrites callback mutations. Deferring all damage changes return
values and timing. Arbitrary `&mut Cow` plus unrestricted `&mut Region` cannot
alias safely. Short state borrows and stateless dispatch avoid all three, with
no `unsafe`, interior-mutability graph or simulation locks.

`EntityRef` is server-lifetime, never-reused identity, distinct from vanilla's
network integer ID and UUID. On removal mark immediately, stop ticking/tracking,
retain data until active call frames finish; retire at entity management. Full
port adds `RemovalReason` and its save/destroy rules. Cross-dimension teleport
uses the **server-exclusive lane at the call site**, not end-of-phase deferral:
vanilla returns the destination entity immediately, recreates ordinary entities,
and treats players specially. Preserve passenger recursion and invalidate old
handles on recreation; players retain the identity vanilla retains.

## Blocks and level access

`BlockBehavior: Sync` has vanilla virtual methods with default vanilla base
behavior as each is ported. Immutable static tables map `Block` to class
objects; constant class parameters may live there, world state may not.
`BlockState` stays a compact copyable registry ID. No object per block position,
no new Rust enum duplicating generated registry facts. Unsupported behaviors
are errors until implemented, never silent base-block behavior.

`Level` is the live access surface; layer read-only `BlockGetter`/`LevelReader`
views when callers need them. Hot block/fluid reads and writes are concrete,
inlinable methods, including typed entity access. Adapter differences live inside
`Level`; only entity/block behavior tables use dynamic dispatch. Tests use the
same concrete access type. `Level#setBlock` ports flags, neighbor update order, block
entity replacement, heightmaps, lighting, POIs and tracking. The foundation
explicitly exposes only **`set_block_state`**, the lower-level storage mutation;
it is not a complete placement implementation. Return missing-ownership errors
in the foundation; server's live adapter resolves them synchronously.

Block entities use the same state/behavior separation, a `BlockEntity` trait
and positional handles. Chunk maps own canonical data; region ticker lists
own handles/rebindable wrappers, not copies or raw references. Validate current
state/type and removal on every tick. This preserves `LevelChunk` replacement
and `Level#tickBlockEntities` pending-list semantics without borrow aliasing.

### Block entity identity

`BlockEntityRef { pos, generation }` carries a per-creation identity, following the
same principle as `EntityRef`. `level.block_entity_at(pos)` returns the current
ref. Tickers and callbacks keep the ref they were given. When a block entity is
removed or replaced, its state is detached from the chunk: it is no longer
ticked, saved or synced. It stays reachable through refs that are still held,
until the phase or callback stack holding them unwinds. A running callback that
writes through its old ref changes the detached state, exactly like Java's
retained receiver object, and never the replacement. Ticker wrappers re-bind by
generation.

### Worldgen shares block behavior

Vanilla block methods take `LevelReader`/`LevelAccessor`, which both
`ServerLevel` and `WorldGenRegion` implement, so worldgen runs real
`updateShape`, `canSurvive`, `placeLiquid` and block-entity loading. Ferraloom
keeps one implementation of each behavior in `ferraloom-game`:

- Methods that vanilla calls on both kinds of level take a `LevelAccessRef<'call, 'chunks>`
  enum (`Server(&mut Level)` / `Worldgen(&mut WorldGenRegion)`). Read-only
  methods use `LevelReaderRef<'call, 'chunks>` (`Server(&Level)` /
  `Worldgen(&WorldGenRegion)`). The call borrow and chunk-neighborhood lifetime
  are independent. It is
  matched statically, so block code still inlines and no `dyn Level` appears
  on the hot path.
- `ferraloom-worldgen` cannot depend on game, so it calls those behaviors
  through `region::WorldgenBlockBehavior`. The game crate implements that trait
  by delegating to the same `BlockBehavior` table with
  `LevelAccessRef::Worldgen`. Until a behavior is ported, the trait returns an
  explicit `Unsupported` error and the affected worldgen parity cases stay
  pending.
- Pure state transforms (rotate, mirror) are generated data tables in
  `ferraloom-data`, extracted from vanilla's own methods.

Live configured features use `LiveWorldGenLevel` implemented by the concrete
`Level` at the cold feature boundary. `Level` construction requires an immutable
`Arc<LevelContext>` containing the actual WorldOptions seed, generator sea level,
generator and configured-feature registry. Tests supply explicit fixture context;
production has no missing-context state or default services. A canonical
`FeatureRandomSelector` resolves short borrows of `Level#random` between world
callbacks, so placement never takes, replaces or copies that stream.

## Global state, commands and messages

The global phase is a sequence of the serial slots above, not a second tick
that moves all global effects to the end. Time/weather/border and raids are
dimension-owned; scoreboard, command storage, game rules, functions and server
clocks are server-owned. Publish immutable read views between phases. Any
same-phase write/read-after-write of these resources takes the appropriate
exclusive lane; buffering an increment cannot emulate a command return value.

`Commands` uses the Brigadier dispatcher and 26.3 execution-context queue.
`CommandSourceStack` is an owned value with entity handle, dimension, position,
rotation, permissions, anchor, result callback and signing context. `withEntity`
does not implicitly replace position or dimension. Selectors and fork expansion
use vanilla order and live data at the same execution step.
`/tp @a` and `/execute as @e` enter server-exclusive access at their original
packet/function/block/entity call site. Evaluate a selector at vanilla's capture
point, execute branches in order, and make results visible to later branches
and commands immediately. Never fan them out into next-tick regional messages.

Messages are reserved for work that already has an asynchronous contract:
completed jobs, network ingress and safely isolated ownership handoff. Envelopes
carry tick/phase, origin sequence, handle and ownership epoch; route through the
current directory, reject stale epochs, deliver once at the receiving tick's
start and integrate at the declared vanilla phase.
Ordinary movement reaching a dependency edge merges synchronously; transfer
between truly independent owners can use next-tick handoff only after the
source's final eligible tick. A newly discovered synchronous dependency always
uses merge/exclusive access. It never becomes a one-tick-late message.

## Players, network tasks and background jobs

Async connection tasks own sockets, framing, protocol transitions, decode,
compression and cipher state. They emit typed packets with a per-connection
sequence into server ingress. Server gives accepted arrivals a global ordinal
and drains at vanilla's packet-processing points; apply each connection's FIFO
without parallel reordering. Tick code does movement validation/inventory/world
mutations through the current player owner. A transfer carries an ingress epoch
and next sequence so queued packets cannot hit the old owner or pass each other.
Do not process play packets on IO threads, or conflate connection tick/doTick
with the player's entity-phase tick.

Region effects append owned outbound commands to tick buffers; a connection
writer receives ordered batches after phase synchronization. Chunk snapshots
are captured at vanilla's chunk-send point, respect tracked status/light/biomes,
batch acknowledgment and chunk budget (`PlayerChunkSender`), then encode away
from the simulation. Assign send sequence **before** parallel encode/compress;
one writer restores that order. Encryption is stateful and serial per stream,
after compression, with no reordered cipher jobs. Buffers have explicit limits:
slow connections are disconnected at an ordered network boundary, never block
the region or silently lose gameplay packets. Suspend/resume flushing remains
an ordered batch boundary.

Separate off-tick pools: worldgen status DAG; lighting; chunk/entity IO and
serialization; navigation; packet encode/compression; network tasks. Jobs receive
immutable snapshots/owned inputs, deterministic request IDs, required phase,
chunk/owner epochs and relevant versions. They return owned results, never
mutate live state. Integrate in request order at vanilla's polling point;
discard invalid results or recompute. Physical completion order never picks
entity insertion or tick order. Save jobs serialize captured snapshots and
honor flush/durable IO completion separately from capture.
For a synchronous vanilla query, including path creation or chunk load, wait at
that call's visibility point (and recompute if invalidated). Do not accept a
"path next tick" shortcut. Workers can compute earlier, but concurrency does
not justify a different return value or a tick-dependent timeout fallback.

## Randomness, identity and accepted differences

In region mode each region owns deterministic forks of the dimension's
`Level#random`, `randValue` position LCG and sound seed source. Use the same RNG
algorithms and Java math. Creation/split consumes forks from the parent in stable
region-ordinal order. At merge each old region forks once in ordinal order;
the merged region takes the lowest-ordinal parent's fork and discards the others.
Random/sound streams use their algorithm's `fork`; LCG children advance the
parent's vanilla recurrence once and take that resulting signed word as their
initial state. Fork events and source states are persisted in replay/checkpoint data;
worker placement and job completion never choose a fork. No RNG operation takes
the exclusive lane. Per-entity RNG lives in canonical `EntityData` and moves with
it unchanged. Worldgen retains its own seeded/positional streams.
`--regions off` leaves original streams and their consumption order untouched,
without even a fork when the one reference region is created. Vanilla initializes
some streams from nondeterministic seeds; exact seeded comparisons require the
same controlled seed inputs in both harnesses, not merely the same world seed.
Worldgen fixes FEATURES visits in request order: native asynchronous readiness can reorder serialized visits and change blocks.
Controlled native traces prove this order dependence; BIOMES/TERRAIN must remain order-independent, and parity fixes the same legal FEATURES order.

In region mode the server allocates network-ID blocks on demand at phase
boundaries in region-ordinal order; each region allocates locally from its block
and keeps unused IDs across phases. Exhaustion requests another deterministic block
through the exclusive lane, never races an atomic counter. Single-region mode
uses vanilla's original counter at the original allocation step, with no ranges.
Clients treat IDs as opaque; the parity harness normalizes them. UUID creation
follows vanilla independently. Persist/transfer handles and IDs separately;
merge/split never renumbers an existing entity.
`getId()` remains the network entity ID, including in `Mob#serverAiStep` cadence.
There is no second scheduling identity. Deterministic ranges preserve the
distribution of staggered AI work, but change which actor receives which cadence;
that is an accepted region-mode difference. Packet-ID normalization alone does
not erase it. Single-region IDs and AI cadence remain exactly vanilla's.

The admitted parallel difference is **within-phase cross-region order**. Its
observable consequences include multi-region selector/read results and packet
interleaving. The explicitly accepted parallel mechanisms also change RNG draw
assignment through deterministic regional forks, which region gets the last
near-cap spawns through quota partitioning (including quota underuse), and opaque
network IDs and per-actor AI cadence through reserved ranges. These are deliberate compatibility
relaxations approved by the coordinator, not bit-exact parity claims.
Connected contraptions merge and keep vanilla local order, though region
creation/merge/split can change the RNG stream they consume.
Exact total server order requires `--regions off`. Worker count/completion order
must not add additional nondeterminism for the same accepted input trace.

Not acceptable: lost neighbors, stale actor snapshots, different math/RNG
algorithms, duplicated/lost ticks or packets, multiplied dimension tick budgets
or global spawn caps, changed portal return semantics, delayed synchronous commands,
or off-tick results applied according to wall-clock races. Both modes run parity
scenarios; ordering-sensitive cross-region scenarios record the known relaxation
explicitly instead of weakening ordinary vanilla assertions.

## Porting milestones and remaining risks

1. Review this ownership/dispatch design and compile the foundation. No behavior
   inventory claims: the foundation is not a complete mob or block feature.
2. Shared chunk leaf is implemented and real-jar-vector tested. Port chunk status/tickets, live Level
   mutation/neighbor order and serialization; wire single-region server phases.
3. Player/connection split, ingress/egress, tracking/chunk sends, login and basic
   playable interactions. Bring up vanilla parity before adding broad gameplay.
4. Entity common/living/mob/navigation state and virtual helpers, then subclass
   families; block/fluid scheduled ticks, events and block entities. Expand
   from focused vanilla-backed scenarios; port commands/functions/global state.
5. Worldgen/light/IO jobs and synchronous integration; remaining gameplay,
   GameTests, raids and all dimension-specific systems.
6. Implement barriers, exclusive continuations, closure merge/split and routing;
   enable regions only with both-mode parity and measured useful parallelism.

Risks to settle during those ports: exact Java collection traversal, retained
handles in deep callback stacks, command/global-state frequency collapsing
parallelism, unbounded dependencies becoming giant components, and memory held
by pending snapshots. Exclusive-lane continuations and priority ordering need
deadlock tests in server. The foundation deliberately has no scheduler, block
entities, loaded saved ticks, chunk ticking eligibility, full damage pipeline,
AI or packet queues. Those must be ported, not inferred from the tiny scaffold.

Source basis: `MinecraftServer`, `ServerLevel`, `Level`, `ServerChunkCache`,
`ChunkMap`, `DistanceManager`, `TicketStorage`, `LevelChunk`, `LevelChunkSection`,
`PalettedContainer`, `Entity`, `LivingEntity`, `Mob`, `PathfinderMob`, `AgeableMob`,
`Animal`, `AbstractCow`, `Cow`, `EntityTickList`,
`PersistentEntitySectionManager`, `BlockBehaviour`, `Block`, `BlockState`,
`BlockEntity`, `LevelTicks`, `LevelChunkTicks`, `ScheduledTick`, `ServerPlayer`,
`ServerGamePacketListenerImpl`, `ServerConnectionListener`, `Commands`,
`CommandSourceStack`, `PathNavigation`, `ServerExplosion`, all from the shared
decompiled 26.3 source. No decompiled code or Mojang assets are included here.
