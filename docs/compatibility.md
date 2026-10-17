# Vanilla compatibility

Ferraloom targets **Minecraft Java Edition 26.3**, the stable release returned by Mojang's [official version manifest](https://piston-meta.mojang.com/mc/game/version_manifest_v2.json) when checked on 2026-10-15. The pinned [version metadata](https://piston-meta.mojang.com/v1/packages/11f1aefe473eefba0af62e3286c33b692807b2eb/26.3.json) records release time 2026-09-15T11:23:02+00:00. Snapshots are excluded. Updating the target is an explicit change that requires new reference artifacts and evidence.

The machine-readable ledger is [compatibility/features.json](../compatibility/features.json). It contains 200 feature families: 191 vanilla server families, seven additional concurrency and performance requirements, and two deferred plugin families. **No family is currently verified.** Partial rows retain selected paired observations; the [checklist](features.md) shows each implementation's results. This inventory and its validator do not establish complete implementation of those families.

The latest [native-check record](../compatibility/results/native-checks.json) used clean engine source
`b8e01b4756b00fe89dcdb88ac76a604c5e4cb9eb` on the dedicated `ferraloom-builder` (`10.10.10.162`),
with executable SHA-256 `145f50241cf971dee01860281614e7f1ab1c8f10c75615bdb8154ae6275afe3f`.
All 16 checks passed: 1,647 Rust tests (96 data, 141 protocol, 92 command,
416 world, 24 inventory, 866 core and 12 wrapper), 151 Python tests, 74 Node
tests, strict TypeScript, both formatting checks, Clippy for the wrapper and all
seven vendored crates, and the final build. Four inherited filtered Core child
results are retained separately and are not added to the total. GameTest compiled
only and contributes zero standalone behavior tests. The
[original native receipt](../compatibility/results/evidence/0d2cb9a1bdd58986df1974805ef528b4775bb73b559ee4262ab2a9d1fe3c93ee.json),
[completed builder receipt](../compatibility/results/evidence/9ff973e5b879ad6f197ee0a10eb7887b5a0b3630eb2ec720ae2f5347e90dd0c8.json) and
[ROOT verification](../compatibility/results/evidence/fdc30e6ff1fd7d45c1bbfcf004681a1f09f08037747882f48ff7ab7b79f96c6b.json) bind actual source, host,
resource scope, final binary and original command logs by content hash.
This checkpoint applies to tested engine `b8e01b4`; this docs/evidence update was not native-checked.
Engine source `b8e01b4` was [published to GitHub](../compatibility/results/evidence/3594d64c7d828659f279497f14b3b7289b416f5565b045d6a2cfc14b3be8864f.json)
on `main` and `validation/r29-cc10c5b`. The receipt records those refs before any later evidence/docs commit.
Both recorded GitHub `Check` runs for the subsequent `0376775` docs/evidence checkpoint
[completed successfully](../compatibility/results/evidence/efd59b90b1a7bdcbdf223f565c1a33804b68a34c7de2cc4b754a48012993a40f.json).
Both `Check` runs for the later `97b584f` publication also
[completed successfully](../compatibility/results/evidence/37733874e3bc824027b7b570b52a463890f705460eb357cf8b3c355407bdb6df.json).
These two receipts establish CI only for the commits they name. Neither records
a manual vanilla-compatibility run or establishes further game behavior.
The integrated harness source `d82613a` separately
[passed all 162 Python tests](../compatibility/results/evidence/dce064e4df04b9fcb58ef69069706fb9829ea3e63478d3231e8ee05a236a58c0.json)
on the builder. Its engine/build inputs are identical to `b8e01b4`; the new
check is a Python unit result, not another full native or runtime campaign.

The [selected paired crafting campaign](../compatibility/results/crafting.json)
used that same engine, client source and executable on `ferraloom-builder`.
All 13 selected ordinary crafting composites passed: 13 Java and 13 Rust case
rows across two fresh boots and two clients, equal recorded outcomes, 114 saved
snapshots and 60 state/statistics checks. Both servers and both clients exited
with code zero without forced cleanup; runtime errors were empty. Six boot
checkpoints retained the source, binary and resource pins. The run used 6 GiB
memory, no swap, 200% CPU, two workers, a 180-second startup timeout and a
20-second interaction timeout. Only the Java reference used the configured
`sync-chunk-writes=false` profile; Rust configuration was unchanged.

