# Ferraloom server core

Ferraloom replaces Horizon's server loop and ownership model, retaining its
vanilla logic and differential harnesses. The base is Horizon v0.2.93, not the
old `ferraloom-*` implementation on `restart`. The old
[game-core design](game-core.md) remains the detailed reference for synchronous
callbacks, region dependencies and accepted parallel-mode differences; its crate
names and foundation status describe the old port, not this checkout.

## Boundaries in this checkout

Today `horizon-server` supervises `horizon-client --dedicated`.
`horizon-client/src/game.rs` owns both simulation and presentation.
`game/levels.rs::Parked` moves dimension fields in and out of `Game`;
`game/lan_actions.rs::as_guest` does the same for players. Neither mechanism is
a suitable unit of parallel ownership. Sharing `Game` behind locks would retain
these dependencies and serialize the work we need to parallelize.

The target dependency graph is:

| Crate | Responsibility |
| --- | --- |
| `ferraloom-core` | Server execution policies independent of the client loop; chunk integration budgets introduced by this milestone |
| `ferraloom-game` (target) | Canonical server, level, region and player state; gameplay adapters and phase methods extracted from `Game` |
| `ferraloom-runtime` (target) | Tick coordinator, region workers, background jobs, network and persistence boundaries |
| `horizon-server` | Existing CLI/configuration/supervision; ultimately starts the server-only runtime |
| `horizon-world` | Existing worldgen, NBT/region codecs, block behaviours, ticks and entity rules |
| `horizon-core`, `horizon-items`, `horizon-command`, `horizon-protocol`, `horizon-server-files` | Existing maths/RNG/palettes, item rules, Brigadier parsing, wire codecs and server files |
| `horizon-mods` | Existing sandboxed WebAssembly plugins; gameplay calls use the same ownership/exclusive access rules |

These are target boundaries, not empty crates to create in advance. Runtime
depends on game; game never depends on sockets or the worker scheduler. Neither
new crate may depend on `horizon-client`. Move reusable server modules out of
that crate and re-export their old paths for Horizon callers. Keep existing
registry IDs and `horizon_core::paletted` storage, rather than rebuilding the old
port's data/util/chunk crate graph.

`horizon-world::level::Behaviours` already accepts `LevelAccess` rather than
`Game`. Extract `GameLevel`/`with_level` into a server-owned adapter implementing
that interface. Preserve its immediate mutation/event semantics, including
heightmaps, lighting, block entities and neighbor callbacks. Library-only mob
rules can move mechanically; `game/creatures.rs` orchestration, commands,
containers and player actions still need extraction. Move their real state and
methods in small units, not a parallel implementation of vanilla. Presentation
hooks become consumers of owned effects, never dependencies of the server.

Separate immutable gameplay resources from `Resources`/`PackResources`: block
facts, registries, recipes and collision data remain shared `Arc` values;
renderers, baked models, audio, input, HUD and shaders stay client-only. The
current `--no-default-features` build disables audio/controllers but still links
client libraries; that is an interim build requirement, not the final boundary.

## Extraction progress

The first step-2 unit is `ferraloom-game::{chunk, collisions}`. It owns the
existing copy-on-write `Section`/`Light` storage, palette configuration and
borrowed block handle, plus the real `SceneCollisions` adapter used by gameplay.
`ResidentWorld` supplies live borrowed sections, block reads and current piston
state through static dispatch. Horizon's existing names re-export the shared
implementation; its `SceneCollisions` alias selects the current `World`.
The same crate now owns `ChunkTickets`/`Levels` and the unchanged fast hasher.
Horizon reexports those old paths too; ticket propagation, map representation
and iteration are unchanged. The crate depends only on `horizon-core` and
`horizon-world`.

Scheduled-tick registration membership and its waiting-for-eligibility state
also live in `ferraloom-game`. A borrowed `LoadedTickChunks` view supplies the
resident key token, membership lookup and original key traversal without owning
chunk payloads. The client adapter retains integration tests against real
resident chunks and saved block/fluid queues. The actual queues and behavior
machinery already live in `horizon-core::ticks` and `horizon-world::level::Ticks`;
this extraction preserves their admission, callback boundaries and unpack time.

