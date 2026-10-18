# Pathfinder core vectors

Run `python3 generate.py` on devbox with the pinned 26.3 vanilla cache and JDK25.
The harness invokes the real Node, Target, Path, BinaryHeap and PathFinder;
its NodeEvaluator supplies an explicit graph without a simulated Mob or world.

The 72 cases cover the 27 PathType IDs/default maluses and invalid ID fallback;
node hashes, overflow, distances, clone metadata and debug bytes; equal-cost and
special-float heap order, changes, removal, stale indices/backing slots after clear and growth
past128; shared Path copy/list/node identity with independent and overflowing
cursors; target best-heuristic ties; and search budgets, strict distance bounds,
reach ranges, multiple targets, hash aliases, weighted heuristics, maluses,
unreachable fallbacks, prepare/done order and Warden's horizontal distance.
Rust unit tests consume every case and compare full state and debug bytes.

PathData is the explicit owner of NodeId and PathNodesId identities. Keep it
alongside returned paths and their cursor copies. The pure slice has no live
Mob, chunk acquisition, movement-control or navigation integration yet.

Run `python3 pathfindability.py` to capture the real `isPathfindable` virtual
for every registered block state and all three PathComputationTypes. The
compressed vectors cover 35,723 states, including all 118 concrete override
classes and their 60 native bodies. Another 64 liquid-state records distinguish
source from flowing fluid membership under real registry tag reloads. These
queries use no live block, chunk or entity data.
