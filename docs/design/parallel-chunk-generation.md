# Parallel chunk generation

Proposal for the workstream after the proto-relight bridge lands. No runtime
changes are included. This follows [game-core](game-core.md) and preserves the
[synchronous acquisition contract](synchronous-chunk-acquisition.md).

The recommendation is one deterministic scheduling lane per level, concurrent
owned chunk computations on the worldgen pool, and ordered result integration.
Start with asynchronous IO, BIOMES and noise TERRAIN. Keep FEATURES serialized
per level, as vanilla 26.3 does. A whole FULL request must no longer occupy the
only generation work slot or wait synchronously for each dependency and light
job.

## What vanilla actually schedules

Source references below are to the shared decompiled 26.3 tree,
`/home/doc/code/Ferraloom/.cache/decompiled/26.3/net/minecraft/`. Line numbers
refer to that tree; no decompiled source is copied here.

`ChunkGenerationTask` is an orchestration task targeting one center/status,
not an indivisible generation job. `create` claims a `StaticCache2D` of holders
at the worst-case generation radius. `runUntilWait` schedules a complete layer
in X-major/Z-minor order and waits for its incomplete futures before advancing.
It initially tries the loading footprint, repeats EMPTY with the generation
footprint if necessary, and selects generation versus loading steps from each
chunk's persisted status. Its pending-future list is inspected from the end;
that observation order does not make worker completion a spatial write order.
Cancellation stops further scheduling and releases the cache claims after
already-scheduled layer futures settle.

Sources: `server/level/ChunkGenerationTask.java:35-173`;
`util/StaticCache2D.java:14-31,45-67`.

`GenerationChunkHolder#applyStep` acquires a status bump once per holder. Its
futures array is shared by overlapping orchestration tasks, so two FULL requests
do not regenerate the same `(holder,status)`. The started-work CAS permits the
next parent-to-child transition, not a neighborhood lock. Target escalation
replaces/cancels the orchestration task while keeping its status futures;
ticket downgrades fail and clear only pending disallowed futures. Generation
reference counts pin lifetime and the save dependency. They do not grant
exclusive access to every claimed chunk.

Sources: `server/level/GenerationChunkHolder.java:43-89,114-168,217-251`.

`ChunkMap#runGenerationTask` submits a short `runUntilWait` visit, then resubmits
when its awaited future settles. The dispatcher does not wait for the entire
requested FULL future before considering another center. Its executor is a
per-level **ConsecutiveExecutor named worldgen**. That executor runs one visit
at a time. The actual parallel computation comes from asynchronous escapes
inside particular status tasks, not from concurrently executing all visits.

Sources: `server/level/ChunkMap.java:191-209,658-677`;
`util/thread/AbstractConsecutiveExecutor.java:39-59,74-89,116-125`.

| Native step | Execution and completion | Mutation relevant to ownership |
|---|---|---|
| EMPTY | Region-file read, background `parseChunk`, then main-executor read/create | One loaded proto/full payload; POI prefetch joins the load. |
| STRUCTURE_STARTS, STRUCTURE_REFERENCES | Synchronous work during the worldgen visit | Center starts/references; structure-check notifications must retain order. |
| BIOMES | `ChunkGenerator#createBiomes` submits to the background pool | Center biome containers; local climate sampler and density-buffer lifetime. |
| TERRAIN, noise generator | `NoiseBasedChunkGenerator#buildTerrain` submits one background computation for fill, surface and carvers | Center sections/heightmaps, with biome, blending and structure inputs. |
| TERRAIN, flat/debug | Completes synchronously during the worldgen visit | Flat writes center layers; debug terrain is a completed task. |
| FEATURES | Synchronous `applyBiomeDecoration` during the worldgen visit | Bounded block writes plus structure-piece state, NBT, ticks and other region effects. |
| INITIALIZE_LIGHT | Synchronous canonical source-buffer initialization; light-executor future | Source buffer, section status and saved layers. |
| LIGHT | Light-executor future | Propagated layers can affect several chunks; light-correct is set at completion. |
| SPAWN | Synchronous worldgen call | Initial entity generation; Ferraloom still has the named NaturalSpawner hook pending. |
| FULL | Main-executor future | Center conversion, proto postLoad, loaded flag, block entities and tick registration. |

Sources: `world/level/chunk/status/ChunkStatusTasks.java:38-208`;
`world/level/chunk/ChunkGenerator.java:126-154`;
`world/level/levelgen/NoiseBasedChunkGenerator.java:353-398`;
`world/level/levelgen/FlatLevelSource.java:67-96`;
`world/level/levelgen/DebugLevelSource.java:77-86`;
`server/level/ChunkMap.java:557-605`.