The byte-identical [ROOT verification](../compatibility/results/evidence/b8d3948bbf73ba511d4df051edd699dc78610a183e105f216e2e353b7ce9cc56.json)
records verification on `devbox` of all 527 original files and archive members,
including 520 production-manifest files. The full report, archive and export
index are retained privately, not public raw goldens; the crafting index labels
their hashes accordingly. This evidence establishes only the selected campaign.
It does not promote a feature family, REA or the ledger, or establish full
gameplay, default Java write-profile parity or crash durability. Earlier failed
crafting evidence and historical results remain unchanged.

The [selected Dragon campaign](../compatibility/results/dragon-parts.json)
used engine `b8e01b4`, the same executable and client harness `b2be7cb` on
`ferraloom-builder`. All 25 bounded composites passed: 25 Java and 25 Rust rows
across four fresh boots and four clients, with 12 boot-audit checkpoints.
All servers and clients exited with code zero without forced cleanup;
runtime errors were empty. Frozen Normal-difficulty NoAI fixtures cover
phase/NBT reads and attacks against eight parts at the literal origin. The
comparison requires positive, unchanged or lethal health-loss classes, not
exact damage magnitudes. Separate fresh death fixtures observe the shared
clock and removal at tick 200. These are state reads, not persistence/restart
tests. Moving part geometry, full flight/fight/Scanning AI, natural Holding
initialization, Peaceful admission, XP totals, crystals, rendering and
advancements remain open. The byte-identical
[ROOT proof](../compatibility/results/evidence/2110215ba955be582bf432debd44a368319e15a55306bfdba148aaf9e9cacbd8.json)
was produced only after checking all 50 archived originals and 43 production-manifest
files, including typed command/wire lineage and the bounded comparison, and pins
their private index. Raw reports,
configurations, worlds and process data remain private.

The [genuine REA selected crafting comparison](../compatibility/results/rea-crafting.json)
used REA 6.5.0 with `rea-process` v3 and `node-pty`, engine/client `b8e01b4`
and bridge `ec8da88`. Two fresh captures matched the declared
`table_offset_right` gameplay file trace, with five retained checkpoints each,
requested result removal of two and whole physical output of three. Both
servers and clients exited cleanly. The reference used buffered Java chunk
writes; Rust configuration was unchanged. The model was defined after
discovery and is not a pristine oracle.

The complete generic REA comparison remains **changed**, with status, terminal,
exit and filesystem dimensions changed, interaction unchanged and process
unknown. Its 16 limitations and each capture's environment/network/process
unknowns are retained. The accepted result applies only to the declared trace;
it does not establish generic REA equivalence. No actual terminal-metadata
fallback occurred in this successful run; eight synthetic controls do not
prove the earlier failed PID's historical state. ROOT verified 118 selected
original files, while 129 world/JAR/binary payloads remain on the builder and
are checked here by retained metadata only. Earlier failed attempts remain
failed. The byte-identical
[ROOT proof](../compatibility/results/evidence/c97a9ab488346ccf7b8c363e166d4ee183de2a55c40862f3dc5352b8cde5d306.json)
retains the declared verdict and complete generic dimension statuses, with
content hashes for the private original comparison. Neither new campaign
promotes a family or establishes default Java
write-profile parity, crash durability or full vanilla compatibility.

The [current-engine player interchange](../compatibility/results/player-interchange.json)
used exact engine and client `b8e01b4` on the builder. All seven selected composites
passed in each direction, Java to Rust and Rust to Java: 14 rows, four clean
server boots, six clean clients and 12 lifecycle checkpoints. Two whole worlds
were copied unchanged after saving and stopping each source. Selected identity,
named inventory components, position, vitals, exact saved XP fields and
advancement continuation passed. Actual statistics requests observed
`open_barrel` 3 at source/import and 5 after continuation; independent stopped
JSON retained `open_barrel` 3 to 5 and `leave_game` 2 to 3.

