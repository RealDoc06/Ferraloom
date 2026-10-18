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
