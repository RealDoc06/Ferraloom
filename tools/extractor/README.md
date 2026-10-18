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
`voxel_shapes.json` additionally preserves native shape representations,
coordinate-list kinds, raw double bits, discrete occupancy and cached bounds.
Its `snapshots` table deduplicates numeric data; `shapes` indexes snapshots while
preserving distinct Java object identities. Every `states` row contains shape
indices for collision, outline, occlusion, then the six cached occlusion faces
in Direction order (down, up, north, south, west, east). `context_dependent`
retains the state flags. Hex strings encode unsigned double/bitset words without
decimal parsing loss. `ExtractData <output-dir> --voxel-shapes-only` regenerates
only that file from an already bootstrapped jar/classpath.

`BlockState::collision_shape` and `shape` return `None` for flagged states.
`cached_shapes` exposes their explicitly marked empty-world baselines;
`occlusion_shape` and `face_occlusion_shape` expose vanilla's actual state caches.
The legacy AABB tables remain because game movement and lighting still use them.
Regenerate the independent Rust parity fixture with
`python3 crates/ferraloom-util/tests/java/shapes/generate.py --registry-caches`.

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

`ExtractData <output-dir> --sound-events-only` records each registered
`SoundEvent.fixedRange` in protocol order. This is pure immutable registry data;
`SoundEvent#getRange` uses the fixed value when present, otherwise 16 or
`16 * volume` with native float arithmetic. All 1,991 registered 26.3 events
currently use variable range. Direct unregistered sound records can still have
fixed ranges and are represented separately by the protocol codec.

`ExtractTagKeys` separately records tag key names and their registry identities,
including vanilla datapack tags without a Java constant, in `tag_keys.json`.
`tools/extract-data.sh` regenerates it. Typed keys have per-registry ordinals;
each loaded `TagSet` resolves them once and stores membership in bitsets. Custom
datapack tags resolve by identifier to a dense `TagId` before simulation reads.

`BlockState::liquid` and `ignited_by_lava` come from the final booleans
`BlockStateBase` copies directly from `Properties` at construction. They never
read a world or position. `is_solid` is vanilla's `initCache` legacySolid;
`replaceable` is already retained in the per-state properties. Vanilla 26.3
uses loaded `BLOCKS_MOTION_IN_HEIGHTMAP` tags for heightmap membership rather
than a `blocksMotion` state method.

`redstone_state_facts` stores `hasAnalogOutputSignal` (whose signature takes
only state), plus redstone conductivity for non-dynamic states. Every registered
26.3 conductor predicate is either the cache-backed collision-full query or
`Blocks::always`/`never`; dynamic states retain an explicit missing live query.

`ExtractBlockMethods` records the first class declaring each selected virtual
method in every registered block's real superclass hierarchy. Signatures include
parameter types; no behavior sampling determines ownership. `Block::overrides`
is false for the recorded vanilla base owner (`BlockBehaviour`, or `Block` for
placement, stepping and falling). This does not imply that every base method is
a constant: collision-shape defaults, for example, call other virtual methods.
The extractor additionally exercises the base survival and shape-update methods
for every applicable state with null level/tick/random arguments, proving those
two known constants require no level behavior or random draws.

Liquid-container and bucket interfaces use `SimpleWaterloggedBlock` as the
default owner. Reflection resolves inherited interface defaults after concrete
class overrides. A non-implementing block records `<not implemented>`, so it
never selects an interface default. Slab waterlogging overrides remain distinct
from that default even when the slab is currently a single half.

`block_items` records `Block#asItem` directly, including wall/standing aliases.
Reversing the item's single block field loses those aliases. The clone-stack
method's reflected base is `BlockBehaviour`; it creates one item and ignores
the include-data flag. Overriding block families still require their own port.

`ExtractData <output-dir> --item-crafting-only` regenerates `item_crafting.json`.
`Item#getCraftingRemainder` is final, returns the `craftingRemainingItem` field
copied from `Item.Properties` by the constructor, and takes no stack or level.
It is a per-item template, independent of the `USE_REMAINDER` data component.
The generated `CraftingRemainder` holds a typed item id/count and persistent
component patch JSON; the game resolves the patch through protocol codecs.
`BannerItem#getColor` is recorded alongside it for component-independent banner
crafting checks.

`fluid_random_ticking` records `FluidState#isRandomlyTicking`, which delegates
only to the fluid type's virtual `isRandomlyTicking`. It has no level, position
or state-property inputs. Source and flowing variants retain their own registry
entries; waterlogged states use the same fluid fact as ordinary water.

`ExtractEnvironmentAttributes <output-dir>` independently extracts all native
EnvironmentAttribute defaults through their real value codecs, attribute-type
registry IDs, allowed modifier names, flags and float sanitization bounds. These
are immutable registry facts: dimension, biome, clock and weather sampling does
not participate. Float limits are measured with sanitizeValue at both infinities;
unbounded ends remain null. The Rust build consumes this committed table without
Java. No environment value is sampled from a fabricated live level.

`ExtractBlockDropTables <output-json>` records the optional actual block loot
keys, including shared keys and absent drops. `ExtractBlockMethods` also records
exact `getDrops` and `spawnAfterBreak` ownership, so unported subclass overrides
cannot silently inherit the base drop behavior.
