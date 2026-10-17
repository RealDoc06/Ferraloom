# Ferraloom development

Ferraloom is a from-scratch Rust port of the official Minecraft Java Edition
26.3 server (protocol 777, data version 5023). The goal is a server where
everything vanilla does works, ticked across many cores. Java `.jar` plugin
support comes only after vanilla parity. Read `docs/architecture.md` before
writing code.

## Vanilla is the spec

- Port behavior from the decompiled server in `.cache/decompiled/26.3/`
  (`tools/decompile.sh` regenerates it). The 26.3 jar is unobfuscated, so class
  and method names are Mojang's own. Use REA (`npx -y rea-agents`) when
  decompiled code is not enough, for example to trace runtime behavior.
- Never commit decompiled Java, the server jar or other Mojang binaries. Write
  idiomatic Rust. Do not transliterate Java line by line.
- Keep vanilla's names for ported concepts so code can be cross-referenced.
  Put a short doc comment on ported items naming the source, for example
  `/// Port of ServerLevel#tickChunk.`
- When behavior is unclear, run the vanilla server and observe it. Do not guess.
- Shared caches live only in the main checkout, never in worktrees:
  decompiled source `/home/doc/code/Ferraloom/.cache/decompiled/26.3/`,
  vanilla jar, libraries and data generator reports
  `/home/doc/code/Ferraloom/.cache/vanilla/26.3/`, JDK 25
  `/home/doc/code/Ferraloom/.cache/toolchains/jdk-25.0.4.1+1/`. Running small
  Java programs against the vanilla jar on devbox, for example to capture
  expected test values, counts as light work.

## Tests

- Unit tests live next to the code. Parity tests run the same scenario against
  vanilla and Ferraloom and must agree (`crates/ferraloom-parity`).
- `FEATURES.md` lists every vanilla feature with its vanilla and Ferraloom test
  status. Update it in the same commit that changes a feature.
- Tests must be focused. No smoke-test spam, receipts, evidence ledgers or
  hash-pinned logs. A test passes or it fails.

## Builds

Devbox is for editing and light work. Run Rust builds and tests on the
dedicated builder through the wrapper, from a clean, committed checkout:

```sh
ferraloom-build -- cargo test --workspace --locked
```

The wrapper (`/home/doc/.local/bin/ferraloom-build`, config
`/home/doc/.config/ferraloom-build.json`) syncs your exact `HEAD` to the builder
(`ferraloom-builder`, CT107 on nodo-3) and runs the command there. Jobs queue
and run one at a time with a shared `target` directory. Commit before building,
because uncommitted edits are never sent. Do not create per-worktree target
directories, and do not edit the builder checkout directly.

Do not alter Proxmox, other containers or live services.

## Git

- Conventional commits, for example `feat(protocol): encode chunk data packets`.
- Keep commits small and focused. Work on your own branch; the parent agent
  merges it.
- Add external dependencies in your crate's own `Cargo.toml`, not in
  `[workspace.dependencies]`, so parallel branches merge cleanly.
- Stay inside the paths your brief assigns you. Ask the parent before touching
  anything else.

## Delegation

Delegated agents send implementation, architecture and scope questions to their
parent agent through agent or thread messages. Do not open user-question UI for
these decisions. The parent answers pending child requests and owns routine
project decisions; only the parent may escalate essential questions to Doc.
Repeat this routing rule in every delegation brief.