The byte-identical [ROOT proof](../compatibility/results/evidence/2c7b1b33fb3d4574debe073d98bd0e826a61e3c70ba2f7bce879fa4a2776e309.json)
follows checks of 260 fixture originals, 12 metadata originals and the complete
archive, including independent decoding of four stopped player files and six
statistics payloads corroborated by their client logs. Current stopped world
files match the recorded manifests. Historical seed snapshots and empty
directory lists remain runtime witnesses rather than reconstructed past bytes.
Only the Java reference used buffered chunk writes. No third destination boot,
XP wire decoding, opaque-field gameplay, legacy/DFU, signed identity or full
player-data family claim follows. The older reverse-transfer failure remains
unchanged, and no feature family is promoted.

The [current-engine item interchange](../compatibility/results/item-interchange.json) and [container metadata](../compatibility/results/container-metadata.json) used exact engine/client `b8e01b4` on `ferraloom-builder`. Items passed five composites per direction with six clean boots, six clean clients and 18 lifecycle checkpoints; containers passed 26 cases per server with four clean boots, four clean clients and 12 checkpoints: 62 rows, ten boots, ten clients and 30 checkpoints combined. The byte-identical [ROOT proof](../compatibility/results/evidence/d9c8cb60979ce2d554c38ffb9876be3e8034328512416b3b914eca5dc03a9f5f.json) follows verification of 366 fixture originals and 17 metadata originals, including independent decoding of four final raw player saves, two container chunks and 176 menu payloads.

The eight selected item stacks cover water/swiftness, typed potion defaults, nested stacks, opaque custom data, fireworks and count mutation followed by reload. Two intermediate player saves and the two unchanged world-copy operations remain historical runtime witnesses; final source/reloaded player bytes and world manifests were independently checked. Container observations cover selected titles, lock controls, hand swaps, pending loot, deposits, double chests and clean restart; privacy observations have no raw chunk-packet check, and deposit prediction-hash tails were retained without decoding. Unsupported loaded predicates remain unverified, as do general item wire/gameplay and full-family compatibility; only Java used buffered chunk writes, Rust stayed unchanged, the existing native proof was reused without new test counts, and older failures and ledger statuses are unchanged.

The separate [selected Jukebox campaign](../compatibility/results/jukeboxes-B-J.json)
used engine `b8e01b4`, client `5bcd504` and the recorded B executable on
`ferraloom-builder`. All 12 composites passed across Java source, Rust import,
Rust reload and Java return: four clean boots, eight clean clients, 12 lifecycle
checkpoints, two unchanged world-copy witnesses and 36 `play_record` events.
All 22 song identities transfer; selected disc 11/Cat gameplay covers both hands
and survival/creative. Only disc 11 reaches actual ticks 1440/1441, on Java source
and Rust import.

ROOT independently checked all 1,373 exported originals. Earlier mutable
player/statistics references and whole-MCA hashes remain historical witnesses;
retained chunk/entity payload decoding does not verify earlier whole-MCA bytes.
Wire privacy checks decode block-entity projections; terrain/light contents are
length-skipped. Registry/item associations and numeric Java prediction hashes
retain the result index's limits. Java used buffered chunk writes, Rust stayed
unchanged, and the prior B native and focused J Python proofs were reused without
new test counts. The [failed combined B/B campaign](../compatibility/results/chat-horns-jukebox-B.json)
and older Jukebox/regression records remain unchanged. No family, default-sync,
crash-durability, plugin or REA claim follows.

Recent changes cover owned Java input continuations through final saves,
terrain-read refusal and item admission, POI persistence fences, builtin horns
and Jukeboxes, imported game time, scheduled tick delays, and current-format
equipment saves. The earlier
[Jukebox campaign](../compatibility/results/evidence/b0bde8d313a377be1616f7f1adf1f17cec84d784181076e1f2642e54c33e0ea0.json)
at `79a5baa` failed during Rust import while inspecting a legacy `Inventory`
entry with slot `-106`. The equipment writer was corrected in `acb8bcd`; its
native persistence tests pass in the record above. The subsequent
[paired Jukebox campaign](../compatibility/results/jukeboxes.json) on `97db9ed`
passed all 12 selected composites: Java authoring, Rust import, Rust restart and
Java return, with eight clients and two unchanged whole-world copies. All four
servers and eight clients exited cleanly without forced stops. The campaign
recorded 36 `play_record` events and completed in 457.961 seconds under supervision.

