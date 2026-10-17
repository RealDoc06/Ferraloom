# Ferraloom development

In Doc's managed environment, run heavy Rust builds, native checks and worktree
checks on the dedicated `ferraloom-builder`. Keep devbox for editing and
lightweight work. Identify the actual execution host in validation reports.

From a clean, committed Ferraloom checkout on devbox:

```sh
/home/doc/.local/bin/ferraloom-build -- cargo check --manifest-path vendor/pumpkin/Cargo.toml --locked --offline --jobs 1 -p pumpkin-core --lib --target-dir target
```

The helper sends the exact Git HEAD to `/home/doc/code/Ferraloom` on the builder,
serializes synchronization and execution, and reuses its fixed target cache. It
refuses dirty checkouts and concurrent jobs. Commit changes before requesting a
check; uncommitted edits are never sent. Do not create per-worktree target
directories or bypass a held lock by editing the builder checkout directly.

The wrapper is `/home/doc/.local/bin/ferraloom-build`; configuration is
`/home/doc/.config/ferraloom-build.json`. SSH alias `ferraloom-builder` targets
the dedicated unprivileged CT107 on nodo-3, separate from devbox. Jobs use 6 GiB
memory, no swap, 200% CPU, one Cargo job and `CARGO_INCREMENTAL=0`. Keep checks
locked and offline; root and vendored commands share `target`. Preserve existing
test counts, assertions and timeouts.

Build outputs and JSON command receipts stay on the builder, including
`.cache/remote-build-jobs/`. Retain exact source, actual host, command, raw logs
and binary hash with reported results. A successful targeted command does not
establish a full native or compatibility pass. Full validation helpers are
additional ignored, hash-pinned inputs, not part of a Git snapshot. Transfer
them explicitly, use fresh receipt names and preserve their execution gates.

Runtime supervisors with devbox-specific gates require a reviewed remote
profile before use on the builder. Do not spoof a host or silently remap
receipts. Record the true engine build and any separate client harness commit.
Client-only changes with a proven empty engine/build-input diff do not require
rebuilding unchanged Rust.

Preserve fixtures, worlds, player data, reference JARs, primary evidence, logs
and receipts. Remove only specifically verified disposable closed artifacts.
Let an active validation run finish before relocating later work. Do not alter
Proxmox, other containers or live services as part of project development.
