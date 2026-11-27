# Plugin usage

What sixteen widely used Spigot plugins need of the bridge, read from their bytecode by
`crates/ferraloom-plugins/java/tools/Usage.java`. Real plugins decide what to build next, not the coverage
percentage. Regenerate after bridge changes:

```sh
python3 crates/ferraloom-plugins/java/build.py --usage <folder of plugin jars>   # writes .local/ferraloom-plugins/usage.md
```

then replace everything below "Generated" with it. The jars are never committed. The report below is
against the foundation (9d2a8f0fc) plus the shared registries (e710f0c48), before the five areas' work:
regenerate it on the integrated branch to see what is left.

**How a reference is classed.** Every `org.bukkit` method, field and event a plugin's classes (and the jars
nested in them) name. A method's status is what the call meets at run time, from the bridge's and the API's
classes that implement the type it is called on: *implemented* (every bridge class overrides it), *partial*
(some do), *stubbed* (the generated stub, or the API's "Not supported yet" body), *absent* (nothing
implements the type: `BossBar`, `Inventory`, `Chunk`), *missing* (not in the Spigot API: Paper or Folia API,
or removed). Calls into the API's own code (`Bukkit.getOfflinePlayer`, `ItemStack.getItemMeta`, a class's
static initializer) are followed, and what they reach counts as an implied use (Plugins minus Direct).
An event is *bridged* when Ferraloom fires it: its handler list is in `Bridge.EVENTS`, or the API's or the
bridge's code constructs it (`PluginEnableEvent` from `SimplePluginManager`). Static analysis: a plugin may
never reach a call it names (Paper-only branches, optional features), so treat counts as demand, not as
certain failures.

## Corpus

Newest Bukkit-family build of each on 2026-11-25 (Modrinth, Vault from GitHub releases); every license
allows download and use.

| Plugin | Build | License | Loads today |
|---|---|---|---|
| Vault | 1.7.3-b131 | LGPL-3.0 | yes |
| LuckPerms | 5.5.71 | MIT | yes |
| EssentialsX | 2.22.0 | GPL-3.0 | yes |
| WorldEdit | 7.4.6 | GPL-3.0 | no: NMS adapters (`adapter/impl/reobf/v1_21_11`) |
| WorldGuard | 7.0.21 | LGPL-3.0 | yes |
| PlaceholderAPI | 2.12.3 | GPL-3.0 | yes |
| CoreProtect CE | 24.1 | Artistic-2.0 | yes |
| Multiverse-Core | 5.8.2 | BSD-3-Clause | yes |
| ChestShop | 3.13-pre-1 | LGPL-2.1 | yes |
| HuskHomes (Paper build) | 4.11.1 | Apache-2.0 | yes; needs `libraries:` |
| TAB | 6.2.1 | Apache-2.0 | no: NMS adapters (`paper_1_20_5`) |
| GriefPrevention | 16.18.8 | GPL-3.0 | yes |
| ViaVersion | 5.12.3 | GPL-3.0 | yes |
| DiscordSRV | 1.30.5 | GPL-3.0 | yes |
| DecentHolograms | 2.10.2 | GPL-3.0 | no: NMS adapters (`nms/v1_13_R2`) |
| Chunky (Paper build) | 1.5.66 | GPL-3.0 | yes |

## Findings

- **Server internals are never on a main path.** No corpus plugin names `net.minecraft` or
  `org.bukkit.craftbukkit` from code its main class reaches directly; the three refused today (WorldEdit,
  TAB, DecentHolograms) carry them only in version-named adapters they pick by version string, and eight
  more probe them reflectively (string constants). Loading them with a warning (the relaxed rule) is what
  lets them run.
- **adventure-platform-bukkit**, shaded by ChestShop, DecentHolograms, DiscordSRV, EssentialsX, HuskHomes
  and PlaceholderAPI, accounts for one cluster across areas: titles, tab-list header and footer, sounds,
  boss bars, `sendPluginMessage`. Implementing that set serves six plugins at once.
- **Paper and Folia API** (`getGlobalRegionScheduler`, `teleportAsync`, `getChunkAtAsync`) is named by
  most modern plugins, behind a Paper check: not ours to provide.
- **`libraries:`** in `plugin.yml`: one plugin (HuskHomes: Jedis, three JDBC drivers, H2).
- **Implied uses** show the API's own dependencies: `ItemStack.getItemMeta` and friends need
  `Server.getItemFactory` (12 plugins), legacy `Material` paths need `UnsafeValues.fromLegacy`.

## Generated

### Per plugin

Misses: distinct stubbed, partial or absent methods it names itself, plus unbridged events it references; implied: more reached through the API's own code (`Bukkit.*`, `ItemStack`, static initializers).

| Plugin | Version | api-version | Bukkit methods named | Implemented | Misses | Implied misses | Not in Spigot API | Unbridged events | Internals (main / adapter / reflective) | Paper classes | libraries |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|---|
| ChestShop | 3.13-pre-1 (build 474) | 1.13 | 176 | 37 | 149 | 21 | 5 | 15 | 0 / 0 / 2 | 3 |  |
| Chunky | 1.5.66 | 1.21.8 | 61 | 23 | 23 | 2 | 17 | 2 | 0 / 0 / 0 | 15 |  |
| CoreProtect | 24.1 | 1.16 | 520 | 67 | 487 | 35 | 19 | 53 | 0 / 0 / 0 | 17 |  |
| DecentHolograms | 2.10.2 | 1.13 | 452 | 47 | 400 | 25 | 15 | 10 | 0 / 417 / 24 | 0 |  |
| DiscordSRV | 1.30.5 | 1.13 | 108 | 45 | 69 | 13 | 5 | 11 | 0 / 0 / 5 | 5 |  |
| Essentials | 2.22.0 | 1.13 | 594 | 91 | 452 | 37 | 93 | 42 | 0 / 0 / 5 | 20 |  |
| GriefPrevention | 16.18.8 | 26.3 | 221 | 58 | 222 | 14 | 0 | 59 | 0 / 0 / 0 | 7 |  |
| HuskHomes | 4.11.1 | 1.20 | 127 | 60 | 64 | 10 | 9 | 6 | 0 / 0 / 2 | 16 | redis.clients:jedis:6.0.0, com.mysql:mysql-connector-j:9.3.0, org.mariadb.jdbc:mariadb-java-client:3.5.3, org.xerial:sqlite-jdbc:3.53.1.0, com.h2database:h2:2.3.232 |
| LuckPerms | 5.5.71 | 1.13 | 57 | 40 | 20 | 2 | 4 | 7 | 0 / 0 / 1 | 4 |  |
| Multiverse-Core | 5.8.2 | 1.13 | 171 | 63 | 105 | 24 | 20 | 17 | 0 / 0 / 1 | 11 |  |
| PlaceholderAPI | 2.12.3 | 1.13 | 86 | 40 | 39 | 10 | 9 | 2 | 0 / 0 / 2 | 10 |  |
| TAB | 6.2.1 | 1.13 | 35 | 12 | 21 | 1 | 3 | 1 | 0 / 174 / 0 | 0 |  |
| Vault | 1.7.3-b131 | 1.13 | 28 | 26 | 2 | 0 | 0 | 0 | 0 / 0 / 0 | 0 |  |
| ViaVersion | 5.12.3 | 1.13 | 90 | 35 | 61 | 15 | 7 | 13 | 0 / 0 / 3 | 6 |  |
| WorldEdit | 7.4.6+7648-00664b0 | 1.21.4 | 143 | 54 | 73 | 22 | 23 | 7 | 0 / 830 / 2 | 15 |  |
| WorldGuard | 7.0.21+2424-da30a99 | 26.3 | 278 | 59 | 285 | 33 | 30 | 96 | 0 / 0 / 0 | 20 |  |

### Players

Methods, by plugins naming them (Direct), then by plugins reaching them through the API's own code (Plugins counts both):

