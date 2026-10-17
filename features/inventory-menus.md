# Inventory and menus

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| inventory-menus.player.inventory-slots | Maintain hotbar, main inventory, equipment and offhand with item stack component matching | `Inventory; InventoryMenu` | | - | todo |
| inventory-menus.menu.lifecycle | Open, close and validate menus against container id, block access and player distance | `AbstractContainerMenu#stillValid; ServerPlayer#openMenu; ServerGamePacketListenerImpl#handleContainerClose` | | - | todo |
| inventory-menus.menu.state-id | Synchronize slots, carried stack and integer data with state ids and full resync on mismatch | `AbstractContainerMenu#broadcastChanges; ServerGamePacketListenerImpl#handleContainerClick` | | - | todo |
| inventory-menus.click.pickup | Apply left and right pickup, place, swap and outside drop behavior | `AbstractContainerMenu#doClick` | | - | todo |
| inventory-menus.click.quick-move | Shift-click items through each menu's transfer ranges and slot predicates | `AbstractContainerMenu#quickMoveStack` | | - | todo |
| inventory-menus.click.swap | Swap menu slots with hotbar and offhand while preserving overflow items | `AbstractContainerMenu#doClick` | | - | todo |
| inventory-menus.click.clone-throw | Apply creative clone and single-stack or full-stack throwing | `AbstractContainerMenu#doClick` | | - | todo |
| inventory-menus.click.quickcraft | Track drag-start, add-slot and drag-end phases with distribution and creative-fill rules | `AbstractContainerMenu#doClick; AbstractContainerMenu#getQuickCraftPlaceCount` | | - | todo |
| inventory-menus.click.pickup-all | Collect matching stacks with slot restrictions and traversal order | `AbstractContainerMenu#doClick` | | - | todo |
| inventory-menus.click.item-overrides | Apply bundle and other item click overrides before default slot handling | `AbstractContainerMenu#tryItemClickBehaviourOverride` | | - | todo |
| inventory-menus.close.carried-stack | Return carried items to inventory or drop them when a menu closes | `AbstractContainerMenu#removed; AbstractContainerMenu#dropOrPlaceInInventory` | | - | todo |
| inventory-menus.creative.validation | Validate creative stack components, slot bounds and drop spam limits | `ServerGamePacketListenerImpl#handleSetCreativeModeSlot` | | - | todo |
| inventory-menus.crafting.result | Recompute craft output, consume ingredients and return remainder stacks when taking results | `CraftingMenu; ResultSlot#onTake` | | - | todo |
| inventory-menus.anvil.cost | Compute repair, combine, enchantment compatibility, rename and prior-work costs | `AnvilMenu#createResult` | | - | todo |
| inventory-menus.enchanting.offers | Compute seeded enchanting offers with bookshelf and obstruction checks and charge XP and lapis | `EnchantmentMenu` | | - | todo |
| inventory-menus.grindstone.result | Remove eligible enchantments, repair durability and return experience | `GrindstoneMenu` | | - | todo |
| inventory-menus.smithing.result | Match template, base and addition recipes and preserve intended result components | `SmithingMenu` | | - | todo |
| inventory-menus.stonecutter.selection | Select unlocked stonecutting recipes and consume input for each result | `StonecutterMenu` | | - | todo |
| inventory-menus.cartography.result | Scale, lock and clone maps with valid input combinations | `CartographyTableMenu` | | - | todo |
| inventory-menus.loom.banner | Select banner patterns and special pattern items with layer limits | `LoomMenu` | | - | todo |
| inventory-menus.merchant.result | Match offers, apply demand prices, consume inputs and award merchant XP | `MerchantMenu; MerchantResultSlot; MerchantContainer` | | - | todo |
| inventory-menus.beacon.selection | Validate beacon payment and allowed primary and secondary effect tiers | `BeaconMenu` | | - | todo |
| inventory-menus.lectern.pages | Handle lectern page buttons, book pickup and permission checks | `LecternMenu` | | - | todo |
| inventory-menus.horse.nautilus-slots | Validate mount equipment, inventory and access restrictions | `HorseInventoryMenu; NautilusInventoryMenu` | | - | todo |
| inventory-menus.sided-automation | Apply face-specific slot availability and insertion or extraction predicates | `WorldlyContainer; HopperBlockEntity` | | - | todo |
| inventory-menus.menu.anvil | `minecraft:anvil` (menu); implementation `AnvilMenu` | `net.minecraft.world.inventory.MenuType#register; AnvilMenu` | | - | todo |
| inventory-menus.menu.beacon | `minecraft:beacon` (menu); implementation `BeaconMenu` | `net.minecraft.world.inventory.MenuType#register; BeaconMenu` | | - | todo |
| inventory-menus.menu.blast-furnace | `minecraft:blast_furnace` (menu); implementation `BlastFurnaceMenu` | `net.minecraft.world.inventory.MenuType#register; BlastFurnaceMenu` | | - | todo |
| inventory-menus.menu.brewing-stand | `minecraft:brewing_stand` (menu); implementation `BrewingStandMenu` | `net.minecraft.world.inventory.MenuType#register; BrewingStandMenu` | | - | todo |
| inventory-menus.menu.cartography-table | `minecraft:cartography_table` (menu); implementation `CartographyTableMenu` | `net.minecraft.world.inventory.MenuType#register; CartographyTableMenu` | | - | todo |
| inventory-menus.menu.crafter-3x3 | `minecraft:crafter_3x3` (menu); implementation `CrafterMenu` | `net.minecraft.world.inventory.MenuType#register; CrafterMenu` | | - | todo |
| inventory-menus.menu.crafting | `minecraft:crafting` (menu); implementation `CraftingMenu` | `net.minecraft.world.inventory.MenuType#register; CraftingMenu` | | - | todo |
| inventory-menus.menu.enchantment | `minecraft:enchantment` (menu); implementation `EnchantmentMenu` | `net.minecraft.world.inventory.MenuType#register; EnchantmentMenu` | | - | todo |
| inventory-menus.menu.furnace | `minecraft:furnace` (menu); implementation `FurnaceMenu` | `net.minecraft.world.inventory.MenuType#register; FurnaceMenu` | | - | todo |
| inventory-menus.menu.generic-3x3 | `minecraft:generic_3x3` (menu); implementation `DispenserMenu` | `net.minecraft.world.inventory.MenuType#register; DispenserMenu` | | - | todo |
| inventory-menus.menu.generic-9x1 | `minecraft:generic_9x1` (menu); implementation `ChestMenu` | `net.minecraft.world.inventory.MenuType#register; ChestMenu` | | - | todo |
| inventory-menus.menu.generic-9x2 | `minecraft:generic_9x2` (menu); implementation `ChestMenu` | `net.minecraft.world.inventory.MenuType#register; ChestMenu` | | - | todo |
| inventory-menus.menu.generic-9x3 | `minecraft:generic_9x3` (menu); implementation `ChestMenu` | `net.minecraft.world.inventory.MenuType#register; ChestMenu` | | - | todo |
| inventory-menus.menu.generic-9x4 | `minecraft:generic_9x4` (menu); implementation `ChestMenu` | `net.minecraft.world.inventory.MenuType#register; ChestMenu` | | - | todo |
| inventory-menus.menu.generic-9x5 | `minecraft:generic_9x5` (menu); implementation `ChestMenu` | `net.minecraft.world.inventory.MenuType#register; ChestMenu` | | - | todo |
| inventory-menus.menu.generic-9x6 | `minecraft:generic_9x6` (menu); implementation `ChestMenu` | `net.minecraft.world.inventory.MenuType#register; ChestMenu` | | - | todo |
| inventory-menus.menu.grindstone | `minecraft:grindstone` (menu); implementation `GrindstoneMenu` | `net.minecraft.world.inventory.MenuType#register; GrindstoneMenu` | | - | todo |
| inventory-menus.menu.hopper | `minecraft:hopper` (menu); implementation `HopperMenu` | `net.minecraft.world.inventory.MenuType#register; HopperMenu` | | - | todo |
| inventory-menus.menu.lectern | `minecraft:lectern` (menu); implementation `LecternMenu` | `net.minecraft.world.inventory.MenuType#register; LecternMenu` | | - | todo |
| inventory-menus.menu.loom | `minecraft:loom` (menu); implementation `LoomMenu` | `net.minecraft.world.inventory.MenuType#register; LoomMenu` | | - | todo |
| inventory-menus.menu.merchant | `minecraft:merchant` (menu); implementation `MerchantMenu` | `net.minecraft.world.inventory.MenuType#register; MerchantMenu` | | - | todo |
| inventory-menus.menu.shulker-box | `minecraft:shulker_box` (menu); implementation `ShulkerBoxMenu` | `net.minecraft.world.inventory.MenuType#register; ShulkerBoxMenu` | | - | todo |
| inventory-menus.menu.smithing | `minecraft:smithing` (menu); implementation `SmithingMenu` | `net.minecraft.world.inventory.MenuType#register; SmithingMenu` | | - | todo |
| inventory-menus.menu.smoker | `minecraft:smoker` (menu); implementation `SmokerMenu` | `net.minecraft.world.inventory.MenuType#register; SmokerMenu` | | - | todo |
| inventory-menus.menu.stonecutter | `minecraft:stonecutter` (menu); implementation `StonecutterMenu` | `net.minecraft.world.inventory.MenuType#register; StonecutterMenu` | | - | todo |