The selected campaign covers all 22 builtin song identities in transfer and
selected disc 11/Cat gameplay through both hands and survival/creative modes.
Only disc 11 reaches its natural duration. Custom song profiles, advancement
criteria, sculk/vibration, recipient boundaries, natural clean-chunk autosave,
and elapsed-only reentrant timing remain open. The exact gaps and raw artifact
hashes are retained in the result index.

The [historical five-suite regression attempt](../compatibility/results/regression-checks.json)
on `97db9ed` and its recorded binary failed in its first suite, player interchange.
All seven Java-to-Rust composites passed. In the fresh Rust source for reverse
transfer, `save-all flush` timed out and the console reported
`Failed to save portal POI: POI region write is already pending`.
The seven reverse-transfer cases share that failed save prerequisite; the Java
destination never booted. All three started servers and five clients exited with
code zero and no forced cleanup. The save failure remains an observed failure of that historical build. Fresh
paired acceptance for `b8e01b4` is not established by its native pass alone;
the selected campaigns above have their own separate evidence. Item interchange,
container metadata, chat throttling and goat horns did not start.

Earlier 24-boot/44-client regression passes at engine `6401924` apply to that
older engine. The new native checkpoint and historical Jukebox pass do not
replace the failed `97db9ed` player report; the ledger remains at 27 partial,
173 unverified and zero verified
families.

Earlier targeted runs used source and harness commit
`a868ebcf565e4566718c68e2b3fae845cd7f61b3`, with executable SHA-256
`68bc35698bea128fa88aae227aa7075c268f55d253ca23c8a144b6809db4de27`.

| Suite | Passing observations |
| --- | --- |
| [Player interchange](../compatibility/results/evidence/59715fad4953c845a79a087abfe18c1553eb86564a136073ef0a036a2b71a8b7.json) | Seven composites in each direction |
| [Item persistence](../compatibility/results/evidence/67718558b980d83535d81edd97b521aa520e2b259bb203c620d211b82c13f3c4.json) | Five composites in each direction, including destination reload |
| [Container metadata](../compatibility/results/evidence/95937a2cdb7a9ee5f294061b96cad355c3f323c5248ad7907558e97b5577938e.json) | 26 cases on each server |
| [Chat throttling](../compatibility/results/evidence/febd5269439e784cb43dcd43cff200cde5fec22f6640dd509a2d81113a31c0b5.json) | Five cases on each server |

All 20 server boots and 36 client sessions exited cleanly. The same source passed
1,360 Rust tests, 76 Python tests, both formatting checks, Clippy across all five
vendored crates and the executable wrapper, and the executable build. These are
targeted observations; no complete feature family is promoted.

Player interchange covers identity, selected inventory components, vitals and XP,
advancements, statistics, and continued saves. The item composites cover selected
potion, nested-container, custom-data and firework components through authoring,
transfer, mutation and reload. They do not establish general item use, rendering,
equipment synchronization or complete component support. Player and item cases
combine both runtimes in each direction and are archived separately from the
per-engine feature observations. The metadata and chat runs add 31 selected
observations to four existing partial families.

The [earlier player run](../compatibility/results/evidence/1f9b92e541587555c3f7af4cc21943a268d6193da3937a499f4a5b77774163f9.json)
retains Ferraloom's incompatible POI coordinate layout and an invalid zero-seed
test precondition. The latest run exercises the corrected POI codec and a real
source reconnect before transferring the observed nonzero enchantment seed.
Earlier failed reports remain unchanged.

