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

`ExtractData <output-dir> --entities-only` regenerates only `entity_types.json`,
including `EntityType#trackDeltas`. Delta tracking is independent of tracking
range and update interval; game code must use the extracted boolean.
Each type also retains all four `EntityDimensions.attachments` lists. Extraction
reads the immutable native map directly, preserving unrotated double coordinates
and registered offsets independently of bounding-box dimensions.

`ExtractData <output-dir> --attributes-only` writes `attributes.json`: each
registered Attribute's default, optional RangedAttribute bounds and syncable
flag. These registry defaults remain separate from entity-type base overrides.

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

Mining appends `getDestroyProgress`, `destroy` and `playerDestroy`; `attack`
already has its original signature/index. Existing method and declaring-class
indices are preserved. Every registered 26.3 block inherits the base progress
algorithm; lifecycle and attack overrides still require their family ports.

Liquid-container and bucket interfaces use `SimpleWaterloggedBlock` as the
default owner. Reflection resolves inherited interface defaults after concrete
class overrides. A non-implementing block records `<not implemented>`, so it
never selects an interface default. Slab waterlogging overrides remain distinct
from that default even when the slab is currently a single half.

`block_items` records `Block#asItem` directly, including wall/standing aliases.
Reversing the item's single block field loses those aliases. The clone-stack
method's reflected base is `BlockBehaviour`; it creates one item and ignores
the include-data flag. Overriding block families still require their own port.

`fluid_random_ticking` records `FluidState#isRandomlyTicking`, which delegates
only to the fluid type's virtual `isRandomlyTicking`. It has no level, position
or state-property inputs. Source and flowing variants retain their own registry
entries; waterlogged states use the same fluid fact as ordinary water.

`ExtractData <output-dir> --item-crafting-only` regenerates `item_crafting.json`.
`Item#getCraftingRemainder` is final, returns the `craftingRemainingItem` field
copied from `Item.Properties` by the constructor, and takes no stack or level.
It is a per-item template, independent of the `USE_REMAINDER` data component.
The generated `CraftingRemainder` holds a typed item id/count and persistent
component patch JSON; the game resolves the patch through protocol codecs.
`BannerItem#getColor` is recorded alongside it for component-independent banner
crafting checks.

`ExtractData <output-dir> --block-render-shapes-only` writes the per-state
`block_render_shapes.json` table, exposed as `BlockStateData::render_shape`.
In 26.3 `BlockStateBase#getRenderShape` delegates to
`BlockBehaviour#getRenderShape(BlockState)` with no level, position or context.
The base returns `MODEL`; every override in the jar (air, barrier, liquids,
bubble column, light, structure void, end portal/gateway and moving piston)
returns constant `INVISIBLE`. This is independent of collision and block entities.

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

`ExtractFeatureRequirements <output-json>` captures `FeatureElement#isEnabled`
against `FeatureFlags.VANILLA_SET` for every block and item. The dedicated server
currently advertises precisely that set. The generated const queries are named
`is_enabled_in_vanilla` so a future experimental-feature runtime cannot mistake
them for checks against an arbitrary live feature set.
The same report reflects both item inventory-click virtuals independently.
Generated `overrides_stacked_on_other` / `overrides_other_stacked_on_me` facts
follow the actual Item subclass, including when stacks override components.

`ExtractData <output-dir> --item-names-only` writes immutable item description IDs
and potion effect-name suffixes to `item_names.json`. `Item#getDescriptionId` and
`Potion#name` return their final constructor fields, independent of stacks or
world context. Potion suffixes deliberately differ from registry keys for strong
and long variants. No state-fact bits are allocated.
`ExtractBlockEntityTypes <output-json>` records valid blocks, OP-only NBT flags
and whether each registered receiver implements `Nameable`. The last fact is
checked against the actual factory receiver for every valid block, including
aliases. It is independent of whether a receiver currently has a custom name;
`CopyNameFunction` reads nullable `Nameable#getCustomName` at evaluation time.

`ExtractBlockMethods` records `spawnDestroyParticles` independently from
`spawnDestroyByEntityParticles`: `Level#destroyBlock` calls the former, whose
base `Block` implementation redispatches the latter with a null source. The
only former-method override is `StrawBedBlock` (level event 2014);
`BaseFireBlock` overrides the latter with no particles. DestroyBlock's breaker
is loot/game-event context and is not excluded from level event 2001.