The first canonical `ServerLevel` unit owns those block/fluid queues, scheduled
membership and tickets together. `Game` holds the active value; `Parked` holds
each inactive value. Dimension changes transfer that owner intact instead of
swapping its individual fields. Gameplay borrows its existing queues directly;
there is no second scheduler, copied state or changed callback boundary. The
resident world, random-section cache and entities join this owner in subsequent
units. Player access is a separate extraction; guest swaps are still present.

Random-section eligibility and its weak-reference invalidation cache now live
in `ferraloom-game` too. The resident section owner and the random-tick block
predicate are borrowed inputs; the client wrapper only supplies its existing
`Chunk` and `BlockFacts`. The scan, weak-reference identity checks, section
order and live rechecks between lower and higher sections are unchanged. The
real-chunk COW/reload and evolving random-tick trace tests stay at the adapter.

Player ownership preparation removes `PlayerMotion`'s boxed identity callback.
Its concrete `IdentityCounter` keeps the same per-player wrapping `i32` sequence;
native replay retains the exact recorded values and allocation-count check.
The only shared replay state is an atomic count, not a gameplay lock. Existing
closure-based world harnesses keep their binding through `IdentitySource`.
The player body's `Send` bound is checked in its unit tests. This does not yet
remove guest swaps or move network-owned state; that access migration follows
the LAN branches' merge and retains immediate reentrant visibility.

That bound also exposed `ObjectOpenMap`'s `Rc<RefCell<Table>>`. Its current table
is now owned directly. Active `for_each` frames follow its live key array until
rehash, then pin that generation's retired keys with an immutable `Arc`; values
still come from the current array. Nested frames share a generation, and a
borrowed guard releases pins on return, error or unwind. Ordinary lookups use
the owned table directly, without a lock or generation lookup. The collection
ownership change has a separate reentrant-rehash/parity gate and a before/after
attribute-lookup benchmark; a slowdown above about 2% blocks it.

`Chunk` and `World` remain in Horizon for now: their block-entity types have
inherent implementations across containers, sculk, spawners and other gameplay
modules. Move those types with their methods in later units, rather than adding
duplicate state or client dependencies to the server crate. `GameLevel` and
`with_level` likewise move subsystem by subsystem. This unit changes no tick
order and does not enable region workers.

Extraction commits preserve the existing serial trace. Confirmed discrepancies
with vanilla's phase order are corrected in separate commits, each with a
vanilla-backed case, before enabling the corresponding parallel phase.

### Deferred read-phase candidate

`SpawnState::new` could prepare loaded-chunk, biome spawn-cost and nearby-player
inputs concurrently over immutable sections/player positions, then apply them
in original entity order at `NaturalSpawner.createState`'s vanilla position.
Charge insertion and floating-point evaluation order, RNG and spawn mutations
must stay unchanged. This is a candidate, not an implemented phase.

The CT 121 profiles do not justify it now: construction accounts for **1.04%**
of captured tick CPU samples at 500 players (208 / 19,932,
`profiles-500p/profile-1793340296`) and **0.94%** at 100 explorers (256 / 27,293,
`profiles-100p/profile-1793267506`). It is absent from the `.txt` top-40 lists;
these inclusive counts come from their matching `.svg` constructor frames,
normalized by the text reports' tick-thread sample totals, not all threads.
Keep regions as the priority and reconsider this candidate only if its share
reaches about 5% of the tick.

## Live ownership

`Server` owns global clocks, functions, scoreboard, game rules and dimensions
in vanilla order. Each `ServerLevel` owns weather/border, tickets and distance
management, dimension-wide spawn/tick budgets, region routing and region values.
A `Region` owns its chunks, canonical entities and spatial indexes, insertion
ordered tick membership, block entities, scheduled block/fluid queues, neighbor
stacks, block events and dirty/effect buffers. A lookup map is not a tick order.
Loaded status, block-ticking eligibility and entity-ticking eligibility remain
separate. Players are canonical game entities; connections only hold handles.