| Method | Status | Plugins | Direct | Which |
|---|---|---:|---:|---|
| `Player.getGameMode()` | stubbed | 10 | 10 | ChestShop, CoreProtect, Essentials, GriefPrevention, LuckPerms, Multiverse-Core, TAB, ViaVersion, WorldEdit, WorldGuard |
| `OfflinePlayer.getPlayer()` | stubbed | 7 | 7 | DiscordSRV, Essentials, GriefPrevention, HuskHomes, PlaceholderAPI, WorldEdit, WorldGuard |
| `Player.isSneaking()` | stubbed | 7 | 7 | ChestShop, CoreProtect, DecentHolograms, Essentials, GriefPrevention, ViaVersion, WorldGuard |
| `Player.sendPluginMessage(Plugin, String, byte[])` | stubbed | 7 | 7 | ChestShop, CoreProtect, DecentHolograms, HuskHomes, LuckPerms, ViaVersion, WorldEdit |
| `Player.sendTitle(String, String, int, int, int)` | stubbed | 7 | 7 | ChestShop, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI, WorldGuard |
| `Player.setPlayerListHeaderFooter(String, String)` | stubbed | 7 | 7 | ChestShop, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI, TAB |
| `Player.playSound(Location, String, SoundCategory, float, float)` | stubbed | 6 | 6 | ChestShop, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Player.playSound(Location, String, float, float)` | stubbed | 6 | 6 | ChestShop, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Player.resetTitle()` | stubbed | 6 | 6 | ChestShop, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Player.stopSound(String)` | stubbed | 6 | 6 | ChestShop, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `OfflinePlayer.hasPlayedBefore()` | stubbed | 5 | 5 | DiscordSRV, Essentials, GriefPrevention, Multiverse-Core, PlaceholderAPI |
| `Player.getBedSpawnLocation()` | stubbed | 5 | 5 | Essentials, HuskHomes, Multiverse-Core, WorldEdit, WorldGuard |
| `Player.getHealth()` | stubbed | 5 | 5 | Essentials, GriefPrevention, HuskHomes, TAB, WorldGuard |
| `Player.getLocale()` | stubbed | 5 | 5 | ChestShop, Essentials, LuckPerms, Multiverse-Core, WorldEdit |
| `Player.getMetadata(String)` | stubbed | 5 | 5 | DiscordSRV, Essentials, GriefPrevention, HuskHomes, TAB |
| `Player.kickPlayer(String)` | stubbed | 5 | 5 | DiscordSRV, Essentials, GriefPrevention, ViaVersion, WorldGuard |
| `Player.setPlayerListFooter(String)` | stubbed | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Player.setPlayerListHeader(String)` | stubbed | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Player.stopSound(String, SoundCategory)` | stubbed | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Player.getEyeLocation()` | stubbed | 4 | 4 | DecentHolograms, Essentials, GriefPrevention, ViaVersion |
| `Player.getFoodLevel()` | stubbed | 4 | 4 | CoreProtect, Essentials, Multiverse-Core, WorldGuard |
| `Player.hasMetadata(String)` | stubbed | 4 | 4 | DiscordSRV, Essentials, GriefPrevention, HuskHomes |
| `Player.canSee(Player)` | stubbed | 4 | 3 | Essentials, GriefPrevention, Multiverse-Core, WorldEdit |
| `Player.getAllowFlight()` | stubbed | 3 | 3 | Essentials, Multiverse-Core, WorldEdit |
| `Player.hasPlayedBefore()` | stubbed | 3 | 3 | DiscordSRV, GriefPrevention, Multiverse-Core |
| `Player.hasPotionEffect(PotionEffectType)` | stubbed | 3 | 3 | DecentHolograms, TAB, ViaVersion |
| `Player.isDead()` | stubbed | 3 | 3 | DiscordSRV, Essentials, GriefPrevention |
| `Player.isInsideVehicle()` | stubbed | 3 | 3 | ChestShop, DecentHolograms, GriefPrevention |
| `Player.playSound(Location, Sound, float, float)` | stubbed | 3 | 3 | DecentHolograms, DiscordSRV, Essentials |
| `Player.sendBlockChange(Location, BlockData)` | stubbed | 3 | 3 | CoreProtect, GriefPrevention, WorldEdit |
| `Player.setFallDistance(float)` | stubbed | 3 | 3 | Essentials, HuskHomes, WorldGuard |
| `Player.setFlying(boolean)` | stubbed | 3 | 3 | Essentials, Multiverse-Core, WorldEdit |
| `Player.setGameMode(GameMode)` | stubbed | 3 | 3 | Essentials, Multiverse-Core, WorldEdit |
| `Player.setHealth(double)` | stubbed | 3 | 3 | Essentials, GriefPrevention, WorldGuard |
| `OfflinePlayer.getLastPlayed()` | stubbed | 2 | 2 | Essentials, GriefPrevention |
| `OfflinePlayer.isBanned()` | stubbed | 2 | 2 | Essentials, GriefPrevention |
| `OfflinePlayer.setOp(boolean)` | stubbed | 2 | 2 | Essentials, LuckPerms |
| `Player.chat(String)` | stubbed | 2 | 2 | DecentHolograms, Essentials |
| `Player.eject()` | stubbed | 2 | 2 | Essentials, HuskHomes |
| `Player.getAddress()` | stubbed | 2 | 2 | DiscordSRV, Essentials |
| `Player.getFireTicks()` | stubbed | 2 | 2 | ViaVersion, WorldGuard |
| `Player.getMaxHealth()` | stubbed | 2 | 2 | Essentials, WorldGuard |
| `Player.getMaximumAir()` | stubbed | 2 | 2 | Essentials, WorldGuard |
| `Player.getPlayerTimeOffset()` | stubbed | 2 | 2 | Essentials, WorldGuard |
| `Player.getPlayerWeather()` | stubbed | 2 | 2 | Essentials, WorldGuard |
| `Player.getSaturation()` | stubbed | 2 | 2 | Essentials, WorldGuard |
| `Player.getStatistic(Statistic)` | stubbed | 2 | 2 | Essentials, TAB |
| `Player.getTargetBlock(Set, int)` | stubbed | 2 | 2 | Essentials, Multiverse-Core |
| `Player.getTargetBlockExact(int)` | stubbed | 2 | 2 | ChestShop, Essentials |
| `Player.getVehicle()` | stubbed | 2 | 2 | Chunky, WorldGuard |
| `Player.hidePlayer(Player)` | stubbed | 2 | 2 | Essentials, TAB |
| `Player.isFlying()` | stubbed | 2 | 2 | Essentials, Multiverse-Core |
| `Player.isPlayerTimeRelative()` | stubbed | 2 | 2 | Essentials, WorldGuard |
| `Player.isSwimming()` | stubbed | 2 | 2 | ViaVersion, WorldGuard |
| `Player.resetPlayerTime()` | stubbed | 2 | 2 | Essentials, WorldGuard |
| `Player.resetPlayerWeather()` | stubbed | 2 | 2 | Essentials, WorldGuard |
| `Player.setAllowFlight(boolean)` | stubbed | 2 | 2 | Essentials, Multiverse-Core |
| `Player.setCompassTarget(Location)` | stubbed | 2 | 2 | Essentials, WorldGuard |
| `Player.setExhaustion(float)` | stubbed | 2 | 2 | Essentials, WorldGuard |
| `Player.setFireTicks(int)` | stubbed | 2 | 2 | Essentials, WorldGuard |

77 more, each needed by fewer plugins.

Unbridged events, by plugins referencing them:

| Event | Plugins | Which |
|---|---:|---|
| `PlayerTeleportEvent` | 10 | ChestShop, Chunky, DecentHolograms, DiscordSRV, Essentials, GriefPrevention, HuskHomes, Multiverse-Core, WorldEdit, WorldGuard |
| `PlayerInteractEvent` | 9 | ChestShop, CoreProtect, Essentials, GriefPrevention, HuskHomes, Multiverse-Core, ViaVersion, WorldEdit, WorldGuard |
| `PlayerCommandPreprocessEvent` | 7 | CoreProtect, DiscordSRV, Essentials, GriefPrevention, LuckPerms, WorldEdit, WorldGuard |
| `PlayerMoveEvent` | 7 | ChestShop, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI, WorldGuard |
| `PlayerRespawnEvent` | 7 | DecentHolograms, Essentials, GriefPrevention, HuskHomes, Multiverse-Core, ViaVersion, WorldGuard |
| `PlayerChangedWorldEvent` | 6 | DecentHolograms, Essentials, LuckPerms, Multiverse-Core, TAB, ViaVersion |
| `PlayerInteractEntityEvent` | 5 | CoreProtect, GriefPrevention, GriefPrevention, WorldGuard, WorldGuard |
| `PlayerLoginEvent` | 5 | DiscordSRV, Essentials, GriefPrevention, LuckPerms, WorldGuard |
| `PlayerBucketEmptyEvent` | 4 | CoreProtect, Essentials, GriefPrevention, WorldGuard |
| `PlayerGameModeChangeEvent` | 4 | Essentials, LuckPerms, WorldEdit, WorldGuard |
| `PlayerPickupItemEvent` | 4 | CoreProtect, Essentials, Essentials, WorldGuard |
| `AsyncPlayerPreLoginEvent` | 3 | DiscordSRV, Essentials, LuckPerms |
| `PlayerBucketFillEvent` | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `PlayerDropItemEvent` | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `PlayerFishEvent` | 3 | Essentials, GriefPrevention, WorldGuard |
| `PlayerItemHeldEvent` | 3 | GriefPrevention, ViaVersion, WorldGuard |
| `PlayerTakeLecternBookEvent` | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `PlayerAdvancementDoneEvent` | 2 | DiscordSRV, Essentials |
| `PlayerEggThrowEvent` | 2 | Essentials, GriefPrevention |
| `PlayerItemBreakEvent` | 2 | CoreProtect, ViaVersion |
| `PlayerKickEvent` | 2 | DiscordSRV, GriefPrevention |
| `PlayerPortalEvent` | 2 | GriefPrevention, Multiverse-Core |
| `PlayerArmorStandManipulateEvent` | 1 | CoreProtect |
| `PlayerBedEnterEvent` | 1 | WorldGuard |
| `PlayerChatEvent` | 1 | DiscordSRV |
| `PlayerCommandSendEvent` | 1 | Essentials |
| `PlayerItemConsumeEvent` | 1 | WorldGuard |
| `PlayerLocaleChangeEvent` | 1 | Multiverse-Core |
| `PlayerShearEntityEvent` | 1 | WorldGuard |
| `PlayerSignOpenEvent` | 1 | GriefPrevention |
| `PlayerToggleSneakEvent` | 1 | ViaVersion |

