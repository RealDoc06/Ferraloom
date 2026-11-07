# Synchronous live chunk acquisition

Approved single-region design; follows [game-core](game-core.md). Source: vanilla 26.3 `Level`,
`LevelReader`, `ServerChunkCache`, `ChunkLevel`, `TicketStorage`,
`ChunkStatusTasks#full` and `BlockableEventLoop#managedBlock`.

## Caller contracts

| Native call | Required behavior |
|---|---|
| `getChunk` / `getChunkAt`; valid block/fluid reads, `setBlock`, owner-thread block-entity lookup | Synchronously acquire `FULL` with `loadOrGenerate=true`. |
| `getChunk(...,status,false)`; collision/stand-on-face (`FULL`), noise biomes (`BIOMES`) | No ticket. Absent without a status-eligible visible holder; otherwise schedule/await its future. `false` can block. Preserve caller-specific absent branches. |
| `getChunkNow` | Completed `FULL` only; no wait/generation. Off-owner: absent. |
| `hasChunk` / `hasChunkAt` / `isLoaded` | Ticket eligibility for `FULL`, not future completion; no load. `getHeight` retains its initial presence guard and absent-height result. |
| `getChunkForLighting` | Inspect available `INITIALIZE_LIGHT` parent status; no acquire. |

Keep bounds/thread guards, including native out-of-bounds states. Preserve the
four-entry `(position,status)` cache, cached absence and invalidation points.
A `true` cache miss follows the ticket/future path even with an existing holder.
Required failure is an error in Strict and Warn.

## Tickets and completion

On the `true` future path, add/refresh `UNKNOWN` at
`ChunkLevel.byStatus(status) = 33 + FULL-step accumulatedRadius(status)`:
**33 for `FULL`**; use the pyramid for earlier statuses. `UNKNOWN` only loads:
no simulation, persistence or dimension-activity flag. Its signed timeout
starts/resets at 1, decrements at purges even unloaded, and expires below zero.
No removal on return, countdown during waits or refresh on cache hits.

Return on requested-status completion, not confirmed full status or
`world.ready` (ticking requires level <=32). FULL publication checks the request
token, integrates ordered metadata/skips and latest structure revision, runs
entity post-load, preserves pending BE/upgrade/post-processing and saved state,
registers block entities/tick containers, and primes heightmaps. Saved ticks
unpack only in `startTickingChunk` at its game time. Preserve entity input on failure.
FULL does not imply pending
activation, entity IO/visibility or ticking post-processing completed.

Install canonical **center light** before returning FULL: generation finished
`LIGHT`, but `World::integrate` queues another actor import. Await that import
or carry its snapshot; no 3x3 ticking requirement. Preserve revision checks,
attached-FULL protection and initialization versus delta-light semantics.

## Blocking without starving the pipeline

The installed `AcquireDriver` exposes
`request_status(&Level,pos,status) -> Result<AcquireHandle,LevelError>` and
`poll_acquisition_step(&Level) -> Result<AcquireProgress,LevelError>`, plus a
wakeable completion test. The caller applies tickets before dispatch. Each
handle identifies a holder incarnation and its per-status future, independently
of a replaceable generation task. `ChunkMap::schedule_generation_task` reuses
that future; upgrading the target cancels the old task without replacing the
future. Downgrades clear only pending disallowed futures. Each holder owns this
state and retains generation claims until its submitted work settles.
`GenerationRuntime` keeps ordered owned generation, light and entity completions
reachable during callbacks. `await_generation_chunk` delegates to the same
synchronous FULL acquisition and does not wait on `world.ready`.

At the read, pump distance/visible-holder updates and dispatch, then lighting
scheduling and source main-thread tasks, following `MainThreadExecutor#pollTask`.
Integrate in request order; progress earlier requests, reject stale tokens and
propagate failure/cancellation. Wait on wakeups only when idle. Keep completions
in a reentrant queue, not an outer local `Vec`; promotion continuations prevent
nested acquires stranding results or duplicating registration. End mutable
region/chunk/holder/driver borrows before task execution.