Regions are connected dependency closures, not fixed squares. Start with the
old design's eight-chunk margin, then enlarge/merge for actual navigation,
movement, explosions, loaded neighbors and retained references. The margin is
not a correctness proof. A cross-owner synchronous operation parks at its call
site and enters the exclusive lane; a missing owner is never air. Merge keeps
entity/tick ordinals and pending queues. Split only after a complete tick when
no active callback, passenger tree or job pins both sides. Use short canonical
state borrows and handles across reentrant callbacks, never take out an actor
and later overwrite callback mutations with its stale state.

The extracted driver will provide `--regions off`: one reference region with
vanilla RNG draws, entity IDs,
collection traversal and immediate callbacks unchanged. Parallel mode retains
local vanilla order and the documented regional RNG/ID/spawn-quota differences
in `game-core.md`. It cannot claim bit-exact total ordering across independent
regions. Worker completion order must introduce no further differences.

## Tick driver and barriers

Retain vanilla's gates for frozen, debug, empty and paused worlds. Do not infer
the target sequence from Horizon's presentation-oriented `tick_world` alone;
check 26.3 `MinecraftServer`, `ServerLevel` and `ServerChunkCache` directly.

1. Apply ordered ingress at vanilla's packet-processing point. Run server
   functions/clocks and time synchronization in their original serial slots.
2. For each dimension in vanilla order, update border/weather/sleep/time;
   collect and execute scheduled block ticks, then scheduled fluid ticks.
   Grant each queue's 65,536 dimension-wide budget in reference order.
3. Tick raids; update chunk tickets/distances; build dimension-wide spawn state;
   tick spawning chunks, weather/random blocks and custom spawners; run tracking
   and chunk-management subphases at their original positions.
4. Drain block events, then eligible entities/passengers and block entities;
   integrate entity loads/unloads. Preserve captured membership and live removal
   checks, including pending block-entity tickers.
5. After all levels, run connection/player-list work and chunk sends, resume
   flushing, then status/autosave/statistics in their vanilla slots.

Each regional subphase joins before the next starts. Dimension/server work
occupies serial slots between these barriers. No independent regional clocks.
A worker requesting exclusive access returns ownership while retaining only
handles/scalars in its continuation. Other workers finish or park in the same
phase. The coordinator serves requests in stable phase/region/operation order,
lends the required regions, and resumes the caller at its original visibility
point. Teleports and multi-target commands cannot become next-tick messages.
Implement and test this continuation protocol before enabling parallel regions;
a mutex around the world is not its substitute.

### Stack carriers and live adapters

Use blocking Rust stacks on OS threads for the first continuation driver.
The normal pool has `worker-count` runnable threads; each runs region jobs in
sequence on its own stack. Do not pin one thread to each region or wake a
thread for each job. When a job parks, its carrier remains blocked with that
call stack, returns canonical ownership and releases its runnable slot. An idle
spare, or one grown lazily from the bounded spare pool, takes over the slot.
Thread demand is the worker count plus currently parked stacks. With no parks,
only the normal workers wake for the phase. Pool synchronization lives in the
runtime, outside simulation state.

The coordinator waits until all phase jobs finish or park, then serves exclusive
requests in stable phase/region/operation order and resumes the original call.
It never chooses a winner by arrival time. Bound region count and therefore
parked-stack/spare demand through deterministic conservative coalescing.
Coalescing and tie-breaking use only simulation state and stable region
ordinals, never worker count, measured cost or completion timing. The carrier
and phase contracts must also serve block entities, scheduled ticks and mobs.

