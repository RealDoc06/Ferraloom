# Vanilla reference investigation

Ferraloom targets the official Java **26.3** release. This investigation ran on
`devbox` (Linux x64) on 2026-10-15, entirely in local `.cache/` directories.
No remote machine, production server, or global REA configuration was changed.

## Reproduce

Prerequisites: Python 3.11+, npm, and a REA-supported Node.js release. The
investigated installation used Node 24.18.0 and `rea-agents` **6.5.0**.

```sh
python3 tools/rea/inspect.py
```

The script downloads the pinned official server if absent, verifies Mojang's
SHA-1, runs actual REA artifact inspections on both the bundler and its inner
server, verifies the inner JAR against `META-INF/versions.list`, and inventories
vanilla data resources. All vendor artifacts and raw evidence stay under
`.cache/`; nothing is uploaded by this workflow. It does not start gameplay or
accept an EULA. The resulting `.cache/rea/report.json` contains resource counts,
version metadata, hashes, and evidence IDs.

To capture the executable's help through REA and optionally inspect focused JVM
bytecode with an existing JDK:

```sh
python3 tools/rea/inspect.py --java /absolute/path/to/java25/bin/java --javap /absolute/path/to/javap
```

The help capture unpacks bundled libraries inside `.cache/rea/help-run/` and
runs `--help`. The `javap` output is separate, explicitly identified bytecode
evidence, rather than a claim that REA decompiled JVM methods.

## Artifact identity