The background executor is a shared ForkJoinPool, normally sized from available
processors minus one, capped by `max.bg.threads`. The passed ChunkMap executor
comes from the server's background executor. Ferraloom's dedicated off-tick
worldgen pool remains our architecture; matching Java's executor object graph
is not required to reproduce its results.

Sources: `util/Util.java:199-259`; `server/MinecraftServer.java:347`.

## Dependencies, priority and throttling

Direct dependencies describe which status each neighbor must have before a
step. Accumulated dependencies compose those requirements through parent
steps; they size the orchestration layers and lifetime claims. Neither is an
instruction to give every dependency exclusive mutable ownership to a worker.
`blockStateWriteRadius` separately bounds ordinary block writes: TERRAIN is 0,
FEATURES is 1, and other steps default to -1.

For one isolated, fresh FULL request, the generation layers have these radii:

| Layer | Radius | Positions |
|---|---:|---:|
| EMPTY; STRUCTURE_STARTS | 11 each | 529 each |
| STRUCTURE_REFERENCES; BIOMES | 3 each | 49 each |
| TERRAIN | 2 | 25 |
| FEATURES; INITIALIZE_LIGHT | 1 each | 9 each |
| LIGHT; SPAWN; FULL | 0 each | 1 each |

The saved-FULL loading footprint is radius 1. A failed loading attempt expands
to generation; existing EMPTY loads are reused. These are bounds for one task,
not the total work of a force-loaded area. Accessible, block-ticking and
entity-ticking range futures legitimately schedule additional statuses/FULL
neighbors. In particular, the forced ticket is entity-ticking level 31; its
range futures must not be deleted to improve a benchmark.

Sources: `world/level/chunk/status/ChunkPyramid.java:11-45`;
`ChunkStep.java:75-144`; `ChunkDependencies.java:11-51`;
`server/level/ChunkMap.java:128,296-318,362-364,680-715`;
`server/level/ChunkHolder.java:273-325`. Ferraloom's committed
`crates/ferraloom-game/src/level/generation/pyramids.tsv` captures these arrays.

Preserve the native queue behavior: lower queue level first, insertion order
between chunk batches at the same priority, and insertion order inside each
batch. Level-change messages precede release messages, submissions and polling
at dispatcher priorities 0, 1, 2 and 3. A task's queue level can change without
changing the identities of its status futures.

The four-slot `ThrottlingChunkTaskDispatcher` belongs to player ticket admission.
It is not a four-worker worldgen limit. Its slot is released after the
entity-ticking range future produces its result, including an unsuccessful
result. Keep that contract while allowing many dependency steps for admitted
requests to run on the pool.

Sources: `server/level/ChunkTaskPriorityQueue.java:22-77`;
`ChunkTaskDispatcher.java:36-99`; `ThrottlingChunkTaskDispatcher.java:23-36`;
`DistanceManager.java:50-55,75-103`.

## Conflicts and vanilla timing dependence

Two adjacent FEATURES calls do **not** execute their placement bodies
concurrently on the same vanilla level. `generateFeatures` calls the generator
inline and returns an already-completed future; the enclosing worldgen visits
are consecutive. A synchronous FEATURES layer also does not yield between its
X-major placements just because another request has higher priority. The
proposal must preserve those logical visit/yield boundaries.

Different terrain centers can compute concurrently. A FEATURES call that could
write a terrain center within radius 1 first requires that center's TERRAIN
future, preventing that ordinary block-write conflict. A holder's own step is
started once. `StaticCache2D` keeps references to the same chunks; it does not
make independent writable copies. `WorldGenRegion` checks direct-status access
and write bounds, rather than acquiring a multi-chunk lock. Paletted containers
also have a threading detector that reports concurrent access; this is not a
transactional feature merge mechanism.

Sources: `server/level/WorldGenRegion.java:89-108,126-158,292-330`;
`world/level/chunk/status/ChunkStatusTasks.java:141-151`;
`world/level/chunk/PalettedContainer.java:29-36,103-129`;
`util/ThreadingDetector.java:29-63`.

There can still be thread-timing dependence in **which serialized visit runs
next**. Independent background biome/terrain futures complete at different
times, enqueueing task resumptions through `ChunkMap#runGenerationTask`.
Overlapping decoration from different visits can therefore observe different
earlier writes if their legal visit order changes. Once a FEATURES body starts,
its neighbor writes and RNG-dependent decisions run in program order; merging
independently generated block deltas afterward would not reproduce that.
This is a source-derived possibility, not a claim that a particular seed has
already been captured producing two different block arrays.