The retained broad main-suite and block-data runs used clean source and harness commit
`105758aa42d2a365a9791857f45d826e56e80e9f`, with executable SHA-256
`5b606a5db4dfdac82865b0dd256d6d5ef6dc8e5ba7f87b995b19dc296bcfa13a`.
Both servers passed 96 of 96 selected main scenarios and a separate 34-case
live block-data check. The latter observes retained container menus, slot swaps,
pending loot, sign editors, literal submissions, selected structured sign commands,
permission gates and persistence across clean restarts. It does not establish
general inventory transactions or complete component and block-entity support.
World interchange passes in both directions, including scoreboard state and
command storage under `.` and a namespace resembling a legacy filename.
In that older run, the decoded command trees differ by two missing roots (`jfr`, `perf`) and 468
missing advertised literal paths with argument names normalized. This checks
advertised syntax, independently of command execution; some differences are
argument representation, while others are missing behavior. Subsequent command
aliases need a fresh command-tree comparison. That run's terrain
sample matched 16,380 of 16,384 states: Rust omitted four deepslate gold-ore blocks.
It reproduced the exact sample hash from an
[earlier failure](../compatibility/results/evidence/a96922864956b30c0a286e089affa721f3ec0b578cf72348de1fc9b35f52ecdf.json)
on the same generation code. Other retained runs matched the
sample; generation ordering remains unresolved.
The immutable
[main report](../compatibility/results/evidence/33a5aae5d5cb70916239d347b269fc0f9632d29b8c20aa69940744f9c33836cf.json)
retains both failures. The separate
[block-data report](../compatibility/results/evidence/054cbee99d48dacba56d705d99f9da6a0f3fc44109a9e3786c1c959ea0c752b9.json)
records all 34 paired passes and four clean server and four clean client exits. A
[prior fixture failure](../compatibility/results/evidence/fc13bba3a3595c4f37415dca8285932dd85eceb9b1054733f4856556c2dc5e7d.json)
is also retained: Java's hook-removal setup failed before removal in an area
reused by other redstone tests. The latest fixtures use separate footprints with
the same assertions. Earlier block-data probe failures remain in immutable reports,
including [command-rate pacing](../compatibility/results/evidence/b28649d22763cc505b6dc99d3eb724af2ee7e7f72283b79972f5c731ac616964.json)
and [privileged filtered-text expectations](../compatibility/results/evidence/b3573255d0cf696deaf510525891eea8cd504e3ff68a6b96dfc0b9e95ac9ae96.json).
The ledger has 27 partial and 173 unverified
families. A separate fresh Java fixture and both runtimes' cooking continuations
pass the selected furnace timer, speed and lava bucket checks in the
[furnace report](../compatibility/results/evidence/dafb048b8b4df8fad289cdf5e2b83e20d225e756b92d3293e40dd9a2ab91cc3b.json).
Zero-step preservation remains visible, with codec acceptance marked skipped;
changed chunk bytes alone cannot prove furnace decoding or encoding.

Rust tests also cover ordered player snapshots, admission through final player,
advancement and statistics saves, atomic persistence-write failures, and renewed chunk
tickets protecting live inventories. Remaining work includes command branches,
other redstone behavior, additional block-entity types and fields, general inventory
transactions, dynamic sign components, full multiplayer and gameplay coverage,
generation ordering, and measured multicore performance. The owned Java packet
lane now retains admitted continuations through session draining and the final
saves; complete input-ordering and gameplay coverage still need paired evidence.
Duplicate-login replacement
currently starts after configuration; vanilla begins it before configuration.

## Inventory scope

Older Ferraloom builds wrote private numeric counters in the `Statistics` tag of
`<world>/players/data/<uuid>.dat`. This version uses the canonical
`<world>/players/stats/<uuid>.json` and refuses a nonempty or malformed private tag,
including when no JSON exists. No migration tool is implemented. Back up the
entire world and retain both files; removing the tag would discard history, and
adding the two sets of counters can double count it. To keep using those players,
run the matching older build against a copy of its pre-upgrade world backup.
Absent or empty private tags are accepted. A refusal prevents player publication
and statistics saves; the server log identifies the UUID and canonical file.

Each family records its expected behavior, required reference scenarios, Rust status and evidence. Families cover protocol, authentication, world generation, dimensions, data packs, storage, block simulation, redstone, entities, AI, combat, inventories, recipes, player systems, commands and administration. Concurrency, determinism and multicore performance are tracked separately because they are Ferraloom requirements rather than vanilla features.

This is a family-level inventory. It does not enumerate every block state, item, entity, recipe, advancement, command branch or data-pack resource in the release. Registry and resource manifests extracted from the pinned official server must supply that lower-level coverage. Each new target registry entry needs an implementation mapping and reference scenario. Broad entries such as `blocks.new_version` explicitly track this outstanding work; their presence does not count as coverage for individual entries.

Public vanilla behavior is the reference: wire messages, persistent world formats, gameplay outcomes, timing and ordering where observable, and administrative behavior. Existing vanilla worlds must round-trip between servers. Fixed-seed generation and state traces should be compared directly; stochastic behavior needs documented sampling and acceptance criteria. Performance improvements require the same gameplay outcomes.

## Status and evidence