### Blocks

Methods, by plugins naming them (Direct), then by plugins reaching them through the API's own code (Plugins counts both):

| Method | Status | Plugins | Direct | Which |
|---|---|---:|---:|---|
| `Block.getBlockData()` | stubbed | 10 | 10 | ChestShop, CoreProtect, DecentHolograms, Essentials, GriefPrevention, HuskHomes, Multiverse-Core, ViaVersion, WorldEdit, WorldGuard |
| `Block.getState()` | stubbed | 9 | 9 | ChestShop, CoreProtect, DecentHolograms, Essentials, GriefPrevention, Multiverse-Core, ViaVersion, WorldEdit, WorldGuard |
| `Block.getRelative(BlockFace)` | stubbed | 8 | 8 | ChestShop, Chunky, CoreProtect, Essentials, GriefPrevention, Multiverse-Core, ViaVersion, WorldGuard |
| `Block.setType(Material)` | stubbed | 6 | 6 | ChestShop, CoreProtect, DecentHolograms, Essentials, GriefPrevention, WorldGuard |
| `Biome.getKey()` | absent | 4 | 4 | Essentials, GriefPrevention, Multiverse-Core, WorldEdit |
| `Block.breakNaturally()` | stubbed | 4 | 4 | ChestShop, DecentHolograms, WorldEdit, WorldGuard |
| `Block.getRelative(int, int, int)` | stubbed | 4 | 4 | CoreProtect, DecentHolograms, Multiverse-Core, WorldGuard |
| `BlockData.getMaterial()` | absent | 4 | 4 | CoreProtect, DecentHolograms, GriefPrevention, WorldGuard |
| `BlockState.getBlockData()` | absent | 4 | 4 | DecentHolograms, Essentials, WorldEdit, WorldGuard |
| `BlockState.getType()` | absent | 4 | 4 | DecentHolograms, GriefPrevention, Multiverse-Core, WorldGuard |
| `Directional.getFacing()` | absent | 4 | 4 | ChestShop, CoreProtect, DecentHolograms, Essentials |
| `Sign.getBlock()` | absent | 4 | 4 | ChestShop, Essentials, GriefPrevention, WorldGuard |
| `Sign.getLine(int)` | absent | 4 | 4 | ChestShop, CoreProtect, Essentials, WorldGuard |
| `Sign.getLocation()` | absent | 4 | 4 | ChestShop, CoreProtect, Essentials, GriefPrevention |
| `Tag.getValues()` | absent | 4 | 4 | CoreProtect, GriefPrevention, WorldEdit, WorldGuard |
| `Tag.isTagged(Keyed)` | absent | 4 | 4 | CoreProtect, Essentials, GriefPrevention, WorldGuard |
| `Banner.addPattern(Pattern)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Banner.getBaseColor()` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Banner.setBaseColor(DyeColor)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Block.getBiome()` | stubbed | 3 | 3 | DecentHolograms, Essentials, GriefPrevention |
| `Block.getChunk()` | stubbed | 3 | 3 | CoreProtect, DecentHolograms, WorldGuard |
| `Block.getData()` | stubbed | 3 | 3 | DecentHolograms, GriefPrevention, ViaVersion |
| `Block.setBlockData(BlockData)` | stubbed | 3 | 3 | CoreProtect, DecentHolograms, GriefPrevention |
| `Block.setBlockData(BlockData, boolean)` | stubbed | 3 | 3 | CoreProtect, DecentHolograms, WorldEdit |
| `Block.setType(Material, boolean)` | stubbed | 3 | 3 | CoreProtect, DecentHolograms, GriefPrevention |
| `BlockState.getLocation()` | absent | 3 | 3 | GriefPrevention, WorldEdit, WorldGuard |
| `BlockState.getY()` | absent | 3 | 3 | DecentHolograms, GriefPrevention, WorldGuard |
| `Chest.getType()` | absent | 3 | 3 | ChestShop, CoreProtect, WorldGuard |
| `CreatureSpawner.getSpawnedType()` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `CreatureSpawner.setSpawnedType(EntityType)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Dispenser.getFacing()` | absent | 3 | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `Levelled.getLevel()` | absent | 3 | 3 | CoreProtect, DecentHolograms, GriefPrevention |
| `Levelled.setLevel(int)` | absent | 3 | 3 | CoreProtect, DecentHolograms, GriefPrevention |
| `Lightable.setLit(boolean)` | absent | 3 | 3 | CoreProtect, DecentHolograms, GriefPrevention |
| `Sign.getLines()` | absent | 3 | 3 | ChestShop, CoreProtect, Essentials |
| `Sign.setLine(int, String)` | absent | 3 | 3 | ChestShop, CoreProtect, Essentials |
| `Banner.getPatterns()` | absent | 2 | 2 | CoreProtect, Essentials |
| `Banner.update()` | absent | 2 | 2 | CoreProtect, Essentials |
| `Bed.getFacing()` | absent | 2 | 2 | CoreProtect, WorldGuard |
| `Bed.getPart()` | absent | 2 | 2 | CoreProtect, WorldGuard |
| `Block.getPistonMoveReaction()` | stubbed | 2 | 2 | GriefPrevention, WorldGuard |
| `BlockData.clone()` | absent | 2 | 2 | CoreProtect, WorldEdit |
| `BlockData.getAsString()` | absent | 2 | 2 | CoreProtect, WorldEdit |
| `BlockState.getBlock()` | absent | 2 | 2 | ChestShop, WorldGuard |
| `BlockState.getData()` | absent | 2 | 2 | DecentHolograms, Essentials |
| `BlockState.getWorld()` | absent | 2 | 2 | DecentHolograms, WorldGuard |
| `BlockState.getX()` | absent | 2 | 2 | DecentHolograms, WorldGuard |
| `BlockState.getZ()` | absent | 2 | 2 | DecentHolograms, WorldGuard |
| `BlockState.isPlaced()` | absent | 2 | 2 | DecentHolograms, WorldGuard |
| `BlockState.update(boolean, boolean)` | absent | 2 | 2 | DecentHolograms, ViaVersion |
| `Cake.getBites()` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Chest.getBlock()` | absent | 2 | 2 | ChestShop, WorldGuard |
| `Chest.getBlockInventory()` | absent | 2 | 2 | CoreProtect, WorldEdit |
| `Chest.getFacing()` | absent | 2 | 2 | ChestShop, WorldGuard |
| `Chest.setType(Chest.Type)` | absent | 2 | 2 | CoreProtect, GriefPrevention |
| `CreatureSpawner.update()` | absent | 2 | 2 | CoreProtect, Essentials |
| `Directional.setFacing(BlockFace)` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Hopper.getLocation()` | absent | 2 | 2 | CoreProtect, WorldGuard |
| `Lectern.getLocation()` | absent | 2 | 2 | CoreProtect, GriefPrevention |
| `Lightable.isLit()` | absent | 2 | 2 | CoreProtect, DecentHolograms |

137 more, each needed by fewer plugins.

Unbridged events, by plugins referencing them:

| Event | Plugins | Which |
|---|---:|---|
| `BlockPlaceEvent` | 8 | ChestShop, CoreProtect, Essentials, GriefPrevention, GriefPrevention, ViaVersion, WorldGuard, WorldGuard |
| `BlockFormEvent` | 6 | CoreProtect, CoreProtect, GriefPrevention, GriefPrevention, WorldGuard, WorldGuard |
| `BlockBurnEvent` | 5 | ChestShop, CoreProtect, Essentials, GriefPrevention, WorldGuard |
| `BlockPistonExtendEvent` | 5 | ChestShop, CoreProtect, Essentials, GriefPrevention, WorldGuard |
| `BlockPistonRetractEvent` | 5 | ChestShop, CoreProtect, Essentials, GriefPrevention, WorldGuard |
| `SignChangeEvent` | 5 | ChestShop, CoreProtect, Essentials, GriefPrevention, WorldGuard |
| `BlockDispenseEvent` | 4 | CoreProtect, GriefPrevention, WorldGuard, WorldGuard |
| `BlockIgniteEvent` | 4 | CoreProtect, Essentials, GriefPrevention, WorldGuard |
| `BlockDamageEvent` | 3 | Essentials, ViaVersion, WorldGuard |
| `BlockExplodeEvent` | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `BlockFertilizeEvent` | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `BlockFromToEvent` | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `BlockPhysicsEvent` | 3 | ChestShop, WorldEdit, WorldGuard |
| `BlockSpreadEvent` | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `BlockFadeEvent` | 2 | CoreProtect, WorldGuard |
| `LeavesDecayEvent` | 2 | CoreProtect, WorldGuard |
| `BlockGrowEvent` | 1 | WorldGuard |
| `BlockReceiveGameEvent` | 1 | Essentials |
| `BlockRedstoneEvent` | 1 | WorldGuard |
| `CampfireStartEvent` | 1 | CoreProtect |
| `CauldronLevelChangeEvent` | 1 | WorldGuard |
| `MoistureChangeEvent` | 1 | WorldGuard |
| `TNTPrimeEvent` | 1 | CoreProtect |

### Worlds

Methods, by plugins naming them (Direct), then by plugins reaching them through the API's own code (Plugins counts both):

| Method | Status | Plugins | Direct | Which |
|---|---|---:|---:|---|
| `World.getSpawnLocation()` | stubbed | 9 | 9 | ChestShop, Chunky, DiscordSRV, Essentials, HuskHomes, Multiverse-Core, PlaceholderAPI, WorldEdit, WorldGuard |
| `World.getChunkAt(int, int)` | stubbed | 7 | 7 | CoreProtect, Essentials, GriefPrevention, Multiverse-Core, ViaVersion, WorldEdit, WorldGuard |
| `Chunk.getX()` | absent | 6 | 6 | CoreProtect, DecentHolograms, GriefPrevention, Multiverse-Core, ViaVersion, WorldGuard |
| `Chunk.getZ()` | absent | 6 | 6 | CoreProtect, DecentHolograms, GriefPrevention, Multiverse-Core, ViaVersion, WorldGuard |
| `World.getWorldBorder()` | stubbed | 6 | 6 | Chunky, DecentHolograms, Essentials, GriefPrevention, HuskHomes, Multiverse-Core |
| `Chunk.getEntities()` | absent | 5 | 5 | CoreProtect, Essentials, GriefPrevention, Multiverse-Core, WorldGuard |
| `World.getEntities()` | stubbed | 5 | 5 | Essentials, GriefPrevention, Multiverse-Core, WorldEdit, WorldGuard |
| `World.isChunkGenerated(int, int)` | stubbed | 5 | 5 | Chunky, Essentials, Multiverse-Core, WorldEdit, WorldGuard |
| `World.isChunkLoaded(int, int)` | stubbed | 5 | 5 | ChestShop, CoreProtect, GriefPrevention, ViaVersion, WorldEdit |
| `Chunk.getWorld()` | absent | 4 | 4 | DecentHolograms, Essentials, GriefPrevention, ViaVersion |
| `World.dropItemNaturally(Location, ItemStack)` | stubbed | 4 | 4 | DecentHolograms, Essentials, GriefPrevention, WorldEdit |
| `World.getChunkAt(Location)` | stubbed | 4 | 4 | CoreProtect, GriefPrevention, HuskHomes, Multiverse-Core |
| `World.getSeaLevel()` | stubbed | 4 | 4 | Chunky, Essentials, GriefPrevention, WorldGuard |
| `World.getWorldFolder()` | stubbed | 4 | 4 | Chunky, DiscordSRV, Multiverse-Core, WorldEdit |
| `World.setStorm(boolean)` | stubbed | 4 | 4 | Essentials, Multiverse-Core, WorldEdit, WorldGuard |
| `World.setThundering(boolean)` | stubbed | 4 | 4 | Essentials, Multiverse-Core, WorldEdit, WorldGuard |
| `WorldBorder.getCenter()` | absent | 4 | 4 | Chunky, DecentHolograms, Essentials, Multiverse-Core |
| `WorldBorder.getSize()` | absent | 4 | 4 | Chunky, DecentHolograms, Essentials, Multiverse-Core |
| `WorldBorder.isInside(Location)` | absent | 4 | 4 | DecentHolograms, GriefPrevention, HuskHomes, Multiverse-Core |
| `World.getGameRuleValue(GameRule)` | stubbed | 5 | 3 | DecentHolograms, DiscordSRV, Essentials, Multiverse-Core, WorldEdit |
| `Chunk.getBlock(int, int, int)` | absent | 3 | 3 | DecentHolograms, GriefPrevention, ViaVersion |
| `Chunk.getTileEntities()` | absent | 3 | 3 | Essentials, GriefPrevention, WorldGuard |
| `World.spawn(Location, Class)` | stubbed | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Chunk.isLoaded()` | absent | 2 | 2 | DecentHolograms, GriefPrevention |
| `Chunk.load(boolean)` | absent | 2 | 2 | DecentHolograms, GriefPrevention |
| `ChunkSnapshot.getBlockType(int, int, int)` | absent | 2 | 2 | GriefPrevention, HuskHomes |
| `Server.getDefaultGameMode()` | stubbed | 2 | 2 | GriefPrevention, WorldGuard |
| `World.createExplosion(Location, float)` | stubbed | 2 | 2 | Essentials, GriefPrevention |
| `World.dropItem(Location, ItemStack)` | stubbed | 2 | 2 | GriefPrevention, WorldGuard |
| `World.getDifficulty()` | stubbed | 2 | 2 | Multiverse-Core, WorldGuard |
| `World.getGameRules()` | stubbed | 2 | 2 | Multiverse-Core, WorldGuard |
| `World.getGenerator()` | stubbed | 2 | 2 | WorldEdit, WorldGuard |
| `World.getHighestBlockYAt(int, int)` | stubbed | 2 | 2 | Chunky, Essentials |
| `World.getKeepSpawnInMemory()` | stubbed | 2 | 2 | Multiverse-Core, WorldGuard |
| `World.getLoadedChunks()` | stubbed | 2 | 2 | Essentials, WorldGuard |
| `World.getLogicalHeight()` | stubbed | 2 | 2 | Chunky, Essentials |
| `World.getPVP()` | stubbed | 2 | 2 | GriefPrevention, WorldGuard |
| `World.getWeatherDuration()` | stubbed | 2 | 2 | WorldEdit, WorldGuard |
| `World.getWorldType()` | stubbed | 2 | 2 | Multiverse-Core, WorldGuard |
| `World.hasStorm()` | stubbed | 2 | 2 | Essentials, WorldEdit |
| `World.playEffect(Location, Effect, int)` | stubbed | 2 | 2 | Chunky, WorldEdit |
| `World.playSound(Location, Sound, float, float)` | stubbed | 2 | 2 | Chunky, DecentHolograms |
| `World.setGameRule(GameRule, Object)` | stubbed | 2 | 2 | DecentHolograms, Multiverse-Core |
| `World.setSpawnLocation(Location)` | stubbed | 2 | 2 | HuskHomes, Multiverse-Core |
| `World.setThunderDuration(int)` | stubbed | 2 | 2 | Essentials, WorldEdit |
| `World.setWeatherDuration(int)` | stubbed | 2 | 2 | Essentials, WorldEdit |
| `World.spawnEntity(Location, EntityType)` | stubbed | 2 | 2 | CoreProtect, Essentials |
| `WorldBorder.getDamageAmount()` | absent | 2 | 2 | DecentHolograms, Multiverse-Core |
| `WorldBorder.getDamageBuffer()` | absent | 2 | 2 | DecentHolograms, Multiverse-Core |
| `WorldBorder.getWarningDistance()` | absent | 2 | 2 | DecentHolograms, Multiverse-Core |
| `WorldBorder.getWarningTime()` | absent | 2 | 2 | DecentHolograms, Multiverse-Core |
| `WorldBorder.setCenter(double, double)` | absent | 2 | 2 | DecentHolograms, Multiverse-Core |
| `WorldBorder.setDamageAmount(double)` | absent | 2 | 2 | DecentHolograms, Multiverse-Core |
| `WorldBorder.setDamageBuffer(double)` | absent | 2 | 2 | DecentHolograms, Multiverse-Core |
| `WorldBorder.setSize(double)` | absent | 2 | 2 | DecentHolograms, Multiverse-Core |
| `WorldBorder.setWarningDistance(int)` | absent | 2 | 2 | DecentHolograms, Multiverse-Core |
| `WorldBorder.setWarningTime(int)` | absent | 2 | 2 | DecentHolograms, Multiverse-Core |
| `Chunk.getChunkSnapshot()` | absent | 1 | 1 | HuskHomes |
| `Chunk.getChunkSnapshot(boolean, boolean, boolean)` | absent | 1 | 1 | GriefPrevention |
| `Chunk.getPersistentDataContainer()` | absent | 1 | 1 | DecentHolograms |

