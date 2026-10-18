Run `bash crates/ferraloom-game/tests/java/crafting/run.sh` from the checkout to
regenerate `src/crafting/vanilla.json.gz`. The script uses the shared 26.3 jar,
JDK 25 and the existing `tree-harness` Mockito/Byte Buddy libraries. Override the
jar cache with `FERRALOOM_VANILLA_CACHE` and the JDK with `JAVA_HOME`.

The harness loads vanilla's real item tags, world registries and recipe registry,
then calls `RecipeManager`, recipe methods, display codecs and `ServerPlaceRecipe`
against real item stacks and inventories. Mockito supplies only saved-map data
through `Level#getMapData`; recipe matching and assembly remain vanilla code.
The compressed fixture contains test inputs and observed results, not recipe JSON
or decompiled source. Rust tests read the same recipes from the pinned server jar.

`bash crates/ferraloom-game/tests/java/crafting/run-primitives.sh` regenerates the
protocol component primitive fixtures using real DFU/Gson/NBT codecs. It covers
integer narrowing and NumberLimits, direct float rounding, all numeric NBT kinds,
and the shared compound-or-SNBT alternative used by custom and typed entity data.

`bash crates/ferraloom-game/tests/java/crafting/run-book.sh` captures
`ServerRecipeBook.Packed.CODEC` and `loadUntrusted` against required lists,
default settings, numeric booleans, empty numeric arrays and unknown recipe ids.

The primitive harness constructs Gson lazy numeric values directly for codec
limit probes; vanilla's strict resource reader separately rejects number tokens
of 1,024 characters. It also compares native NBT encoding and HashOps CRC32C,
including raw negative zero and NaN payloads. NbtOps canonicalizes signed zero;
HashOps preserves raw bits.

`bash crates/ferraloom-game/tests/java/crafting/run-java-floats.sh` captures boxed
Java float/double equality, ordering and hash codes in util's Rust vectors.
`bash crates/ferraloom-game/tests/java/crafting/run-component-floats.sh` captures
record and stack equality, record hash codes, and raw component encodings. Each
encoding gets a distinct RegistryInfoLookup identity so EncoderCache's NaN-value
collapsing does not hide the codec's raw bits; that cache behavior is separate.

`bash crates/ferraloom-game/tests/java/crafting/run-nbt-floats.sh` compares
DataOutputStream and Netty writes for scalar and nested float/double tags, then
reads both through DataInputStream and ByteBufInputStream. File writes canonicalize
NaNs; network writes and both readers retain NaN payloads.

`bash crates/ferraloom-game/tests/java/crafting/run-component-maps.sh` captures
map, patch, stack and template equality across component insertion orders, removal
orders, signed zeros and NaN payloads. It checks native map entry-sum hashes and
map/patch hash invariance, and records patch bytes in their original wire order.