The concrete adapter may keep one private `RefCell<Option<Region>>`, owned by
the active job on its worker and never shared between workers. Small helpers
return owned answers/handles and end their state borrows before invoking a
callback or parking. No borrow guard or reference into canonical region state
may escape a helper. A reentrant borrow panic is a bug, not a serial-fallback
signal. Resident chunk reads return owned `Arc` handles through
`RandomTickChunk::get_owned`, retaining the existing weak-cache fast path.

Resolve the separate canonical `Ticks` borrow before enabling workers. A
`TickAccess` facade performs short scoped borrows of the same live tick state;
legacy `Ticks` can implement the facade while existing callers migrate. Active
behaviour frames retain the facade and handles/scalars, not `&mut Ticks` or a
queue borrow across a world callback. Reentrant callback/handoff tests must
observe mutations of that same canonical tick state after resumption.

Gate the adapter cell with a paired block-read hot-path benchmark, using the
same compiler/profile/input and alternating before/after trials as the
attribute lookup gate. Reject a regression above about 2%; use the wider
`&mut` read-API conversion only if the cell fails this bar. Include a focused
reentrant callback test. Neither this design nor a passing type assertion
claims that the adapter or continuation driver has been implemented yet.

With worker coverage, report wakeup overhead, parked-thread high water, RSS and
the share of regions coalesced by the cap. Include a two-core serial control:
regional execution must not lose to serial there. Keep a serial-slot fallback
when dispatch is slower, chosen before phase work and preserving the mode's
RNG, ordering and callback semantics. Execution policy must not change the
simulation-driven coalescing decisions or worker-count determinism.

## Work outside the tick

Capture chunk-send/save inputs at their original visibility points. Existing
resident sections are copy-on-write `Arc` snapshots (`chunk.rs`); preserve that
property on extraction. Worldgen, lighting, encoding and persistence jobs own
inputs and return results tagged with request order, owner epoch and versions.
Integrate at the declared phase in request order, validate versions, and discard
or recompute stale results. A synchronous vanilla query still waits at its call
site. Per-region integration budgets may postpone asynchronous readiness, never
a required synchronous load or an already eligible simulation tick.

Connection workers already frame/compress/encrypt in `lan/conn.rs`; `save.rs`
already snapshots and writes chunk records on background workers. Reuse those
mechanisms. Extend the pipeline only where work remains on the simulation
thread. Assign packet sequence before parallel encoding and preserve each
connection's FIFO, batch boundaries and acknowledgments. Encryption stays serial
per connection. Bound queued snapshot bytes and account for slow clients without
blocking simulation indefinitely. Save capture and durable flush are distinct
completion contracts. Saving or disconnecting must drain the appropriate jobs.

## First shipped step

Introduce dependency-free `ferraloom-core::{IntegrationLimits, IntegrationBudget}`
and adapt `Streaming` to publish a bounded prefix of completed worker results.
Doc's pinned explorer measurements are the reason: increasing workers raised
MSPT from 46 to 86 to 154 ms while opening more terrain. Worker throughput must
not decide how much simulation work the tick takes on.

The initial dedicated-server policy permits two completed columns and 2 ms of
result integration per dimension per tick. `tick_server` replenishes it once;
between-tick pregeneration pumps share the remaining allowance. Limits are both
time and admission count: cheap insertion can still open expensive terrain.
Completions are atomic, so one may overrun the time allowance. At least one
completion can progress each tick. Results keep their original polling snapshot
and FIFO; the unconsumed suffix stays owned for later ticks. No result is dropped
or moved ahead of another, and decoration's existing dependency/order queue is
unchanged. Already-loaded eligible chunks, scheduled ticks and entities still
run normally; this limits asynchronous publication, not gameplay ticking.

Initial world startup retains synchronous/unbounded loading. Explicit saving
and closing drain retained generation/save completions outside the admission
budget, then honor the existing durable write barrier. The generation/save test
runs both modes, pumps twice per tick, moves/unloads/reloads areas and restarts,
checking that each chunk finishes once with every neighbor decoration intact.
Core tests cover carried allowance, atomic overruns, count limits and disabled
reference behavior.

