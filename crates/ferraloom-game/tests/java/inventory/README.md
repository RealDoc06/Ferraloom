Run `python3 generate.py` for ordinary menu storage, slot predicates and layouts.
Run `python3 generate-crafting.py` for server-side crafting and recipe placement;
it also builds the shared fixture classes through `generate.py`.
Run `python3 generate-listeners.py` for nested real menu broadcasts: a listener
replaces or mutates the current slot, then edits the original lastSlots copy.
It records both listeners and the memoized copy delivered to the synchronizer.
It also transfers lastSlots between two menus sharing the player inventory,
then replaces either menu's entry and mutates the original aliased copy.
Fresh split/copy objects also move into a container, then remain independently
mutable when a later setItem replaces them.
Real Slot.safeInsert callbacks observe both count edits before setByPlayer,
then replace or mutate the source/target while their original objects stay held.

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
