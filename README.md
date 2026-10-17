# Ferraloom

An independently implemented Minecraft Java server in Rust. Target: Java **26.3**, protocol **777**, data version **5023**.

The independent engine is under construction. It does not yet provide a playable world or vanilla compatibility. Transport, connection setup, world simulation and gameplay are being implemented directly here. Ordinary Rust libraries provide the async runtime, serialization and cryptography. No existing Minecraft server engine is embedded or ported.

The earlier Pumpkin-based prototype is preserved on branch `archive/pumpkin-prototype-ee7`. Reports under `compatibility/` describe that historical engine and do not count as evidence for this implementation. The independent server starts with no verified feature families. Existing external reference/client tools remain available for testing new behavior against the official pinned server.

## Development

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
```

In Doc's environment, run Rust checks on the dedicated builder using the wrapper documented in [AGENTS.md](AGENTS.md). Java is a reference-test dependency, not a server runtime dependency. Plugins remain deferred until vanilla compatibility is established.

GPL-3.0-only. Minecraft is a trademark of Mojang Studios; this project is independent and unofficial.