Admission is explicit at each streaming pump: `Streaming::update` is unbounded
for synchronous/client work, while `update_budgeted` consumes the allowance
retained by `begin_server_tick`. `tick_server` and between-tick pregeneration
select the latter and assert that selection in debug builds. Synchronous pumps
neither spend nor replenish the allowance for later bounded pumps. The command
harness freezes its clock while settling terrain; inheriting a spent allowance
there previously prevented progress forever. Real single-player play was not
affected: its loop renews the allowance every server tick, startup precedes the
first budget, and saving already drains independently.

On Linux, `horizon-threads` captures the launcher's allowed CPU mask before
workers start. The tick chooses its lowest logical CPU; streaming, save, encode
and network helper lanes exclude that CPU and its SMT siblings. All masks stay
inside the original `taskset`/cgroup intersection. Pool sizes also respect the
CPU quota, captured before the tick thread is pinned. Startup workers keep their
existing mask until selecting a lane; the tick pins when its loop starts. An
unknown topology, single physical core or single-CPU quota leaves affinity unchanged. OS calls
stay in Horizon's existing unsafe OS boundary, not in simulation/core policy.

`FERRALOOM_INTEGRATION_US=0` disables admission limits for the reference path;
`FERRALOOM_CHUNKS_PER_TICK` changes the column allowance (default 2), and
`FERRALOOM_TICK_AFFINITY=0` disables CPU separation. These controls support
same-binary comparisons. Ordinary client streaming keeps its previous behavior.
This budget cannot make already-overloaded simulation cheap: parallel regions
remain necessary. Report TPS together with delivered/generated chunks and
pull-backs, so throttling terrain is explicit rather than a disguised win.

This milestone starts with the region coordinator's admission policy while
retaining Horizon's world logic and serial execution order.

## Migration with a gate at every step

Each row is a sequence of small commits/PRs, not permission for a broad rewrite.
Keep the dedicated server playable through the existing entry point until its
replacement passes the same end-to-end tests. The revised M2–M4 path reaches a
first regional phase before completing the executable/runtime split or every
player adapter. The driver borrows actual owned simulation state; regions are
not miniature `Game` shells. Extract only the state and adapters required by
the next worker phase, retaining all other phases in their existing serial slots.

| Step | Deliverable | Required gate |
| --- | --- | --- |
| 1 | Bound chunk publication per tick and keep workers off the tick core | Budget/topology tests, both-mode generation/save invariants, affected parity and pinned explorer/normal-play bench |
| 2 | Extract the minimal block-phase kernel, real state ownership and its live adapters; retain the existing executable and tick loop | Focused affected parity, standalone game tests and client compilation; unchanged serial trace and immediate effects |
| 3 | Introduce the reusable one-region phase driver, exclusive continuations and dependency closures with parallelism disabled | Exact reference trace; reentrant mutation, removal/replacement, teleport, commands and ownership-boundary tests |
| 4 | Enable regional random block ticks behind a flag, joining at their original phase boundary; other phases remain serial | Flag-off exact reference; flag-on worker-count determinism, split/merge/movement and both-mode parity; measured worker coverage on all three workloads |
| 5 | Reuse the kernel/driver for block entities, scheduled ticks, then mobs and spawning, extracting their dependencies as needed | Per-phase parity and ordering gates, immediate callbacks, dimension-wide budgets and measured useful parallelism |
| 6 | Finish canonical player access and server-only runtime/startup as their dependencies require, without delaying the first independent worker phase for unrelated client cleanup | Player-action/portal/entity parity and playable login/exploration/save/reload; eventual build without renderer/audio/input dependencies |
| 7 | Add parallel sensing/navigation and nonoverlapping decoration behind the same job contract | Apply in vanilla order, preserve synchronous answers and FEATURES visit dependencies; differential cases and explorer/farm bench |

### Phase rollout and useful coverage

