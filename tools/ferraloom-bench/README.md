# Ferraloom bench

Explorer load tests: Horizon-based builds against vanilla and Paper on `vm-minecraft` (10.10.10.101,
`~/horizon-bench`, port 25570; never touch its Velocity proxy on 25565), with `horizon-bots` on Doc's Windows laptop.

| File | Runs on | What |
|---|---|---|
| `deploy.sh <sha>` | devbox | Copies the builder's `horizon-server` and `horizon-client` to `builds/<sha>` on the VM, with the builder's loader and libc beside them (the VM's glibc is older). Build first: `ferraloom-build -- cargo build --release --locked -p horizon-client --no-default-features -p horizon-server`. |
| `run_explore.sh` | VM | One server at a time per `bots:speed` config (`CONFIGS`, `SERVERS`, `BIN`, `R`, `HORIZON_SPIKES`), through `tools/server_bench.py` with the Windows patch from phase B; counts chunks by status after each run and deletes the world. |
| `relay_vm.py`, `relay_next.py`, `relay_worker.py` | VM, VM, laptop | The bots relay: the VM queues `horizon-bots` command lines, the laptop worker claims them over outbound SSH and streams stdout back. |
| `count_chunks.py <world>` | VM | Chunks per generation status in the region files (`minecraft:full` is a finished chunk). |
| `spans_summary.py <server log>` | VM | Mean ms per span over `HORIZON_SPIKES` slow-tick lines. |
| `ab_table.py <result dirs>` | VM | One row per build and bot count. |
| `fetch_mc.py <dir>` | anywhere | A launcher-style 26.3 game folder from Mojang's piston-meta, SHA-1 checked (for `horizon-prepare`). |

Example, the profile of one build at 25 explorers flying 11 blocks a second:

    HORIZON_SPIKES=60 BIN=~/horizon-bench/builds/<sha> R=~/horizon-bench/profile-<sha> SERVERS=horizon CONFIGS=25:11 ./run_explore.sh
    python3 spans_summary.py ~/horizon-bench/profile-<sha>/horizon-25-11-artifacts/horizon-explore-25.log

Baseline on the VM (4 vCPU Xeon E5-2660 v3), explorers at 11 blocks a second, v0.2.93 built on the builder:
MSPT 116 / 149 / 191 at 25 / 50 / 100 bots; with the light-region fix and look-ahead generation 88 / 132 / 187.