60 more, each needed by fewer plugins.

Unbridged events, by plugins referencing them:

| Event | Plugins | Which |
|---|---:|---|
| `PortalCreateEvent` | 4 | CoreProtect, GriefPrevention, Multiverse-Core, WorldGuard |
| `WorldLoadEvent` | 4 | DecentHolograms, Essentials, Multiverse-Core, WorldGuard |
| `StructureGrowEvent` | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `WorldUnloadEvent` | 3 | DecentHolograms, Multiverse-Core, WorldGuard |
| `ChunkLoadEvent` | 2 | Multiverse-Core, WorldGuard |
| `ThunderChangeEvent` | 2 | Multiverse-Core, WorldGuard |
| `WeatherChangeEvent` | 2 | Multiverse-Core, WorldGuard |
| `WorldInitEvent` | 2 | Chunky, WorldEdit |
| `ChunkPopulateEvent` | 1 | CoreProtect |
| `ChunkUnloadEvent` | 1 | WorldGuard |
| `GenericGameEvent` | 1 | CoreProtect |
| `LightningStrikeEvent` | 1 | WorldGuard |

### Entities

Methods, by plugins naming them (Direct), then by plugins reaching them through the API's own code (Plugins counts both):

| Method | Status | Plugins | Direct | Which |
|---|---|---:|---:|---|
| `Entity.setCustomName(String)` | stubbed | 7 | 7 | ChestShop, CoreProtect, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Entity.remove()` | stubbed | 6 | 6 | CoreProtect, Essentials, GriefPrevention, Multiverse-Core, WorldEdit, WorldGuard |
| `Damageable.getMaxHealth()` | stubbed | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Damageable.setHealth(double)` | stubbed | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Tameable.isTamed()` | absent | 5 | 5 | CoreProtect, Essentials, GriefPrevention, WorldEdit, WorldGuard |
| `Projectile.getShooter()` | absent | 4 | 4 | CoreProtect, Essentials, GriefPrevention, WorldGuard |
| `Tameable.getOwner()` | absent | 4 | 4 | CoreProtect, Essentials, GriefPrevention, WorldGuard |
| `Tameable.setTamed(boolean)` | absent | 4 | 4 | CoreProtect, DecentHolograms, Essentials, GriefPrevention |
| `Ageable.setAdult()` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Ageable.setBaby()` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `AttributeInstance.setBaseValue(double)` | absent | 3 | 3 | CoreProtect, DecentHolograms, TAB |
| `Axolotl.setVariant(Axolotl.Variant)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Cat.setCatType(Cat.Type)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `ChestedHorse.setCarryingChest(boolean)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Creeper.setPowered(boolean)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Entity.getPassengers()` | stubbed | 3 | 3 | Chunky, GriefPrevention, Multiverse-Core |
| `Entity.hasMetadata(String)` | stubbed | 3 | 3 | Essentials, GriefPrevention, WorldGuard |
| `Entity.setVelocity(Vector)` | stubbed | 3 | 3 | Essentials, Multiverse-Core, WorldGuard |
| `Fox.setFoxType(Fox.Type)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Frog.setVariant(Frog.Variant)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Goat.setScreaming(boolean)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Hanging.getLocation()` | absent | 3 | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `Item.getItemStack()` | absent | 3 | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `LivingEntity.getEquipment()` | stubbed | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Llama.setColor(Llama.Color)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `MushroomCow.setVariant(MushroomCow.Variant)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Panda.setHiddenGene(Panda.Gene)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Panda.setMainGene(Panda.Gene)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Parrot.setVariant(Parrot.Variant)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Phantom.setSize(int)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Tameable.setOwner(AnimalTamer)` | absent | 3 | 3 | CoreProtect, Essentials, GriefPrevention |
| `ThrownPotion.getShooter()` | absent | 3 | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `TropicalFish.setBodyColor(DyeColor)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `TropicalFish.setPattern(TropicalFish.Pattern)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `TropicalFish.setPatternColor(DyeColor)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Vehicle.getLocation()` | absent | 3 | 3 | GriefPrevention, Multiverse-Core, WorldGuard |
| `Server.selectEntities(CommandSender, String)` | stubbed | 3 | 2 | DiscordSRV, LuckPerms, Multiverse-Core |
| `AbstractHorse.setDomestication(int)` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `AbstractHorse.setJumpStrength(double)` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `AbstractHorse.setMaxDomestication(int)` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Ageable.setAge(int)` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Ageable.setAgeLock(boolean)` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Ageable.setBreed(boolean)` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Arrow.getBasePotionType()` | absent | 2 | 2 | CoreProtect, WorldGuard |
| `Arrow.getCustomEffects()` | absent | 2 | 2 | CoreProtect, WorldGuard |
| `Art.getBlockHeight()` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Art.getBlockWidth()` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Art.getId()` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Art.name()` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Bee.setAnger(int)` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Bee.setHasNectar(boolean)` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Bee.setHasStung(boolean)` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `BlockProjectileSource.getBlock()` | absent | 2 | 2 | GriefPrevention, WorldGuard |
| `Boat.setWoodType(TreeSpecies)` | absent | 2 | 2 | DecentHolograms, Essentials |
| `Cat.setCollarColor(DyeColor)` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `EnderCrystal.setShowingBottom(boolean)` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `Entity.addPassenger(Entity)` | stubbed | 2 | 2 | Chunky, Multiverse-Core |
| `Entity.eject()` | stubbed | 2 | 2 | Chunky, WorldGuard |
| `Entity.getCustomName()` | stubbed | 2 | 2 | Essentials, WorldEdit |
| `Entity.getPersistentDataContainer()` | stubbed | 2 | 2 | DecentHolograms, GriefPrevention |

309 more, each needed by fewer plugins.

Unbridged events, by plugins referencing them:

| Event | Plugins | Which |
|---|---:|---|
| `EntityDamageEvent` | 14 | CoreProtect, CoreProtect, CoreProtect, DecentHolograms, DecentHolograms, Essentials, Essentials, GriefPrevention, GriefPrevention, HuskHomes, ViaVersion, WorldGuard, WorldGuard, WorldGuard |
| `EntityDeathEvent` | 10 | CoreProtect, CoreProtect, DiscordSRV, Essentials, GriefPrevention, GriefPrevention, HuskHomes, ViaVersion, WorldGuard, WorldGuard |
| `EntitySpawnEvent` | 10 | CoreProtect, CoreProtect, CoreProtect, DecentHolograms, GriefPrevention, GriefPrevention, Multiverse-Core, Multiverse-Core, WorldEdit, WorldGuard |
| `EntityChangeBlockEvent` | 7 | ChestShop, CoreProtect, Essentials, GriefPrevention, GriefPrevention, WorldGuard, WorldGuard |
| `EntityCombustEvent` | 6 | Essentials, Essentials, GriefPrevention, WorldGuard, WorldGuard, WorldGuard |
| `HangingBreakEvent` | 6 | CoreProtect, CoreProtect, GriefPrevention, GriefPrevention, WorldGuard, WorldGuard |
| `EntityExplodeEvent` | 5 | ChestShop, CoreProtect, Essentials, GriefPrevention, WorldGuard |
| `EntityPickupItemEvent` | 4 | CoreProtect, Essentials, GriefPrevention, WorldGuard |
| `EntityRegainHealthEvent` | 4 | DecentHolograms, Essentials, Multiverse-Core, WorldGuard |
| `FoodLevelChangeEvent` | 4 | CoreProtect, Essentials, Multiverse-Core, WorldGuard |
| `EntityInteractEvent` | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `EntityTargetEvent` | 3 | Essentials, GriefPrevention, GriefPrevention |
| `HangingPlaceEvent` | 3 | CoreProtect, GriefPrevention, WorldGuard |
| `PotionSplashEvent` | 3 | Essentials, GriefPrevention, WorldGuard |
| `EntityKnockbackEvent` | 2 | GriefPrevention, WorldGuard |
| `EntityTransformEvent` | 2 | CoreProtect, WorldGuard |
| `EntityUnleashEvent` | 2 | WorldGuard, WorldGuard |
| `ExpBottleEvent` | 2 | GriefPrevention, WorldGuard |
| `ProjectileHitEvent` | 2 | GriefPrevention, WorldGuard |
| `VehicleDamageEvent` | 2 | GriefPrevention, WorldGuard |
| `AreaEffectCloudApplyEvent` | 1 | WorldGuard |
| `CreeperPowerEvent` | 1 | WorldGuard |
| `EntityMountEvent` | 1 | WorldGuard |
| `EntityPortalEnterEvent` | 1 | GriefPrevention |
| `EntityPortalEvent` | 1 | Multiverse-Core |
| `EntityPotionEffectEvent` | 1 | WorldGuard |
| `EntityShootBowEvent` | 1 | Essentials |
| `EntityTameEvent` | 1 | WorldGuard |
| `EntityToggleGlideEvent` | 1 | ViaVersion |
| `ExplosionPrimeEvent` | 1 | WorldGuard |
| `ItemMergeEvent` | 1 | GriefPrevention |
| `LingeringPotionSplashEvent` | 1 | WorldGuard |
| `PigZapEvent` | 1 | WorldGuard |
| `PlayerLeashEntityEvent` | 1 | WorldGuard |
| `RaidTriggerEvent` | 1 | GriefPrevention |
| `VehicleDestroyEvent` | 1 | WorldGuard |
| `VehicleEnterEvent` | 1 | WorldGuard |
| `VehicleExitEvent` | 1 | WorldGuard |
| `VehicleMoveEvent` | 1 | WorldGuard |

### Inventory

Methods, by plugins naming them (Direct), then by plugins reaching them through the API's own code (Plugins counts both):

| Method | Status | Plugins | Direct | Which |
|---|---|---:|---:|---|
| `Player.getInventory()` | stubbed | 12 | 12 | ChestShop, CoreProtect, DecentHolograms, DiscordSRV, Essentials, GriefPrevention, HuskHomes, Multiverse-Core, PlaceholderAPI, ViaVersion, WorldEdit, WorldGuard |
| `BookMeta.setAuthor(String)` | absent | 7 | 7 | ChestShop, DecentHolograms, DiscordSRV, Essentials, GriefPrevention, HuskHomes, PlaceholderAPI |
| `BookMeta.setTitle(String)` | absent | 7 | 7 | ChestShop, DecentHolograms, DiscordSRV, Essentials, GriefPrevention, HuskHomes, PlaceholderAPI |
| `PlayerInventory.getItemInMainHand()` | absent | 7 | 7 | ChestShop, CoreProtect, DecentHolograms, Essentials, GriefPrevention, WorldEdit, WorldGuard |
| `PlayerInventory.getItem(int)` | absent | 6 | 6 | CoreProtect, Essentials, GriefPrevention, Multiverse-Core, ViaVersion, WorldGuard |
| `PlayerInventory.setItemInHand(ItemStack)` | absent | 6 | 6 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI, ViaVersion |
| `InventoryView.getTopInventory()` | absent | 6 | 5 | ChestShop, CoreProtect, DecentHolograms, Essentials, ViaVersion, WorldGuard |
| `BookMeta.Spigot.addPage(BaseComponent[][])` | stubbed | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `BookMeta.spigot()` | absent | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Inventory.getHolder()` | absent | 5 | 5 | CoreProtect, Essentials, Multiverse-Core, WorldEdit, WorldGuard |
| `Inventory.getType()` | absent | 5 | 5 | CoreProtect, Essentials, GriefPrevention, ViaVersion, WorldGuard |
| `Player.openBook(ItemStack)` | stubbed | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `PlayerInventory.addItem(ItemStack[])` | absent | 5 | 5 | CoreProtect, GriefPrevention, Multiverse-Core, WorldEdit, WorldGuard |
| `PlayerInventory.getItemInHand()` | absent | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `PlayerInventory.getItemInOffHand()` | absent | 5 | 5 | CoreProtect, Essentials, GriefPrevention, WorldEdit, WorldGuard |
| `PotionMeta.getCustomEffects()` | absent | 5 | 5 | ChestShop, CoreProtect, DecentHolograms, Essentials, WorldGuard |
| `Damageable.setDamage(int)` | absent | 12 | 4 | ChestShop, CoreProtect, DecentHolograms, DiscordSRV, Essentials, GriefPrevention, HuskHomes, Multiverse-Core, PlaceholderAPI, ViaVersion, WorldEdit, WorldGuard |
| `BlockStateMeta.setBlockState(BlockState)` | absent | 4 | 4 | CoreProtect, DecentHolograms, Essentials, ViaVersion |
| `EnchantmentStorageMeta.getStoredEnchants()` | absent | 4 | 4 | ChestShop, CoreProtect, DecentHolograms, Essentials |
| `Inventory.clear()` | absent | 4 | 4 | CoreProtect, Essentials, GriefPrevention, WorldEdit |
| `Inventory.getContents()` | absent | 4 | 4 | CoreProtect, DecentHolograms, Essentials, ViaVersion |
| `InventoryHolder.getInventory()` | stubbed | 4 | 4 | ChestShop, CoreProtect, GriefPrevention, WorldEdit |
| `ItemMeta.getLore()` | absent | 4 | 4 | ChestShop, CoreProtect, DecentHolograms, Essentials |
| `ItemMeta.hasLore()` | absent | 4 | 4 | ChestShop, CoreProtect, DecentHolograms, Essentials |
| `LeatherArmorMeta.getColor()` | absent | 4 | 4 | ChestShop, CoreProtect, DecentHolograms, Essentials |
| `PlayerInventory.getArmorContents()` | absent | 4 | 4 | CoreProtect, Essentials, GriefPrevention, ViaVersion |
| `PlayerInventory.getContents()` | absent | 4 | 4 | Essentials, GriefPrevention, WorldEdit, WorldGuard |
| `PlayerInventory.getHelmet()` | absent | 4 | 4 | CoreProtect, Essentials, ViaVersion, WorldGuard |
| `PotionMeta.getBasePotionType()` | absent | 4 | 4 | ChestShop, DecentHolograms, Essentials, WorldGuard |
| `Damageable.getDamage()` | absent | 6 | 3 | ChestShop, CoreProtect, DecentHolograms, Essentials, ViaVersion, WorldGuard |
| `ItemMeta.getEnchants()` | absent | 4 | 3 | ChestShop, CoreProtect, DecentHolograms, Essentials |
| `ArmorMeta.getTrim()` | absent | 3 | 3 | ChestShop, DecentHolograms, Essentials |
| `BannerMeta.addPattern(Pattern)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `BannerMeta.getPatterns()` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `BlockStateMeta.getBlockState()` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `BookMeta.getAuthor()` | absent | 3 | 3 | ChestShop, DecentHolograms, Essentials |
| `BookMeta.getTitle()` | absent | 3 | 3 | ChestShop, DecentHolograms, Essentials |
| `CrossbowMeta.getChargedProjectiles()` | absent | 3 | 3 | ChestShop, CoreProtect, DecentHolograms |
| `Enchantment.getName()` | stubbed | 3 | 3 | ChestShop, DecentHolograms, Essentials |
| `EntityEquipment.setItemInMainHand(ItemStack)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `FireworkMeta.addEffect(FireworkEffect)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `FireworkMeta.getEffects()` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `Inventory.getItem(int)` | absent | 3 | 3 | CoreProtect, DecentHolograms, WorldGuard |
| `Inventory.getStorageContents()` | absent | 3 | 3 | CoreProtect, DecentHolograms, ViaVersion |
| `Inventory.setItem(int, ItemStack)` | absent | 3 | 3 | DecentHolograms, Essentials, WorldGuard |
| `ItemMeta.getDisplayName()` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `ItemMeta.hasDisplayName()` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `ItemMeta.setDisplayName(String)` | absent | 3 | 3 | ChestShop, DecentHolograms, Essentials |
| `LeatherArmorMeta.setColor(Color)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `PersistentDataContainer.remove(NamespacedKey)` | absent | 3 | 3 | Essentials, GriefPrevention, HuskHomes |
| `PersistentDataContainer.set(NamespacedKey, PersistentDataType, Object)` | absent | 3 | 3 | Essentials, GriefPrevention, HuskHomes |
| `Player.closeInventory()` | stubbed | 3 | 3 | ChestShop, Essentials, GriefPrevention |
| `Player.getItemInHand()` | stubbed | 3 | 3 | GriefPrevention, ViaVersion, WorldGuard |
| `Player.getOpenInventory()` | stubbed | 3 | 3 | ChestShop, Essentials, ViaVersion |
| `Player.updateInventory()` | stubbed | 3 | 3 | ChestShop, CoreProtect, Essentials |
| `PlayerInventory.getHeldItemSlot()` | absent | 3 | 3 | Multiverse-Core, ViaVersion, WorldGuard |
| `PlayerInventory.setItem(int, ItemStack)` | absent | 3 | 3 | Essentials, ViaVersion, WorldGuard |
| `PlayerInventory.setItemInMainHand(ItemStack)` | absent | 3 | 3 | Essentials, ViaVersion, WorldGuard |
| `PotionMeta.addCustomEffect(PotionEffect, boolean)` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |
| `PotionMeta.clearCustomEffects()` | absent | 3 | 3 | CoreProtect, DecentHolograms, Essentials |