Random ticks prove ownership and the driver, not the performance payoff. The
500-player whole-server profile assigns at most 9% to all block work together;
random ticks are only part of that bucket. Amdahl's limit prevents this first
phase alone from delivering a substantial TPS gain. Keep phase dispatch,
ownership closures, exclusive requests, stable ordinals and barriers reusable
by the following phases; do not build a crop-specific scheduler.

| Order | Regional phase | Measured tick share at 500 players |
| --- | --- | --- |
| Proof | Random block ticks | Unseparated portion of the 9% block-work bucket |
| Next | Block entities and their tickers | About 8% |
| Then | Scheduled block and fluid ticks | Unseparated portion of the same 9% block-work bucket; do not count it again as another 9% |
| Then | Mobs and natural spawning | About 26% combined |

Each phase retains vanilla order and joins before the next. Keep scheduled
block/fluid budgets dimension-wide, spawning counts/quotas at their barrier,
and effects visible at their original callback points. A worker cannot defer
an immediate spawn, drop, neighbor update or actor mutation to the phase join
merely because the caller lives outside the kernel. Supply that dependency or
serve it through the exclusive continuation before resuming the call.

If initial dispatch uses an allowlist, its eligibility proof must cover reachable
callbacks and mutations, not only the initial block states. Unsupported regions
stay serial before dispatch. Never partially mutate a region, abandon its work
and replay it serially. Missing ownership is never air; synchronous reads and
loads must obtain the live owner through closure merging or exclusive access.

For explore 100, explore 200 and the 300-normal/200-explore mix, report both
worker/total random-position draws and worker/total actual random-tick callbacks,
including serial fallback regions in the totals. Also report CPU time spent
executing that phase on workers divided by total simulation-tick CPU time
(coordinator plus regional workers), separately from elapsed MSPT. Attribute
worker CPU to real simulation work, excluding background generation/encoding.
Report ready/merged region counts and worker utilization with those figures.
If the allowlist leaves coverage low, report it immediately; a flag or two busy
threads is not evidence that explorers scale across cores.

With the flag off, preserve the exact serial reference trace, RNG and callback
visibility. With it on, run the same fixtures repeatedly at worker counts 1, 2,
4 and 8 and compare simulation traces and ordered effects, including exclusive
requests and different completion orders. Only the already approved regional
differences may remain; worker count/completion order adds none.

LAN remains separately owned and outside this migration scope. Its measured
34% serial share at 500 players, reported as about 60 ms, already exceeds the
entire 50 ms tick budget. Regional simulation gains therefore cannot alone
establish the complete 500-player/20-TPS target. This document makes no LAN
parallelization plan.

Research speculative execution for touching regions separately after these gates;
it needs validated dependency tracking and rollback before it can execute live
vanilla mutations.

Before each change, obtain the actual affected set with
`python3 tools/parity.py --changed-files <path-list> --list`; do not pin the
historical count of harnesses. Run Rust builds/tests only through
`ferraloom-build` from a clean committed checkout. Prepared resources and vanilla
oracles remain outside the public tree. Performance acceptance uses
`tools/ferraloom-bench` and the approved pinned CT 121 matrix on port 25570,
with bots on devbox and the builder idle. The original VM/laptop-relay setup
remains available. Report mean/P95/P99 MSPT, TPS, delivered/full chunks,
pull-backs, CPU and busiest thread against `4a661f939`. Report the cost of
reduced terrain admission alongside the simulation gain; farm correctness
remains mandatory.


## Initial measurements

Pinned CT 121, Xeon E5-2660 v3; seed 12345, view/simulation distance 10,
60-second warmup and 180-second measurement, bots on devbox. This branch
submitted no builds during measurement. Separate profiling builds overlapped
the initial later runs, on disjoint builder CPUs `2-3,10-12,14`; shared-host
cache/clock noise remains possible for normal play and the four-column trial.
The four-core explorer run finished before that overlap; SMT exploration was
repeated with the builder reserved and idle for the entire run.
Explorers fly at 11 blocks/s. Each cell is one run, not a confidence interval.
Baseline is `4a661f939`; the budgeted build is `b84aad467` for four-core
exploration and normal play, and `65fca9e64` for the final SMT explorer run.
The latter includes quota-aware pool sizing and retains the same 3/6 helper
counts for these CPU sets.

