# Synchronous live chunk acquisition

Proposal only; follows [game-core](game-core.md). Source: vanilla 26.3 `Level`,
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
unpack only in `startTickingChunk` at its game time; today's `add_chunk` unpacks
too early for a prolonged FULL-only acquire. Preserve entity input on failure.
FULL does not imply pending
activation, entity IO/visibility or ticking post-processing completed.

Install canonical **center light** before returning FULL: generation finished
`LIGHT`, but `World::integrate` queues another actor import. Await that import
or carry its snapshot; no 3x3 ticking requirement. Preserve revision checks,
attached-FULL protection and initialization versus delta-light semantics.

## Blocking without starving the pipeline

Extract a concrete, reentrant driver separate from gameplay borrows. Proposed
chunk-pipeline-owner seam:
`request_status(pos,status,load) -> AcquireHandle` and
`poll_acquisition_step() -> Progress/Idle/Error`, plus a wakeable completion
test. Handles retain request identity/ordinal. Apply tickets before dispatch;
`ChunkMap::request` returning `None` means reuse. Reuse
`World::request`/`poll_terrain` and
`generation_completion::{integrate_terrain,integrate_light}` behind that seam;
extend their FULL-only runtime boundary to requested statuses.
`await_generation_chunk`'s whole-server borrow/`world.ready` wait cannot serve it.

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

## Ownership in each mode

Single-region: a safe insert-only store gives stable chunk references and
publication through `&self`; reads stay `&Level`, mutation requires `&mut Level`,
removal/unload only at phase boundaries. No hot-read `RefCell`, game locks or
unsafe casts. Publication must register new canonical records through disjoint
stores; it cannot turn the parked `&Level` into `&mut Level`. Refactoring whole-
`ServerLevel` integration is prerequisite; registration cannot be deferred.

Parallel: end state borrows, park at the call site, yield ownership and enter the
exclusive lane in phase/region/operation order. Acquire/merge the dependency
closure, then resume. References pin ownership against removal/split/transfer.
Require game-core's continuations before enabling parallel mode; no next-tick
message, stale halo, air or timeout fallback.
