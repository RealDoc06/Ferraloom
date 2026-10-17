# Vanilla data extraction

Run `tools/extract-data.sh` from any checkout. It uses Java 25 (`JAVA_HOME`,
then the shared cached JDK, then `PATH`), curl, Python 3 and sha1sum. Downloads,
extracted libraries, compiled Java and report-generator logs stay in the primary
checkout's `.cache/`; `FERRALOOM_CACHE_DIR` can override that cache root.
`tools/fetch-vanilla.sh` downloads and verifies the pinned official bundler and
runs its report generator. `tools/decompile.sh` additionally verifies the Maven
Central Vineflower 1.12.0 checksum and reconstructs the reference source tree.

The extractor bootstraps vanilla and binds its default registry components with
`VanillaRegistries.createWorldLookup` and `DATA_COMPONENT_INITIALIZERS`. Item
components use vanilla's registry-aware `DataComponentMap.CODEC`. JSON objects
have recursively sorted keys; registry arrays retain protocol order. No Mojang
binaries, assets or Java sources are committed.

Shapes are AABB lists in block-local coordinates, queried with
`EmptyBlockGetter.INSTANCE`, `BlockPos.ZERO`, and an empty collision context.
The shared shape table deduplicates collision, outline and occlusion shapes.
`context_dependent_shape` is true for dynamic shapes and position offsets.
These states require live queries in game code; the extracted shape is only an
empty-world baseline. Entity-dependent collision contexts also require live
queries (for example scaffolding, powder snow and water walking); the extractor
marks these separately via the same conservative context flag. State facts are
retained individually because sounds, map colors, destroy time and flags can
vary within a block. `opacity` is vanilla 26.3's `getLightDampening`.

Rust builds read only committed JSON. `ferraloom-data` generates typed registry
IDs, packet enums, property tokens and constant lookup tables in `OUT_DIR`.
A property's type covers all its vanilla domains: for example `Facing` includes
up/down, and `East` covers boolean connections, redstone sides and wall heights.
`BlockState::with` validates the receiving block's domain and returns `None` for
unsupported values. Numeric property values are `u8`, boolean values are `bool`.
Other values are generated enums; none require string lookup during simulation.

`ExtractTagKeys` separately records tag key names and their registry identities,
including vanilla datapack tags without a Java constant, in `tag_keys.json`.
`tools/extract-data.sh` regenerates it. Typed keys have per-registry ordinals;
each loaded `TagSet` resolves them once and stores membership in bitsets. Custom
datapack tags resolve by identifier to a dense `TagId` before simulation reads.