| Workload | Baseline mean/P95/P99 ms | Budget mean/P95/P99 ms | TPS before → after |
| --- | --- | --- | --- |
| Explore 25, 4c | 86.17/104.48/119.65 | 57.83/66.57/75.14 | 11.60 → 17.29 |
| Explore 25, 4c+SMT | 154.06/178.76/196.80 | 58.81/68.93/77.49 | 6.49 → 17.01 |
| Normal 50, 4c | 26.61/37.41/48.01 | 29.10/41.17/51.33 | 20.00 → 20.00 |
| Normal 50, 4c+SMT | 26.67/40.05/50.11 | 27.65/38.44/47.23 | 20.00 → 20.00 |

Delivered counts are chunk packets, including initial joins; rates use the
measurement window. Full chunks are counted in region files after shutdown.
Pull-backs are correction events, not distinct players. CPU/busiest are mean
percentages; 100% means one logical CPU.

| Workload | Delivered/s (total), before → after | Full, before → after | Pull-backs, before → after | CPU/busiest %, before → after |
| --- | --- | --- | --- | --- |
| Explore 25, 4c | 117.13 (52,983) → 71.84 (37,587) | 8,886 → 9,590 | 24,763 → 31,336 | 342.19/98.88 → 228.82/98.77 |
| Explore 25, 4c+SMT | 150.96 (62,840) → 71.36 (38,005) | 9,659 → 9,685 | 13,631 → 31,541 | 603.86/99.53 → 304.60/99.54 |
| Normal 50, 4c | 316.22 (107,282) → 316.20 (107,049) | 2,188 → 2,188 | 0 → 0 | 93.78/53.26 → 100.99/58.68 |
| Normal 50, 4c+SMT | 316.32 (107,277) → 316.19 (107,076) | 2,188 → 2,188 | 0 → 0 | 113.53/53.60 → 116.01/56.01 |

Two columns reduce explorer mean MSPT by 33% at four cores and 62% with SMT,
while delivered terrain falls and corrections rise. Normal play keeps 20 TPS
and its delivery rate, with observed mean MSPT 2.49/0.98 ms higher. The tick
thread remains saturated during exploration; the serial simulation and global
decoration queue still need the later migration steps.

A four-column trial at four cores produced 70.19/81.77/94.41 ms, 14.25 TPS,
92.76 delivered chunks/s (46,605 total), 13,829 full chunks, 28,859 pull-backs,
and 279.40% CPU / 99.89% busiest thread. Keep two as the conservative initial
default for simulation time; four buys more terrain at higher tick cost.
This trades terrain throughput for simulation time. Exploration remains below
20 TPS.
Raw runs remain on CT 121 under `/root/bench/results/core-b84aad467` and
`/root/bench/results/core-b84-count4`; the final idle-window SMT repeat is in
`/root/bench/results/core-65fca9e64-ht-idle`. The baseline is in
`/root/bench/results`.

Validation covers 19 focused Rust/save tests and 13 publication-guard tests,
including carried budgets, CPU topology/quota limits and generation
save/reload/restart in both modes. Scoped vanilla harnesses pass:
`chunk_status`, `chunk_stage`, `generation_tick`, `feature_region` and
`decoration_region`. Both `dedicated_server` and `server_dimension_players`
match vanilla-codec transcripts with limits/isolation enabled and disabled.
The dedicated check uses `--server-only --engine target/release/horizon-client`;
the rendered-client check is omitted. Java-server correctness fixtures can use
RAM-backed scratch to avoid builder storage watchdogs; the performance runs
use CT 121's region files. The complete retained-history artifact audit passes.
File-based selection widens conservatively through `Game` and the client
manifest; this is scoped server validation, not a full client/rendering audit.
