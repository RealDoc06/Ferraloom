# Ferraloom

A from-scratch Rust port of the Minecraft Java Edition **26.3** dedicated server
(protocol 777). The goal is a server where everything vanilla does works,
ticked across many CPU cores instead of one. Java `.jar` plugin support comes
after vanilla parity.

Status: early. The port is starting from the foundation crates. See
[`FEATURES.md`](FEATURES.md) for what works, tested against vanilla, and
[`docs/architecture.md`](docs/architecture.md) for how it is built.

## Build

```sh
cargo build --release --locked
./target/release/ferraloom
```

## Develop

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The previous Pumpkin-based prototype is preserved on the `archive/gpt-pumpkin`
branch.

GPL-3.0-only. See [LICENSE](LICENSE).
