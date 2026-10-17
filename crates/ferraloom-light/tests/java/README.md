`bash crates/ferraloom-light/tests/java/generate.sh` runs on devbox against the
shared 26.3 jar with JDK 25. No Java or server binary is committed or needed by
Rust builds. Run from this checkout; output is under `tests/fixtures`.

The harness uses vanilla `ProtoChunk`, `ChunkSkyLightSources`, and
`LevelLightEngine`, with initialize/light pre/post update ordering taken from
`ThreadedLevelLightEngine`. It records constructed block changes and full sky
and block nibble arrays at each checkpoint, plus every queried nibble in the
surrounding section halo. Rust replays the same edits and compares all values,
section states, array presence, and homogeneous/allocation semantics. Disk
reloads pass the real engine's arrays into a new engine through queueSectionData.
The no-skylight lifecycle scenario also covers retention, removal/recreation,
queued replacement data, disabling and enabling emission.

`faces.bin` checks both face-occlusion predicates for every ordered pair of
representative extracted occlusion shapes in all six directions. This verifies
the exact face-mask representation independently of propagation scenarios.

The binary scenario stream starts with FL26, section bounds and a sky flag;
then typed commands (initial block, initialize, propagate, checked block, compare,
enable, section status, retain, queue, reload, end). Integers are big-endian.
Each compare visits chunks -2..2 on X/Z, the height range with one section of
padding above and below, and block then sky layers. It stores section type,
optional layer (homogeneous value or 255 for allocated, then bytes), and queried
bytes. Arrays use lossless runs: i32 run count, then (u16 length, u8 value).
