# Third-party notices

## Horizon

Ferraloom is built on Horizon v0.2.93 by Ludovic Marie and the Horizon contributors, under the MIT license
(`LICENSE`). Its repository is no longer public; this one carries its full history.

## Bukkit and Spigot API

Ferraloom's plugin bridge (`crates/ferraloom-plugins/java/`) is compiled against, and runs with, the Spigot
API (`org.spigotmc:spigot-api`, the build pinned in `crates/ferraloom-plugins/java/api.lock.json`), which is
licensed under the GPL-3.0. Ferraloom does not contain or redistribute it: `build.py` fetches it from Spigot's
repository. The bridge's own files are MIT; a compiled bridge is conveyed under the GPL-3.0's terms. See
"Licensing" in `docs/ferraloom/plugins.md`.

## Rust crates

The server links the crates in `Cargo.lock`, each under its own license (MIT, Apache-2.0, BSD, ISC, Zlib,
MPL-2.0, Unicode-3.0, BSL-1.0, CDLA-Permissive-2.0). `cargo metadata` lists them.
