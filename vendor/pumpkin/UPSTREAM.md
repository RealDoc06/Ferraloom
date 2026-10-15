# Engine modules maintained by Ferraloom

Source: https://github.com/Pumpkin-MC/Pumpkin.git

Base commit: `5b53f064af8708ba33fcaa533b81ca29a53a3310`.

Only `pumpkin-core`, `pumpkin-world`, and `pumpkin-command` are vendored. Cargo patches the pinned Git dependency with these modules. Other engine crates keep their original pinned Git source and shared type identities. The nested workspace inherits upstream package, lint, and dependency settings, with unvendored Pumpkin paths redirected to that exact revision.

The world's two generated feature-definition includes are copied from the same base revision's `pumpkin-data` sources so the world crate can compile independently. Referenced upstream test fixtures, the status icon, and management schema are retained with the asset notices.

The world crate extracts its embedded world-preset JSON and an igloo template test fixture into Cargo's build output from the checksum-verified official 26.3 server, using `PUMPKIN_MINECRAFT_SERVER_JAR` or the repository's ignored reference cache. This replaces source-tree includes that depended on the upstream data crate's extraction location. Official executables and extracted datapacks are not committed.

Local patches cover status connection handling, command block states and predicates, forced chunk loading, datapack discovery, 26.3 world storage, and ore-generation arithmetic. Further patches implement execute score stores, clock restoration and saving, ordered entity saves, retryable region writes, explicit flush completion, and Java LZ4 stream terminators with bounded compression levels. Anvil region writes synchronize the file before reporting completion. Stronghold cache initialization runs its small one-time calculation sequentially to avoid Rayon workers re-entering the same `OnceLock`; chunk generation remains parallel. The nested lockfile pins the additional development dependencies needed to run engine-module tests independently, with the same local module patches as the root build.

Upstream license and asset notices are retained. Ferraloom's differential suite records behavior changes against the official server. See the Git history for local patches.