Structure identity also matters beyond block footprints. A shared start may
carry piece flags or an adjusted position across several FEATURES calls.
`StructureStart#placeInChunk` calls each intersecting piece and `afterPlace` on
the same piece container. For example, shipwreck pieces retain `heightAdjusted`,
and scattered pieces retain the first ground-height result. Neighbor jobs must
not restore an older copy of such state. The existing typed structure carrier
and generation revisions remain the publication path.

Sources: `world/level/levelgen/structure/StructureStart.java:79-111`;
`structures/ShipwreckPieces.java:140-187`;
`ScatteredFeaturePiece.java:50-75`.

Do not classify every structure field as immutable just because its start
future completed. Noise terrain constructs a Beardifier from referenced pieces
inside its asynchronous computation. The owned terrain input must capture the
needed typed geometry at an explicitly ordered preparation point, before any
conflicting piece mutation; it must not retain pointers to mutable piece state.
Any geometry case whose native observation point cannot be reproduced this way
stays serialized until audited. JVM collection/identity traversal differences
already covered by the structure oracles are a separate source of variation,
not evidence for concurrent FEATURES writes.

For a geometry input that FEATURES can still mutate, retain a scheduler read
lease until its terrain job returns, so the conflicting placement waits. This
is an ownership/scheduling record, not a lock held by a callback. It selects
the legal native execution in which the terrain observation finishes first;
it does not invent a merge of two versions of a mutable bounding box.

Sources: `world/level/levelgen/NoiseBasedChunkGenerator.java:99-103,363-366`;
`world/level/levelgen/Beardifier.java:46-97`.

The determinism target is identical results for identical inputs and logical
request/ticket order regardless of Rust worker count or completion order. When
vanilla has several legal outcomes, select a reproducible legal schedule:
retain its dependencies, priority/tie rules, complete-layer ordering and
consecutive placement bodies, while observing asynchronous completions in our
stable integration order. This does not promise to match an arbitrary vanilla
wall-clock interleaving. The legality argument must be checked with controlled
native completion/order vectors before expanding parallel placement.

## Proposed owned work and continuation model

`GenerationRuntime` remains the canonical owner of the scheduler and reachable
continuations. Stop moving the entire `GenerationState` into one worker. Its
holder payloads have explicit resident/in-flight ownership; each dispatched
step moves only its writable payload and owned immutable inputs. The current
holder's `HolderGeneration` remains the sole futures/task/allowed-status state.
Do not introduce another holder map or simulation graph.

Keep three identities distinct: holder incarnation and status-future identity;
replaceable orchestration task identity; and a monotonic ordinal assigned when
a dependency-ready work item is submitted. Deduplicate by holder/status, not
FULL request. Several requests can wait on the same step. Workers receive the
identity, context, owned input and cancellation token and return owned results,
partial effects and failures. They never wait for the suspended tick to return.

| Work item | Owned mutable data | Other inputs and integration |
|---|---|---|
| Load/parse | Owned disk input and resulting chunk | Issue IO without immediately blocking on each read; decode on the pool, publish in submission order. |
| BIOMES | One center proto | Immutable generator/configuration and job-local sampler/RNG; return the center and its biome result. |
| Noise TERRAIN | One center proto | Owned prepared Beardifier and the exact biome/environment/blending inputs; return sections, heightmaps and all center side effects. |
| Starts/references | Initially preserve the consecutive native visit | Parallelize only after their exact read/mutation sets and notifications are separated; use the existing typed structure payload. |
| FEATURES | One exclusive owned direct neighborhood and its mutable referenced starts | Initially one FEATURES body per level; restore all region payloads, publish newer structure revisions, and drain partial-write light before settling it. |
| INITIALIZE_LIGHT/LIGHT | The existing proto engine and owned canonical section/source snapshots | Future-like light continuation; no worldgen worker waits synchronously for the pool. |
| FULL | Owned prepared center and promotion continuation | Existing center-light/staging/entity/finalization checkpoints, on the owner path. |

