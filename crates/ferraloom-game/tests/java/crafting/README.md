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