The [official version metadata](https://piston-meta.mojang.com/v1/packages/11f1aefe473eefba0af62e3286c33b692807b2eb/26.3.json)
identifies the
[server download](https://piston-data.mojang.com/v1/objects/33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c/server.jar).
Its downloaded bytes were verified, then its embedded `version.json` was read.

- Outer server SHA-1: `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`.
- Outer server SHA-256: `d052f14d7a173734fba553711e5b570162e2f2a313267ee31a21b975a679be64`.
- Inner server SHA-256: `a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6`.
- Protocol: **777**; world data version: **5023**; Java: **25**.
- Resource pack version: **97.1**; data pack version: **121.0**.
- Outer entry point: `net.minecraft.bundler.Main`; inner entry point:
  `net.minecraft.server.Main`.

The outer JAR contains the real server at
`META-INF/versions/26.3/server-26.3.jar`. Treating only the bundler classes as the
Minecraft implementation would miss almost the entire server.

## Actual REA results

The [REA investigation skill](https://github.com/morluto/rea/blob/fe7bcc3fbeafe73fed97f1d6d75f6ee3a0821d4a/.agents/skills/reverse-engineer-anything/SKILL.md)
was read and applied. CLI calls used pinned npm package 6.5.0, with no `setup`
or global installation. The inspected upstream revision was
`fe7bcc3fbeafe73fed97f1d6d75f6ee3a0821d4a`.

- Outer `inspect-artifact` evidence:
  `ev_a6561ae3f6427ab0cc92f0c8af962abe6f5a846748ed32d14bb5a8dd483c45a4`.
- Inner `inspect-artifact` evidence:
  `ev_f3b5691b97f3a992098c925f7108a08fd1299200ee7263286a6d35f5c5fd1f46`.
  Its inventory completed with 18,558 occurrences, including the archive root.
- Java 25 `capture-process` evidence for `server.jar --help`:
  `ev_e9d44a8eb849df6e7ee25d80dbd449c3a96c9b26e3fb83a4ef402fce8b715159`.
  The process exited with code 0 and verified process-group cleanup. All 11,413
  raw terminal bytes were retained. Help documented settings initialization,
  world/universe selection, port, demo, bonus chest, safe mode, upgrade/cache
  controls, region recreation, profiling, and GUI selection.

Evidence IDs bind the original run and its local paths. Runtime evidence IDs
will differ on subsequent runs.

The inner archive contains **7,762 class files**. Its vanilla resources include
2,042 recipe files, 1,866 advancements, 1,447 loot tables, 1,511 structures, 883
tags, and 1,144 world generation files. Those counts exclude directory entries;
they are an artifact inventory, not a count of independently tested behaviors.
Newer systems visible in the archive include timelines, world clocks, villager
trade definitions, and sulfur cube archetypes. `report.json` preserves every
observed top-level data resource category so later parity work can expand the
feature inventory from actual shipped content.

## Focused protocol conclusions

Reading the exact inner JAR with `javap -p -c` established these static behaviors:

- `ClientIntentionPacket` reads the protocol as a VarInt, hostname with a
  255-character limit, port as an unsigned short, and intent as a VarInt.
- `ServerStatusPacketListenerImpl` answers the first status request, records
  that it was handled, and disconnects on a repeated status request.
- A ping request returns the supplied signed 64-bit value and then disconnects.
  The handler does not require a preceding status request.
- `ServerStatus` includes description, optional players/version/favicon, and
  `enforcesSecureChat`.

These observations suggest concrete differential cases: repeated status,
ping before status, negative and boundary ping values, invalid host lengths,
and status-field comparison. Static control flow does not establish observed
network scheduling; the differential harness must check the executable.

## Gameplay and persistence investigations

Additional Java 25 `javap -p -c` inspections use the same verified inner JAR.
They guide implementation; the paired executable tests remain the acceptance
evidence.

- `ScoreboardSaveData.Packed` and its objective, score and team codecs use a
  `data` envelope with root `DataVersion: 5023`. Missing objective criteria and
  render types mean `dummy` and `integer`; missing score `Locked` means false.
  A real reference save confirms signed scores, team membership and settings,
  the sidebar objective, and an unlocked trigger score. The current below-name
  slot identifier is `below_name`.
- `ForceLoadCommand.changeForceLoad` visits X in ascending order, then Z.
  `ServerLevel.setChunkForced` waits for each newly forced chunk before the
  command admits the next. Add/remove results count actual changes; repeating
  an unchanged range fails. Removing all forced chunks returns zero.
- `ChunkGenerationTask` schedules status layers and waits for a layer before
  advancing. Java's world-generation dispatcher uses a consecutive executor;
  neighboring decoration work can observe earlier decoration writes. Rust's
  independently ready generation tasks therefore need ordering analysis as
  well as cache exclusion. Sorting whichever tasks happen to be ready does
  not establish equivalent scheduling.
- Generation postprocessing performs the captured fluid tick first, then the
  liquid-block tick or neighbor-shape fold in west, east, north, south, down,
  up order. Pending positions belong to dimension-relative sections and must
  survive saving until processed.
- `execute if loaded` requires entity-ticking accessibility and loaded
  entities, independently of the queried Y coordinate. Scheduled block/fluid
  ticking extends one chunk beyond that entity-ticking range. A generated
  neighbor can consequently tick blocks while failing `if loaded`.
- Vanilla's autosave countdown continues while simulation is frozen. Global
  player and saved data are still saved with `save-off`; per-world saving and
  ordinary terrain chunk-unload processing honor that setting. Entity storage
  independently processes hidden-section unloads even with `noSave` enabled.
  Ferraloom now schedules server-wide autosaves outside the frozen simulation
  branch; its ordinary terrain unload path still needs the save-off gate.
- The foundation's `Nbt::write_to_writer` drops serialization failures before
  writing its byte buffer. Durable writers now use a shared fallible Java NBT
  serializer and synchronized atomic replacement. Oversized modified-UTF keys
  or values return errors before replacing existing saves. Separate tests
  cover player files, chunk/entity caches, metadata, POI and PUMP containers.
- `BaseCommandBlock.performCommand` resets `successCount` before executing a
  command, including attempts that fail. Ferraloom now resets that count too.
  Last-execution tracking and the complete command-block lifecycle remain
  separate compatibility work; the scheduled-callback test advances game time
  between executions.

The original underground sample differed in three unsupported glow-lichen
states and four gold-ore cells. Deferred postprocessing removed the lichen
differences. The clean `571b6ca` executable's 67-scenario paired run now matches
all 16,384 sampled block states, including the gold cells. That build includes
sequential forced-chunk admission and chunk lifecycle changes. This observation
does not isolate the earlier gold mismatch's cause or establish general
generation ordering. A diagnostic build of an earlier Rust revision had also
matched vanilla, so its trace failed the required original-output baseline and
could not explain the uninstrumented failure conclusively.

A separate official-server fixture captures a furnace, blast furnace and smoker
after exactly 237 stepped ticks, with coal consumed and cooking in progress.
Modern saves use signed integer cooking and fuel timers and a floating-point
`speed_multiplier`; contextual fuel resolution supplies cooking speed as well
as burn duration. Copies of that Java save retain their timers and continue
cooking correctly in Java. The `571b6ca` Rust executable loses those timers on
load. Independent lava-bucket copies also expose the missing bucket crafting
remainder and wet-sponge conversion. These are diagnostic failures pending a
repair and a fresh comparison, not passing feature evidence. The source fixture
is kept immutable under the ignored reference cache; its canonical file
manifest SHA-256 is
`de91961415ddb941e59f4741b005b1d768a5beabb1461e2b8ccbf3952ecf3603`.

## Boundaries and remaining work

REA 6.5.0 does **not** expose a JVM JAR decompilation workflow. Its JADX provider
supports standalone Android APKs. A real `inspect-android-class` probe against
the verified inner JAR returned `capability_unavailable`, with the exact reason
`is zip; provide one standalone APK`. The prerequisite JADX 0.7.1 download was
verified against REA's pinned SHA-256
`6e5eacf500b64292bfb73c49797c1958f6ee44646e43e868039ae7feb573ff75`;
its availability does not change that target-format boundary. Renaming a JAR
or constructing a fake APK would not establish supported analysis.

Archive inspection covers ZIP central-directory entries. It does not establish
execution, complete semantics, signature validity, or parity. The help capture
uses before/after filesystem snapshots and sampled process trees. Its raw
terminal is complete, but rendered-terminal frames and some extracted-library
digests exceeded explicit budgets. The Java executable digest is a prelaunch
sample; inherited environment variables are unrecorded. None of these results
assert full runtime equivalence, world-generation correctness, multiplayer
correctness, or that all default-server features are implemented in Rust.

Continue with REA process captures and the vanilla differential harness for
each feature, using JVM bytecode tools where REA lacks format support. Keep
Mojang JARs, extracted resources, and disassembled/decompiled code out of Git.
