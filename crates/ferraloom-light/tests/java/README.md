`bash crates/ferraloom-light/tests/java/generate.sh` runs on devbox against the
shared 26.3 jar with JDK 25. No Java or server binary is committed or needed by
Rust builds. Run from this checkout; output is under `tests/fixtures`.

The harness uses vanilla `ProtoChunk`, `ChunkSkyLightSources`, and
`LevelLightEngine`, with initialize/light pre/post update ordering taken from
`ThreadedLevelLightEngine`. It records constructed block changes and full sky
and block nibble arrays at each checkpoint, plus every queried nibble in the
surrounding section halo. Rust replays the same edits and compares all values,
section states, array presence, homogeneous/allocation semantics, and the exact
section notification traversal order. A batched-edit case grows the pending-node
set beyond its initial capacity. Disk reloads pass the real engine's arrays into
a new engine through queueSectionData.
The empty-section boundary case preserves vanilla's source-scan reset after a
slab on the bottom edge. The queued-edit case checks Java DataLayer aliasing
between retained/queued and newly initialized sections. The no-skylight lifecycle
scenario also covers retention, removal/recreation, queued replacement data,
disabling and enabling emission.

`faces.bin` checks both face-occlusion predicates for every ordered pair of
60 representative effective occlusion shapes in all six directions. This verifies
the exact face-mask representation independently of propagation scenarios.
`shape_flags.bin` checks the empty-shape queue flag for all 35,723 registry states.

The binary scenario stream starts with FL26, section bounds and a sky flag;
then typed commands (initial block, initialize, propagate, checked block, compare,
enable, section status, retain, queue, reload, end). Integers are big-endian.
Each compare visits chunks -2..2 on X/Z, the height range with one section of
padding above and below, and block then sky layers. It stores section type,
optional layer (homogeneous value or 255 for allocated, then bytes), and queried
bytes, followed by the ordered section-notification list. Arrays use lossless
runs: i32 run count, then (u16 length, u8 value).