- `unverified`: no sufficient paired reference run establishes compatibility.
- `missing`: the feature is known to be absent.
- `partial`: some behavior exists, but required scenarios remain unmet.
- `verified`: every listed required scenario has matching, passing vanilla and Rust evidence for the pinned target.

A source inspection, upstream README, successful compilation or passing Rust-only test cannot verify vanilla parity. REA analysis of the pinned official executable can guide implementation and scenario design, but observed differential tests must establish the result. Record disassembly provenance and test environment with the artifacts when REA is used.

Evidence has this shape:

```json
{
  "test_id": "network.status_fields",
  "scenario": "status fields",
  "target_version": "26.3",
  "report_sha256": "<SHA-256 of the paired report snapshot>",
  "test_suite_sha256": "<SHA-256 recorded by the test suite>",
  "environment": "isolated reference and Rust processes; configuration and tool versions in artifacts",
  "vanilla": {
    "result": "passed",
    "build_id": "sha1:<pinned-official-server-digest>",
    "artifact": "compatibility/results/evidence/<report-sha256>.json"
  },
  "rust": {
    "result": "passed",
    "build_id": "sha256:<tested-Rust-binary-digest>",
    "artifact": "compatibility/results/evidence/<report-sha256>.json"
  }
}
```

`scenario` must exactly match one entry in the family's `required_scenarios`. Results can be `passed`, `failed` or `skipped`; only paired passes satisfy verification. Artifacts use repository-relative paths, including materialized CI artifacts. Store executable hashes, build configuration, JVM and Rust toolchain versions, seed, relevant settings, input trace, expected observations, actual observations and comparison rules with the run. For tests that do not involve world randomness, record the seed as inapplicable. Keep successful evidence only while it applies to the current Rust build; a code change that invalidates a scenario requires rerunning it or returning the affected family to `unverified` or `partial`.

Evidence counted toward `verified` requires report and suite SHA-256 hashes, the pinned official server SHA-1, and the tested Rust binary SHA-256. Strict modes always inspect materialized artifacts. They check report bytes against the recorded hash and bind each observation to its actual test ID, result, target version, suite hash and build IDs. The scenario map must link that test to the claimed feature and scenario. All required scenarios must pass on one Rust build, and a failing or skipped observation on that build cannot be hidden by another passing test for the same scenario. Hash checks detect changed evidence; they do not execute tests or authenticate who produced a report. The importer retains selected observations as `partial` and never promotes a whole family to `verified`.

## Runnable checks and completion gates

Run these from the repository root with Python 3.10 or newer:

```sh
python3 tools/validate_features.py
python3 -m unittest discover -s tests -p test_feature_inventory.py -v
```

The default check validates the inventory's structure, identifiers, scenario lists and evidence records. It succeeds for an honest inventory of unverified work. It rejects `verified` claims without a test identifier, matching target version and passing evidence for every required scenario.

The vanilla completion gate is:

```sh
python3 tools/validate_features.py --strict --check-artifacts
```

The complete Ferraloom feature-family gate also requires the concurrency and performance requirements:

```sh
python3 tools/validate_features.py --strict-all --check-artifacts
```

Both strict commands intentionally fail while required families remain unverified, missing or partial. Exit code `0` means the requested ledger check passed, `1` means required feature families are incomplete, and `2` means the file, schema or evidence is invalid. `--check-artifacts` checks referenced files and recorded SHA-256 hashes; it does not authenticate authorship or execute tests. A passing ledger gate is necessary but must accompany actual reference tests and complete extracted-registry coverage before claiming a working replacement.

Run the reference and Rust servers in disposable worlds and isolated test processes. Keep ports, saves and credentials separate from existing servers. A complete acceptance run needs vanilla-client login, gameplay across all dimensions, world interchange, restart and recovery, multiplayer interaction, malicious-input isolation, and reproducible load tests. It must retain paired artifacts rather than reporting an unqualified smoke-test pass.

## Plugins

Java JAR plugin compatibility is deferred until vanilla server integration and its acceptance gates pass. Plugin APIs, a possible JVM bridge, scheduler semantics, loading and failure isolation are separate work. A JAR filename does not establish compatibility with Bukkit, Spigot, Paper, Fabric, Forge or another Java plugin/mod API. Decide and document the intended API before implementing that phase. Deferred plugin rows never satisfy or block the vanilla completion gate.
