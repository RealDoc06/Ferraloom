# Vanilla compatibility

Ferraloom targets **Minecraft Java Edition 26.3**, the stable release returned by Mojang's [official version manifest](https://piston-meta.mojang.com/mc/game/version_manifest_v2.json) when checked on 2026-10-15. The pinned [version metadata](https://piston-meta.mojang.com/v1/packages/11f1aefe473eefba0af62e3286c33b692807b2eb/26.3.json) records release time 2026-09-15T11:23:02+00:00. Snapshots are excluded. Updating the target is an explicit change that requires new reference artifacts and evidence.

The machine-readable ledger is [compatibility/features.json](../compatibility/features.json). It contains 200 feature families: 191 vanilla server families, seven additional concurrency and performance requirements, and two deferred plugin families. **No family is currently verified.** Partial rows retain selected paired observations; the [checklist](features.md) shows each implementation's results. This inventory and its validator do not establish complete implementation of those families.

The latest selected run tested source `571b6ca8ee705ff09a170d0c494972361e940bfe`
with Rust executable SHA-256
`1628b037014ed5a52c2c930bc677af8e82f6df90bbcbf3cef1dd730bf05edae0`.
Vanilla passed 67 of 67 scenarios; Ferraloom passed 65 of 67. Mob team
selection failed both before and after restart, and the vanilla-to-Rust world
check failed on that same team predicate. Rust-to-vanilla interchange passed.
The decoded command trees differ by two missing roots (`jfr`, `perf`) and 543
missing advertised literal paths with argument names normalized. This checks
advertised syntax, independently of command execution. All 16,384 block states
in the selected underground chunk sample matched; that single sample does not
establish general generation parity. The immutable
[paired report](../compatibility/results/evidence/b2f55868c867c1be9c297f2ad9faa2b63d9b1210b332eb22113660032dc20cf3.json)
retains every failure. The ledger has 19 partial and 181 unverified families.

## Inventory scope

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

Both strict commands intentionally fail while required families remain unverified, missing or partial. Exit code `0` means the requested ledger check passed, `1` means required feature families are incomplete, and `2` means the file, schema or evidence is invalid. `--check-artifacts` checks that referenced files exist inside the repository; it does not authenticate their contents or execute tests. A passing ledger gate is necessary but must accompany actual reference tests and complete extracted-registry coverage before claiming a working replacement.

Run the reference and Rust servers in disposable worlds and isolated test processes. Keep ports, saves and credentials separate from existing servers. A complete acceptance run needs vanilla-client login, gameplay across all dimensions, world interchange, restart and recovery, multiplayer interaction, malicious-input isolation, and reproducible load tests. It must retain paired artifacts rather than reporting an unqualified smoke-test pass.

## Plugins

Java JAR plugin compatibility is deferred until vanilla server integration and its acceptance gates pass. Plugin APIs, a possible JVM bridge, scheduler semantics, loading and failure isolation are separate work. A JAR filename does not establish compatibility with Bukkit, Spigot, Paper, Fabric, Forge or another Java plugin/mod API. Decide and document the intended API before implementing that phase. Deferred plugin rows never satisfy or block the vanilla completion gate.