The owner runs independently of IO/generation/light executors. Workers consume
owned inputs and never await the suspended tick; the owner pumps promotion
tasks rather than blocking their server actor. Split `integrate_jobs` progress
from eviction, which stays at boundaries. Advance no time, ticket expiry or
packet/random/scheduled phases. Preserve side effects and RNG draws inside
permitted promotion callbacks. These are required new contracts.

Single-region limitation: while a shared Level read is suspended, the managed
pump progresses distance updates, generation, IO, light and shared FULL/entity
publication, but defers mutating `prepareTickingChunk` tasks
(`postProcessGeneration`, `startTickingChunk` and sendSync readiness) to the next
exclusive point in their captured queue order. Vanilla can execute those tasks
inside `managedBlock`, so their interleaving is an explicit parity deviation
until the phase-5 continuation model supplies safe exclusive resumption.
FULL/UNKNOWN33 acquisition never waits on them.

`stage_full_chunk` installs the center and light with `loaded=false`, returning
an opaque Copy stage token and owned entity NBT. The runtime replaces parent
status futures before calling shared entity postLoad without backend guards.
Only below-FULL status requests observe that center. A nested FULL request
returns typed `ReentrantFull`: vanilla would wait forever on the unsettled
future, and no reachable native call chain requiring it is claimed. Now-lookups
remain absent until finalization completes; the token is not a read accessor.

The runtime retains the actual entity error and its Preflight/Publishing stage.
Only explicit Preflight retries are safe; successful entity registration is a
permanent continuation checkpoint. `finalize_full_chunk` resumes native
`setLoaded -> BEs -> block ticks -> fluid ticks -> completion` steps, never
entities. Successful finalization cannot repeat (`AlreadyFinalized`); removed,
recreated or foreign tokens fail. Canonical pending queues survive errors and
exclusive save/unload. Typed `WorldgenSkip` records share the existing policy,
counters and deduplication before the runtime reports completion.

## Ownership in each mode

Single-region runtime ownership: Level owns one `Box<dyn AcquireDriver + Send>`.
Cold `as_any` access exposes its concrete `GenerationRuntime`, which owns the
canonical `World`, `ChunkMap`, `TicketStorage` and `DistanceManager` in private,
short-lived `RefCell`s. Callers return owned work and end every backend guard
before callbacks, publication, reentry or an await. Installation is one-time;
there is no take/restore or mirrored tracker. The server-private `LiveLevel`
retains only Level, game time and dimension metadata after destructuring the
bootstrap ServerLevel.

One typed `PhaseOwner::{Task(tokio::task::Id),Thread(ThreadId)}` token binds at
outer phase entry and has one owner comparison path. The canonical server Tokio
task remains the owner across executor-thread migration. Non-Tokio synchronous
fixtures use the thread variant; other callers forward owned requests through
the mailbox.

`Level::run_distance_manager_updates` runs only the pure graph slots against
the sole Region simulation tracker and returns owned holder changes. The driver
publishes requested status/readiness and entity status after all backend/graph
guards end, invalidating the source cache before nested work. Shared light
imports use `publish_light_snapshot`; exclusive imports and the end-of-chunk-source
boundary reclaim superseded immutable versions.

Single-region: a safe insert-only store gives stable chunk references and
publication through `&self`; reads stay `&Level`, mutation requires `&mut Level`,
removal/unload only at phase boundaries. Block/chunk lookups have no `RefCell`;
game logic has no locks or unsafe casts. Requested full status and ticking-ready
use separate typed `Cell` fields. Simulation eligibility and entity section
status use canonical trackers behind module-private `RefCell`s with short guards;
queries read those trackers directly, with no mirrored eligibility sets. Publish
distance/status transitions in native order, including entity lifecycle and
storage requests, and release every guard before a callback. Publication must
register new canonical records through disjoint stores; it cannot turn the
parked `&Level` into `&mut Level`. Refactoring whole-`ServerLevel` integration is
prerequisite; shared status/registration cannot be deferred with mutating ready
tasks.

Parallel: end state borrows, park at the call site, yield ownership and enter the
exclusive lane in phase/region/operation order. Acquire/merge the dependency
closure, then resume. References pin ownership against removal/split/transfer.
Require game-core's continuations before enabling parallel mode; no next-tick
message, stale halo, air or timeout fallback.
