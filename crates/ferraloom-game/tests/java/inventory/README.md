Run `python3 generate.py` for ordinary menu storage, slot predicates and layouts.
Run `python3 generate-crafting.py` for server-side crafting and recipe placement;
it also builds the shared fixture classes through `generate.py`.

Both scripts use the pinned jar, JDK 25 and libraries in the main checkout's
shared cache. Crafting uses the Mockito jars already used by the recipe harness
under `.cache/vanilla/26.3/tree-harness/`.

`CraftingMenuVectors` loads the real vanilla recipes and invokes real
InventoryMenu/CraftingMenu clicks, ResultSlot consumption, ServerRecipeBook
unlocks, and AbstractCraftingMenu.handlePlacement. Its mocked ServerLevel and
ServerPlayer supply registry/recipe access and record stat calls, recipe-crafted
callbacks and outbound packets. Advancement listeners are empty. This exercises
server-only result calculation while avoiding a running world or sockets.

The regular `InventoryVectors` fixture uses a concrete non-server Level and
empty recipe property sets. Those vectors cover storage rules rather than
recipe computation. All generated expected values come from vanilla execution.
