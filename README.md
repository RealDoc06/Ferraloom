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

The latest targeted batch passed player interchange and item persistence in both directions, plus 26 container-metadata cases and five chat-throttle cases on each server. All 20 server boots and 36 client sessions exited cleanly. The [compatibility contract](docs/compatibility.md) links the exact source, executable and immutable reports, including earlier failures. The retained [main-suite run](compatibility/results/latest.json) passed 96 selected cases and both world transfers, but missed four gold-ore blocks and failed command-tree comparison. A separate [34-case block-data check](compatibility/results/block-data.json) passed. Those broader runs used an older build. No complete feature family is verified.

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
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --manifest-path vendor/pumpkin/Cargo.toml --locked --workspace --all-targets --target-dir target -- -D warnings
npm ci
npm run test:protocol
npm run check:protocol
python3 -m unittest discover -s tests -p 'test_*.py'
```

Keep `Cargo.lock`: the pinned RSA prerelease requires the compatible `pkcs1` prerelease recorded there. Dependency changes must pass a build and the reference suite. Debug builds are optimized lightly for development; use release builds for performance measurements. Worker counts alone do not establish a speedup.

GPL-3.0-only. See [LICENSE](LICENSE) and [NOTICE](NOTICE) for engine attribution and separate upstream asset notices. Official server downloads, worlds, and full disassembly dumps are excluded from Git. Small, attributed bytecode excerpts used as test witnesses are retained with their provenance.