Neighbor input preparation needs a narrower contract than today's
`&mut [RegionChunk]` for every step. Two TERRAIN centers in a single FULL task
have overlapping radius-8 metadata neighborhoods, so moving both whole
neighborhoods exclusively would retain the bottleneck. Prepare immutable,
typed biome/structure inputs, with required status and revision, while owning
each writable center exclusively. Copies are frozen computation inputs, never
independent writable protos later merged over canonical state. Prepare from a
valid logical scheduling point; no arbitrary stale snapshot or live-level halo
is permitted. Do not rebuild starts from NBT except at disk load.

FEATURES is deliberately more conservative. A radius-1 block footprint alone
does not bound all mutations: block-entity lookup can materialize NBT, entity
and tick APIs address chunks, and referenced starts can live outside the block
write area. Initially transfer its entire direct region plus the referenced
mutable start payloads, waiting for any already-leased center to return. This
may delay a visit but adds no invented status requirement or generation radius.
Deduplicate these payloads by their owning chunk/start identity: an origin
already in the direct region must not also travel as a second mutable copy.
No callback holds a scheduler guard. Parallel disjoint FEATURES calls are a
later optimization requiring complete read/write/structure/side-effect sets,
not just nonoverlapping 3x3 squares.

Sources for the additional effects: `server/level/WorldGenRegion.java:215-237,
324-373`. The new worker-input contract requires coordination with the
worldgen region/context owner before implementation.

The scheduler records dependency waiters and native layer continuations rather
than rescanning and re-executing whole FULL pyramids. A logical worldgen visit
continues through synchronous steps until vanilla would wait. If a synchronous
step executes on our off-tick pool, reserve that logical visit until its owned
result returns; do not interleave a different FEATURES visit inside it. Native
asynchronous BIOMES/TERRAIN/IO/light waits release the control lane, allowing
other ready visits and many center computations to proceed.

Submission ordinals are topological: dispatch only after required inputs are
committed. Buffer physical completions and integrate submitted work in ordinal
order, including errors and cancelled results. Thus no lower ordinal waits for
a dependency assigned a higher one. Repricing affects queued work using the
native priority rules, not already-issued ordinals. Result buffering introduces
some head-of-line delay; measure it rather than allowing physical arrival to
choose a feature order. Bound outstanding inputs and completions by worker and
IO windows so parallelism cannot clone hundreds of full neighborhoods.

The existing main-thread completion queue still starts FULL integrations in
its canonical request order. An active promotion keeps later completions
reachable for a nested acquisition; request order must not be strengthened
into waiting for an outer callback to finish before its nested dependency can
progress. Entity registration and successful finalization retain their
permanent checkpoints. Cancellation stops new steps, drains issued work and
retains partial effects before save/release; holder epochs reject publication
into a removed/recreated holder. Required failures never become empty chunks.

## Light and managed acquisition

The relight follow-up's engine and canonical `ChunkSkyLightSources` buffers are
prerequisites. One serialized proto light-engine owner serves concurrent
generation jobs. BIOMES and pre-light TERRAIN jobs do not need it. A lit-proto
FEATURES body borrows that same engine for section-status/checkBlock calls and
updates its owned source buffer synchronously. If the engine is already on
the light pool, retain a continuation and keep unrelated pure jobs running;
do not manufacture another engine or block a worldgen worker waiting for it.

After restoring the neighborhood, submit an owned flush and retain its
continuation. Integrate every affected layer, including propagation-only
neighbors, before success, the original partial-write error, cancellation save,
proto save or FULL/light transport. INITIALIZE_LIGHT fills sources explicitly,
including loaded chunks; saved status alone does not fill them. Raw bulk writes
must not derive new source buffers. FULL snapshot refresh preserves the one
canonical buffer. Advance generation revisions in logical integration order.

Native lighting itself has a consecutive executor and PRE_UPDATE/propagation/
POST_UPDATE ordering; its future does not occupy the worldgen lane until light
finishes. Preserve that separation. Ferraloom currently has separate proto and
live engines connected by guarded imports. This proposal adds no engine and
does not assert that split is observationally identical; the separately
requested canonical-engine review remains necessary.

Sources: `server/level/ThreadedLevelLightEngine.java:59-66,124-134,144-213`.

The parked synchronous acquisition pump drains distance updates, dispatch and
ordered IO/generation/light/publication continuations, waking on completions.
End all short backend guards before callbacks, reentry or await. No server tick,
expiry, scheduled/random phase or packet simulation advances while parked.
Keep the approved single-region deviation: mutating postProcess/startTicking
and send-ready continuations wait for the next exclusive point. FULL33 does
not wait for them. The phase-5 continuation model, not worldgen parallelism,
removes that deviation.