259 more, each needed by fewer plugins.

Unbridged events, by plugins referencing them:

| Event | Plugins | Which |
|---|---:|---|
| `InventoryClickEvent` | 6 | CoreProtect, CoreProtect, Essentials, ViaVersion, WorldGuard, WorldGuard |
| `InventoryCloseEvent` | 3 | ChestShop, Essentials, ViaVersion |
| `InventoryDragEvent` | 3 | CoreProtect, Essentials, WorldGuard |
| `InventoryMoveItemEvent` | 3 | ChestShop, CoreProtect, WorldGuard |
| `InventoryOpenEvent` | 2 | ChestShop, WorldGuard |
| `InventoryPickupItemEvent` | 2 | CoreProtect, GriefPrevention |

### Server

Methods, by plugins naming them (Direct), then by plugins reaching them through the API's own code (Plugins counts both):

| Method | Status | Plugins | Direct | Which |
|---|---|---:|---:|---|
| `Server.getOfflinePlayer(String)` | stubbed | 10 | 7 | ChestShop, CoreProtect, DiscordSRV, Essentials, GriefPrevention, LuckPerms, Multiverse-Core, PlaceholderAPI, Vault, WorldEdit |
| `BossBar.setColor(BarColor)` | absent | 7 | 7 | ChestShop, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI, TAB |
| `BossBar.setStyle(BarStyle)` | absent | 7 | 7 | ChestShop, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI, TAB |
| `BossBar.setTitle(String)` | absent | 7 | 7 | ChestShop, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI, TAB |
| `UnsafeValues.getDataVersion()` | stubbed | 7 | 6 | ChestShop, CoreProtect, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI, WorldEdit |
| `BossBar.addFlag(BarFlag)` | absent | 6 | 6 | ChestShop, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `BossBar.addPlayer(Player)` | absent | 6 | 6 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI, TAB |
| `BossBar.removeFlag(BarFlag)` | absent | 6 | 6 | ChestShop, DecentHolograms, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `BossBar.removePlayer(Player)` | absent | 6 | 6 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI, TAB |
| `BossBar.setProgress(double)` | absent | 6 | 6 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI, TAB |
| `ProxiedCommandSender.getCallee()` | absent | 6 | 6 | ChestShop, DiscordSRV, Essentials, HuskHomes, LuckPerms, PlaceholderAPI |
| `BlockCommandSender.getBlock()` | absent | 5 | 5 | CoreProtect, Essentials, LuckPerms, Multiverse-Core, WorldEdit |
| `BossBar.getPlayers()` | absent | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `BossBar.isVisible()` | absent | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `BossBar.removeAll()` | absent | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `BossBar.setVisible(boolean)` | absent | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Server.getOfflinePlayer(UUID)` | stubbed | 8 | 4 | ChestShop, CoreProtect, DiscordSRV, Essentials, GriefPrevention, LuckPerms, Multiverse-Core, WorldGuard |
| `BanList.addBan(String, String, Date, String)` | absent | 4 | 4 | DiscordSRV, Essentials, GriefPrevention, WorldGuard |
| `BanList.pardon(String)` | absent | 3 | 3 | DiscordSRV, Essentials, GriefPrevention |
| `Server.matchPlayer(String)` | stubbed | 3 | 3 | CoreProtect, Essentials, Multiverse-Core |
| `Server.getBanList(BanList.Type)` | stubbed | 4 | 2 | DiscordSRV, Essentials, GriefPrevention, WorldGuard |
| `Server.getOfflinePlayers()` | stubbed | 3 | 2 | DiscordSRV, GriefPrevention, Vault |
| `Advancement.getKey()` | absent | 2 | 2 | DiscordSRV, Essentials |
| `BanList.isBanned(String)` | absent | 2 | 2 | DiscordSRV, Essentials |
| `HelpMap.registerHelpTopicFactory(Class, HelpTopicFactory)` | absent | 2 | 2 | Multiverse-Core, WorldEdit |
| `PlayerProfile.getTextures()` | absent | 2 | 2 | DecentHolograms, Essentials |
| `PlayerProfile.update()` | absent | 2 | 2 | DecentHolograms, Essentials |
| `PlayerTextures.getSkin()` | absent | 2 | 2 | CoreProtect, DecentHolograms |
| `PlayerTextures.setSkin(URL)` | absent | 2 | 2 | CoreProtect, Essentials |
| `UnsafeValues.modifyItemStack(ItemStack, String)` | stubbed | 2 | 2 | DecentHolograms, Essentials |
| `Server.Spigot.getConfig()` | stubbed | 4 | 1 | Essentials, Multiverse-Core, WorldEdit, WorldGuard |
| `Server.getHelpMap()` | stubbed | 3 | 1 | Essentials, Multiverse-Core, WorldEdit |
| `AdvancementDisplay.shouldAnnounceChat()` | absent | 1 | 1 | DiscordSRV |
| `BanEntry.getExpiration()` | absent | 1 | 1 | Essentials |
| `BanEntry.getReason()` | absent | 1 | 1 | Essentials |
| `BanEntry.getTarget()` | absent | 1 | 1 | Essentials |
| `BanList.getBanEntries()` | absent | 1 | 1 | Essentials |
| `BanList.getBanEntry(String)` | absent | 1 | 1 | Essentials |
| `BlockCommandSender.getName()` | absent | 1 | 1 | WorldEdit |
| `BlockCommandSender.hasPermission(String)` | absent | 1 | 1 | WorldEdit |
| `BlockCommandSender.sendMessage(String)` | absent | 1 | 1 | WorldEdit |
| `ConsoleCommandSender.setOp(boolean)` | stubbed | 1 | 1 | Essentials |
| `IpBanList.getBanEntry(Object)` | absent | 1 | 1 | Essentials |
| `Lootable.getLootTable()` | absent | 1 | 1 | GriefPrevention |
| `Metadatable.getMetadata(String)` | stubbed | 1 | 1 | WorldGuard |
| `Metadatable.removeMetadata(String, Plugin)` | stubbed | 1 | 1 | WorldGuard |
| `Metadatable.setMetadata(String, MetadataValue)` | stubbed | 1 | 1 | WorldGuard |
| `OldEnum.name()` | absent | 1 | 1 | WorldEdit |
| `PermissionRemovedExecutor.attachmentRemoved(PermissionAttachment)` | absent | 1 | 1 | LuckPerms |
| `PlayerProfile.getName()` | absent | 1 | 1 | DecentHolograms |
| `PlayerProfile.getUniqueId()` | absent | 1 | 1 | DecentHolograms |
| `ProfileBanList.getBanEntry(Object)` | absent | 1 | 1 | Essentials |
| `Server.createPlayerProfile(UUID)` | stubbed | 1 | 1 | Essentials |
| `Server.createPlayerProfile(UUID, String)` | stubbed | 1 | 1 | Essentials |
| `Server.getAllowFlight()` | stubbed | 1 | 1 | WorldGuard |
| `Server.getAmbientSpawnLimit()` | stubbed | 1 | 1 | WorldGuard |
| `Server.getAnimalSpawnLimit()` | stubbed | 1 | 1 | WorldGuard |
| `Server.getBannedPlayers()` | stubbed | 1 | 1 | DiscordSRV |
| `Server.getConnectionThrottle()` | stubbed | 1 | 1 | WorldGuard |
| `Server.getIPBans()` | stubbed | 1 | 1 | DiscordSRV |

28 more, each needed by fewer plugins.

Unbridged events, by plugins referencing them:

| Event | Plugins | Which |
|---|---:|---|
| `RemoteServerCommandEvent` | 2 | DiscordSRV, LuckPerms |
| `ServerCommandEvent` | 2 | DiscordSRV, LuckPerms |
| `ServerLoadEvent` | 2 | ChestShop, PlaceholderAPI |
| `ServerListPingEvent` | 1 | Essentials |

### Not in the Spigot API

Paper or Folia API (plugins mostly call it after detecting Paper), or API removed since the plugin was built: a NoSuchMethodError if reached on Ferraloom.

| Method | Status | Plugins | Direct | Which |
|---|---|---:|---:|---|
| `Server.getGlobalRegionScheduler()` | missing | 7 | 7 | CoreProtect, HuskHomes, LuckPerms, PlaceholderAPI, ViaVersion, WorldEdit, WorldGuard |
| `Server.getRegionScheduler()` | missing | 6 | 6 | Chunky, CoreProtect, HuskHomes, LuckPerms, PlaceholderAPI, ViaVersion |
| `Block.getState(boolean)` | missing | 5 | 5 | ChestShop, Essentials, Multiverse-Core, WorldEdit, WorldGuard |
| `Entity.getScheduler()` | missing | 5 | 5 | Chunky, CoreProtect, HuskHomes, LuckPerms, PlaceholderAPI |
| `Player.hideTitle()` | missing | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Player.sendTitle(Title)` | missing | 5 | 5 | ChestShop, DiscordSRV, Essentials, HuskHomes, PlaceholderAPI |
| `Server.getAsyncScheduler()` | missing | 5 | 5 | CoreProtect, HuskHomes, PlaceholderAPI, WorldEdit, WorldGuard |
| `World.getChunkAtAsync(int, int, boolean)` | missing | 5 | 5 | Chunky, Essentials, Multiverse-Core, WorldEdit, WorldGuard |
| `Entity.teleportAsync(Location)` | missing | 4 | 4 | Chunky, CoreProtect, WorldEdit, WorldGuard |
| `Entity.teleportAsync(Location, PlayerTeleportEvent.TeleportCause)` | missing | 4 | 4 | Essentials, Multiverse-Core, WorldEdit, WorldGuard |
| `Player.getPotentialBedLocation()` | missing | 4 | 4 | Essentials, Multiverse-Core, WorldEdit, WorldGuard |
| `World.ChunkLoadCallback` | missing | 4 | 4 | Essentials, Multiverse-Core, WorldEdit, WorldGuard |
| `World.getChunkAtAsync(int, int, World.ChunkLoadCallback)` | missing | 4 | 4 | Essentials, Multiverse-Core, WorldEdit, WorldGuard |
| `World.getChunkAtAsync(int, int, boolean, boolean)` | missing | 4 | 4 | Essentials, Multiverse-Core, WorldEdit, WorldGuard |
| `Player.getScheduler()` | missing | 3 | 3 | Chunky, ViaVersion, WorldGuard |
| `World.getGameRuleValue(String)` | missing | 3 | 3 | DiscordSRV, ViaVersion, WorldGuard |
| `Bukkit.createProfile(String)` | missing | 2 | 2 | CoreProtect, WorldGuard |
| `Bukkit.createProfile(UUID)` | missing | 2 | 2 | CoreProtect, WorldGuard |
| `Bukkit.getCommandMap()` | missing | 2 | 2 | Essentials, WorldEdit |
| `Bukkit.getCurrentTick()` | missing | 2 | 2 | Essentials, WorldEdit |
| `Bukkit.getGlobalRegionScheduler()` | missing | 2 | 2 | WorldEdit, WorldGuard |
| `Bukkit.getRegionScheduler()` | missing | 2 | 2 | WorldEdit, WorldGuard |
| `Bukkit.isOwnedByCurrentRegion(Location)` | missing | 2 | 2 | WorldEdit, WorldGuard |
| `Bukkit.isStopping()` | missing | 2 | 2 | Essentials, ViaVersion |
| `CommandSender.sendMessage(Component)` | missing | 2 | 2 | Chunky, Essentials |
| `DoubleChest.getLeftSide(boolean)` | missing | 2 | 2 | ChestShop, WorldGuard |
| `DoubleChest.getRightSide(boolean)` | missing | 2 | 2 | ChestShop, WorldGuard |
| `Player.locale()` | missing | 2 | 2 | LuckPerms, Multiverse-Core |
| `Player.teleportAsync(Location)` | missing | 2 | 2 | Chunky, WorldEdit |
| `Server.isGlobalTickThread()` | missing | 2 | 2 | HuskHomes, PlaceholderAPI |

