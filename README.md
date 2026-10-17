# Ferraloom

Minecraft Java **26.3** server in Rust, with parallel CPU workers and a version-pinned vanilla compatibility suite.

Ferraloom uses the [Pumpkin Rust engine](https://github.com/Pumpkin-MC/Pumpkin/tree/5b53f064af8708ba33fcaa533b81ca29a53a3310), with a pinned base and [locally maintained engine modules](vendor/pumpkin/UPSTREAM.md). This repository owns the executable, compatibility fixes, operational defaults, feature inventory, and independent reference tests. Full vanilla parity is **not established**. The [feature ledger](compatibility/features.json) and [recorded results](compatibility/results/latest.json) are the acceptance criteria, including failures and untested behavior.

## Run

Requires Rust 1.98.1, a native C toolchain, Git, and internet access for the first build. The build extracts vanilla data from the official 26.3 server. Java is needed for reference tests, not Ferraloom's runtime.

```sh
python3 tools/oracle.py fetch
PUMPKIN_MINECRAFT_SERVER_JAR="$PWD/.cache/vanilla/26.3/server.jar" cargo build --release --locked
./target/release/ferraloom --data-dir run/server --bind 0.0.0.0:25565
```

Join with Minecraft Java 26.3 at your server host's address, port 25565. Account authentication is enabled by default. A loopback listener is accessible only on the server host itself.

The first run writes `pumpkin.toml` into the data directory. Start with [the example](examples/pumpkin.toml) for explicit settings. `--workers 4` sets four workers in each of Ferraloom's Tokio and Rayon pools; the engine also maintains chunk-generation pools. `stop`, Ctrl-C, and SIGTERM save and shut down the server. `--offline` is for isolated testing.

Plugins, Bedrock listeners, and upstream telemetry are disabled. Java `.jar` plugin integration is deferred until the vanilla completion gate passes.

## Compare with vanilla

Requires Python 3.11+, Node.js 24+, and Java 25. Both servers run sequentially in fresh ignored directories, use a fixed seed, and bind only to `127.0.0.1` on ephemeral ports. The probe implements the actual 26.3 protocol; it does not substitute an older Minecraft client schema.

```sh
cargo build --locked
# This flag accepts the Minecraft EULA for the local reference run.
python3 tools/parity/suite.py --accept-eula --java /path/to/java25/bin/java
# Independently check the default LZ4 world format with the official decoder and server.
python3 tools/check_default_storage.py --accept-eula --java /path/to/java25/bin/java --rust-binary target/debug/ferraloom
# Check saved furnace timers, cooking speeds and lava bucket remainders.
python3 tools/check_furnaces.py --accept-eula --java /path/to/java25/bin/java --rust-binary target/debug/ferraloom
# Check live menus, sign edits, command permissions and clean restarts.
python3 tools/check_block_data.py --accept-eula --java /path/to/java25/bin/java --rust-binary target/debug/ferraloom
```

The main suite writes `compatibility/results/latest.json` and fails when any selected scenario fails. Auxiliary checks write separate reports under `.cache/`. The restart and world-interchange checks include unflushed blocks, a mob's UUID and health, a frozen clock and game rule, scoreboard state, and namespaced command storage. Separate cases exercise recursive data merging, redstone removal, chunk activation during freeze, and scheduled fluid ticks beyond entity-ticking range. The client also decodes the exact release's advertised command tree; syntax presence does not establish working commands. `--record` preserves failures in the report while allowing a development recording run to exit successfully. Neither mode declares full compatibility. Raw logs and test worlds stay in `.cache/runs/`.

The latest complete [native checks](compatibility/results/native-checks-b58452a.json) passed 1,676 Rust, 168 Python and 74 Node tests across all 16 gates at clean engine source `b58452a` on the dedicated `ferraloom-builder`. This includes the 20 new absolute-tick controls, inventory tests, strict TypeScript, both formatting checks, Clippy for the executable and all seven vendored crates, and the final build. GameTest was compiled only; four isolated Core child results are not added to the total. The published `3ce0503` contains that tested engine with unchanged engine/build inputs; CI completion for that publication is unobserved. No paired clone or runtime campaign at `b58452a`, general clean-unload parity or full vanilla parity follows from these native checks.

Separate [targeted clone-foundation checks](compatibility/results/native-clone-foundation-9560ee2.json) passed all-targets Clippy for `pumpkin-world` and `pumpkin-core`, then 888 Core library tests: 18 new controls and 870 existing tests. Four isolated one-test/887-filtered child results are excluded. These ran on `ferraloom-builder` at `9560ee2`; the same 12 patches and seven changed file blobs/modes were integrated at `0749182`. This covers the unwired World/BE foundation; complete clone and menu/factory integration remain separate.

Separate [bounded-parser checks](compatibility/results/parser-e7e0009.json) passed command all-targets Clippy and 114 command-library tests at `e7e0009`: 13 new and 101 existing, without child results. Its three exact patches were integrated at `3839c8e`. These results are separate from Core 888 and historical B584 full16; they establish no combined native run, server executable build, runtime/parity or ledger promotion. At that source, resource-prefix behavior remained open. Canonical aliases, SNBT, factory, negative-zero and general Java Property-typed equivalence remain open.

Separate [resource-suggestion checks](compatibility/results/resource-suggestions-bdcbd17.json) passed command all-targets Clippy and all 117 command-library tests at `bdcbd17`: three new controls and 114 existing tests. The three exact patches and one changed file were integrated at `6fd0f96`. This adds bounded ASCII resource matching; Unicode, Java Property-typed equivalence and full parser behavior remain open. The earlier `3839c8e` server executable predates these changes; no current server build, full-native or runtime/parity result follows.

The [earlier B native record](compatibility/results/native-checks.json) remains at 1,647 Rust, 151 Python and 74 Node tests on engine `b8e01b4`. Both recorded GitHub `Check` runs for the later `97b584f` publication [completed successfully](compatibility/results/evidence/37733874e3bc824027b7b570b52a463890f705460eb357cf8b3c355407bdb6df.json); those CI results apply only to that commit. The integrated client harness separately [passed all 162 Python tests](compatibility/results/evidence/dce064e4df04b9fcb58ef69069706fb9829ea3e63478d3231e8ee05a236a58c0.json) with engine/build inputs unchanged from B. The selected B runtime campaigns below remain bound to their recorded engine and separate client/bridge sources, including Jukebox B/J; their results do not automatically apply to `b58452a`.

The [selected paired crafting campaign](compatibility/results/crafting.json) at engine and client source `b8e01b4` passed all 13 ordinary crafting composites: 13 Java and 13 Rust case rows, equal recorded outcomes, and two clean server and two clean client exits on `ferraloom-builder`. The Java reference used `sync-chunk-writes=false`; Rust configuration was unchanged. This establishes only those selected cases under that buffered Java profile.

The [selected Dragon campaign](compatibility/results/dragon-parts.json) passed 25 bounded comparisons across four fresh boots and four clean clients, using engine `b8e01b4` and harness `b2be7cb`. It covers frozen phase/NBT observations, origin-part attacks with positive health loss, and shared death-clock milestones through removal at tick 200. Exact damage multipliers, moving multipart geometry and full Dragon AI remain open.

A fresh [genuine REA comparison](compatibility/results/rea-crafting.json) matched one declared `table_offset_right` gameplay trace using engine/client `b8e01b4` and bridge `ec8da88`. Both captures exited cleanly. The complete generic REA comparison remains changed, with its process dimension unknown; the model was defined after discovery. This establishes only the declared trace under the buffered Java profile.

The [current-engine player interchange](compatibility/results/player-interchange.json) passed seven selected composites in each direction on exact engine/client `b8e01b4`: Java to Rust and Rust to Java, four clean boots and six clean clients. Raw stopped player saves, named statistics and captured statistics packets were independently checked. This covers selected current-format identity, inventory, vitals, XP and advancement/statistics continuation under the buffered Java reference profile; it does not establish full player-data compatibility.

The [current-engine item interchange](compatibility/results/item-interchange.json) passed five selected composites in each direction across six clean boots, six clean clients and 18 lifecycle checkpoints; [container metadata](compatibility/results/container-metadata.json) passed 26 cases per server across four clean boots, four clean clients and 12 checkpoints, both on exact engine/client `b8e01b4`. The shared [ROOT proof](compatibility/results/evidence/d9c8cb60979ce2d554c38ffb9876be3e8034328512416b3b914eca5dc03a9f5f.json) covers 62 rows, ten boots and ten clients, including four final raw player saves, two saved container chunks and 176 independently decoded menu payloads. These selected cases use buffered Java chunk writes with Rust unchanged; they add no native test count and establish no item wire decoding, general item gameplay or full-family compatibility.

The [current-engine chat and horn results](compatibility/results/chat-horns-jukebox-B.json) passed five chat-throttle cases per server, with four separate default-setting prerequisites, and ten horn composites across all four boots on exact engine/client `b8e01b4`. The combined campaign failed during jukebox import: all twelve jukebox composites remain failed, with only Java source completed. Actual totals were 12 boots, 32 clients and 36 lifecycle checkpoints, against planned 14/36/42. ROOT checked the preserved originals offline; this added no runtime or native test count. Java used `sync-chunk-writes=false` with Rust unchanged. Historical jukebox/regression records remain unchanged; full-family, default-sync, crash-durability and REA claims remain open.

A separate [selected Jukebox campaign](compatibility/results/jukeboxes-B-J.json) passed all 12 composites on engine `b8e01b4` with client `5bcd504`: four clean boots, eight clean clients, 12 lifecycle checkpoints and 36 `play_record` events. ROOT checked all 1,373 exported originals offline. This buffered-Java result covers the selected controls only; the earlier combined failure remains unchanged, and no feature family or native test count is promoted.

The [historical paired Jukebox campaign](compatibility/results/jukeboxes.json) passed 12 selected composites across four boots and eight clients at `97db9ed`. The [historical regression attempt](compatibility/results/regression-checks.json) on that engine failed when `save-all flush` hit a pending POI write during Rust-to-Java player setup. All seven Java-to-Rust player composites passed; reverse transfer stopped before Java booted. The other four suites did not start. These historical results are unchanged. No feature family is promoted: the ledger remains at 27 partial and 173 unverified families, with zero verified.

Earlier targeted runs passed player and item interchange in both directions, 26 container-metadata cases and five chat-throttle cases per server, with 20 clean boots and 36 clean client sessions. The [compatibility contract](docs/compatibility.md) identifies their older source and immutable reports. The retained [main-suite run](compatibility/results/latest.json) passed 96 selected cases and both world transfers, but missed four gold-ore blocks and failed command-tree comparison. A separate [34-case block-data check](compatibility/results/block-data.json) passed on that older build.

```sh
python3 tools/oracle.py inventory --java /path/to/java25/bin/java
./target/debug/ferraloom --export-registry .cache/rust-registry.json
python3 tools/oracle.py compare-registry
python3 tools/validate_features.py --check-artifacts
# Full vanilla completion, intentionally fails while required features lack evidence:
python3 tools/validate_features.py --strict --check-artifacts
```

The [feature checklist](docs/features.md) contains all 200 feature families with separate vanilla and Rust observations. The machine-readable inventory also includes official registry identifiers, block-state counts, command roots, and datapack resource paths. Registry presence and successful startup do not prove game behavior. Read [the compatibility contract](docs/compatibility.md) and [REA investigation](docs/reverse-engineering.md).

## Development

```sh
cargo fmt --all -- --check
cargo fmt --manifest-path vendor/pumpkin/Cargo.toml --all -- --check
cargo test --locked
cargo test --locked -p pumpkin-command --lib
cargo test --manifest-path vendor/pumpkin/Cargo.toml --locked -p pumpkin-data --lib --target-dir target
cargo test --manifest-path vendor/pumpkin/Cargo.toml --locked -p pumpkin-protocol --lib --tests --target-dir target
cargo test --manifest-path vendor/pumpkin/Cargo.toml --locked -p pumpkin-world --lib --target-dir target
cargo test --manifest-path vendor/pumpkin/Cargo.toml --locked -p pumpkin-core --lib --target-dir target -- --test-threads=2
cargo check --manifest-path vendor/pumpkin/Cargo.toml --locked -p pumpkin-gametest --all-targets --target-dir target
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --manifest-path vendor/pumpkin/Cargo.toml --locked --workspace --all-targets --target-dir target -- -D warnings
npm ci
npm run test:protocol
npm run check:protocol
python3 -m unittest discover -s tests -p 'test_*.py'
```

Keep `Cargo.lock`: the pinned RSA prerelease requires the compatible `pkcs1` prerelease recorded there. Dependency changes must pass a build and the reference suite. Debug builds are optimized lightly for development; use release builds for performance measurements. Worker counts alone do not establish a speedup.

GPL-3.0-only. See [LICENSE](LICENSE) and [NOTICE](NOTICE) for engine attribution and separate upstream asset notices. Official server downloads, worlds, and full disassembly dumps are excluded from Git. Small, attributed bytecode excerpts used as test witnesses are retained with their provenance.