Sources: `server/level/ServerChunkCache.java:143-173,623-630`;
`server/level/ChunkMap.java:680-715`; synchronous acquisition design above.

## Delivery slices and measurements

1. Measure the serial relight baseline before changing scheduling. Add optional
   cold-path spans/counters around existing boundaries, not permanent per-cell
   logs or a new gameplay policy. Capture the same far `[64,64]` request from
   `/forceload add 1024 1024` on vanilla and Ferraloom, with identical seed,
   dimension, generator, tickets and a fresh target region. Run the existing
   Strict bedrock-flat case and a separate Strict noise case: noise terrain
   parallelism alone cannot explain or guarantee a fix for flat-world latency.
2. Split dispatch from execution with per-holder/status futures and retained
   continuations, initially at one worker. Issue bounded asynchronous IO and
   remove the whole-request `generation_work` slot. Preserve existing output
   order, cancellation, save and reentrant promotion behavior before adding
   concurrent mutation.
3. Dispatch owned BIOMES and noise TERRAIN centers concurrently. Keep metadata
   preparation and FEATURES in their native logical lane. Test two real terrain
   jobs overlapping, not only two queued jobs. Compare one worker with two and
   the configured pool width while deliberately reversing completion order.
4. Replace generation-worker light blocking with the owned light continuation;
   overlap it with independent center computations. Exercise cross-chunk lit
   writes, cancellation/error flush and center-light publication.
5. Broaden only from measured bottlenecks: empty-chunk allocation/parse,
   starts/references and redundant snapshot/capture work. Parallel FEATURES is
   not required for the initial multicore gain and needs a separate proof.

Record durations as intervals with a request/step identity and status. Report
both aggregate work and the actual request critical path; parallel spans must
not be summed into an apparent wall-clock latency.

Use the same builder allocation sequentially for the two servers. Report Rust
profile, JVM warmup, worker limits, cold/warm cache state and the actual chunk
counts. Separate first-request startup from repeated fresh-region requests,
report median and tail latency, and require the force-load feedback to fit the
existing connection deadline. A noisy single run or more queued jobs is not
proof of multicore speedup.

| Measurement | Boundary / comparison |
|---|---|
| End-to-end latency | Server receives the command to feedback flush; also bot send-to-feedback on both servers, to separate networking from generation. |
| Queue wait | Submission to worker start, plus dependency wait, priority wait, owned-payload wait and ready-result wait for ordered integration. |
| IO | Read queue/service, decompression/parse, absent-chunk construction, capture/serialization and writes; count requests, hits and misses. |
| Per status | Submitted/executed/deduplicated count and CPU/wall duration for each real 26.3 status; split TERRAIN fill/surface/carvers only when profiling needs it. |
| Pool utilization | Active worldgen jobs over time, maximum concurrency, busy-worker time, and time stalled on light or ownership. |
| Light | Queue wait, snapshot/source-copy cost, propagation, returned layer count and all affected chunks. |
| Integration | Ordered-result buffering, proto capture/save, center import, stage/entity/finalize and deferred ready tasks separately. |
| Memory/copy cost | Peak owned chunks/snapshots, copied section bytes and buffered completions. |

For native per-stage data, use its `JvmProfiler#onChunkGenerate` JFR events
(`ChunkStep#apply`) plus a narrowly scoped scheduling trace for submission,
resumption, async start/end and FEATURES entry/exit. JFR generation events skip
loading-only steps, so label those separately. Any native instrumentation must
preserve the executor wrappers and must not serialize the pool accidentally.
Keep timings out of parity observations; do not extend connection deadlines to
turn a slow request into a pass.

Evidence already available is limited to the earlier scenario totals:
Ferraloom reported approximately 48.206 s before relocation and 30.058 s after;
vanilla completed its 19 observations, but its matching request latency and
the per-stage split have not been measured. No speedup is attributed to a
particular stage yet. The native schedule trace should also confirm that
same-level FEATURES intervals never overlap and BIOMES/noise TERRAIN do.

Correctness gates retain the 6,534 start-chunk oracles, active saved-status and
structure-carrier tests, flat join and the same force-load scenario. Add
controlled native order vectors for two overlapping decoration visits and
shared piece state, then verify Rust worker-count/completion-order invariance
against each chosen legal order. Include cancellation after partial writes,
shared-dependency deduplication, repricing, holder recreation, and a nested FULL
promotion dependency. The 20 full-FEATURES goldens and two-sided noise
saved-region parity remain pending with their existing named behavior reasons;
parallelism does not make those cases pass by declaration.