157 more, each needed by fewer plugins.

### Server internals

Classes naming `net.minecraft` or `org.bukkit.craftbukkit`. Main: reached from the plugin's main class through direct references, not version-named. Adapter: version-named or not reached directly (picked by version string). Reflective: a string constant only.

- **ChestShop**: `CraftBukkitFacet.PacketFacet` (reflective, net.minecraft.server), `MinecraftReflection` (reflective, net.minecraft.server)
- **DecentHolograms**: `SkullUtils` (reflective, net.minecraft.world.item.component.ResolvableProfile), `ReflectionUtil` (reflective, net.minecraft.server.), `NBTReflectionUtil` (reflective, net.minecraft.core.component.DataComponents), `DataFixerUtil` (reflective, net.minecraft.util.datafix.fixes.References), `NBTJsonUtil` (reflective, net.minecraft.world.item.ItemStack), `ClassWrapper` (reflective, net.minecraft.nbt), `Forge1710Mappings` (reflective, net.minecraft.nbt.NBTBase), `MojangToMapping.1` (reflective, net.minecraft.nbt.CompoundTag), 433 more
- **DiscordSRV**: `DiscordSRV` (reflective, net.minecraft.server.MinecraftServer), `CraftBukkitFacet.PacketFacet` (reflective, net.minecraft.server), `MinecraftReflection` (reflective, net.minecraft.server), `PlayerAdvancementDoneListener` (reflective, net.minecraft.advancements.Advancement), `NMSUtil` (reflective, org.bukkit.craftbukkit.)
- **Essentials**: `CraftBukkitFacet.PacketFacet` (reflective, net.minecraft.server), `MinecraftReflection` (reflective, net.minecraft.server), `VersionUtil` (reflective, net.minecraft.server.MinecraftServer), `ReflUtil` (reflective, org.bukkit.craftbukkit.CraftServer), `AdvancementListenerProvider` (reflective, net.minecraft.locale.Language)
- **HuskHomes**: `CraftBukkitFacet.PacketFacet` (reflective, net.minecraft.server), `MinecraftReflection` (reflective, net.minecraft.server)
- **LuckPerms**: `CraftBukkitImplementation` (reflective, org.bukkit.craftbukkit)
- **Multiverse-Core**: `SpawnCategoryMapper` (reflective, net.minecraft.world.entity.EntityType)
- **PlaceholderAPI**: `CraftBukkitFacet.PacketFacet` (reflective, net.minecraft.server), `MinecraftReflection` (reflective, net.minecraft.server)
- **TAB**: `NMSComponentConverter` (adapter, net.minecraft.network.chat.Component), `NMSImplementationProvider` (adapter, org.bukkit.craftbukkit.entity.CraftPlayer), `NMSPacketScoreboard` (adapter, net.minecraft.world.scores.criteria.ObjectiveCriteria$RenderType), `NMSPacketTabList` (adapter, net.minecraft.network.protocol.game.ClientboundPlayerInfoRemovePacket), `NMSTabListEntryTracker` (adapter, net.minecraft.network.protocol.game.ClientboundPlayerInfoRemovePacket), `CanvasPacketScoreboard` (adapter, net.minecraft.world.scores.Objective), `NMSComponentConverter` (adapter, net.minecraft.network.chat.Component), `NMSImplementationProvider` (adapter, org.bukkit.craftbukkit.entity.CraftPlayer), 166 more
- **ViaVersion**: `PlayerSneakListener` (reflective, org.bukkit.craftbukkit), `BukkitViaInjector` (reflective, net.minecraft.SharedConstants), `NMSUtil` (reflective, net.minecraft.server.MinecraftServer)
- **WorldEdit**: `Refraction` (reflective, org.bukkit.craftbukkit.CraftServer), `ComponentConverter.Serializer` (adapter, net.minecraft.core.HolderLookup$a), `PaperweightAdapter` (adapter, net.minecraft.core.IRegistryCustom$Dimension), `PaperweightAdapter.1` (adapter, net.minecraft.world.level.block.state.properties.IBlockState), `PaperweightAdapter.MojangWatchdog` (adapter, net.minecraft.server.dedicated.DedicatedServer), `PaperweightBlockMaterial` (adapter, net.minecraft.world.level.block.state.IBlockData), `PaperweightDataConverters` (adapter, net.minecraft.nbt.DynamicOpsNBT), `PaperweightDataConverters.DataConverter` (adapter, net.minecraft.nbt.NBTTagCompound), 824 more
