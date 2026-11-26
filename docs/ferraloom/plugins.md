# Bukkit plugins

Ferraloom runs Spigot plugins (`.jar`, Bukkit API) on its dedicated server
without costing the server anything when no plugin uses a feature. This page
is the design, the rules every API area follows, and the plan for building
the API out in parallel. The foundation (a vertical slice, below) is in place.

## Decisions

| Question | Answer |
|---|---|
| Where Java runs | In the server process: a JVM started through the JNI invocation API. `libjvm` is found and `dlopen`ed at run time, only when `plugins/` holds a jar. No link-time Java dependency: without plugins no JVM, no thread, no mapping (`/proc/<pid>/maps` has no `libjvm`; the e2e test checks it). |
| Why not a separate process | A cancellable event is a synchronous question; out of process every listener call is an IPC round trip on the tick thread. |
| What plugins see | The real `org.bukkit.*` interfaces, implemented by Ferraloom's Java bridge (`crates/ferraloom-plugins/java/bridge/`), whose methods call into Rust. Bukkit's own machinery from the API jar does the rest: `JavaPluginLoader`, `SimplePluginManager`, `PluginDescriptionFile`, `SimpleCommandMap`, `PermissibleBase`, `SimpleServicesManager`, `StandardMessenger`, the configuration classes. |
| NMS / CraftBukkit | Not provided. Multi-version plugins carry adapters for them that they pick by version string and may never run (no plugin in the usage corpus, `plugin-usage.md`, reaches them from its main path), so a jar whose classes name `net/minecraft/` or `org/bukkit/craftbukkit/` loads with one warning: `'plugins/X.jar' names net.minecraft.server.MinecraftServer (from com.example.v1_21.Adapter and 3 more classes), Minecraft server internals (NMS or CraftBukkit) that Ferraloom does not provide; it is disabled if it reaches one`. If its code reaches a missing internal class and the error escapes to Bukkit (a listener, task, command or `onEnable`), the log handler prints one line instead of the trace, `Disabling X v1.0: it reached net.minecraft.server.MinecraftServer (from com.example.v1_21.Adapter.server), ...`, and the plugin is disabled at the next tick's start; a plugin that probes (`Class.forName`) and handles the failure is left alone. No cost on any path that logs no throwable. |
| Unimplemented API | Generated stubs throw `UnsupportedOperationException("Ferraloom does not implement Player.getHealth() yet")`. |
| API artifact | `org.spigotmc:spigot-api:26.3-R0.1-SNAPSHOT`, classifier `shaded`, pinned to build `20261123.204233-11` by SHA-256 in `java/api.lock.json`. Spigot publishes 26.3 only as snapshots (hub.spigotmc.org snapshots repository; no release). The shaded jar carries the API's runtime libraries (Guava 33.6, Gson 2.14, SnakeYAML 2.7, BungeeCord chat 1.21-R0.4, JOML 1.10.9). Not in it, as CraftBukkit provides them: commons-lang3 and the Maven resolver, so `libraries:` in `plugin.yml` does not work yet (the loader logs "Could not initialize LibraryLoader"). The jar is fetched by `build.py`, never committed. |

## Layout

| Path | What |
|---|---|
| `crates/ferraloom-plugins/src/` | The Rust side, no `horizon-client` dependency. `lib.rs` (public API, `PluginHost`), `events.rs` (listener bits), `jvm.rs` (find/load/create the JVM), `bridge.rs` (runtime, lifecycle, the `Entry` and `call` helpers), `natives.rs` (host lending, deferral, the exception wall), one module per API area: `players.rs`, `worlds.rs`, `server.rs` (host trait, natives, event calls), `commands.rs` (command tree JSON). |
| `crates/ferraloom-plugins/java/` | The Java side, built with the JDK alone by `build.py`. `bridge/dev/ferraloom/bukkit/`: `Bridge` (lifecycle), `PlayerEvents`/`BlockEvents` (event entry points), `ServerNative`/`PlayerNative`/`WorldNative` (natives), `Ferraloom*` (the implementations), `FerraloomScheduler`, `FerraloomCommandMap`, `Internals` (server internals: the load warning, disabling a plugin that reaches one), `NativeLog`. `tools/StubGen.java`, `tools/Coverage.java`, `stubs.txt`, `api.lock.json`, `test-plugin/`, `nms-plugin/`. |
| `crates/horizon-client/src/game/plugins.rs`, `game/plugins/{players,worlds,server,entities}.rs` | The game's side: `GameHost` (the host over `Game`), lifecycle, event glue per area. |
| `crates/horizon-client/src/mobs/plugin_sites.rs` | The entity events fired inside mob code, and the rule for the parallel mob pass. |
| `crates/horizon-client/src/lan/hub/plugins.rs` | Plugin commands in the hub's command tree. |
| `tools/plugins_e2e.py`, `crates/ferraloom-plugins/examples/plugin_bot.rs` | The end-to-end test and its scripted player. |

Running plugins: `python3 crates/ferraloom-plugins/java/build.py --install <server folder>` puts
`ferraloom-bukkit.jar` and the API jar in `<server folder>/ferraloom-bukkit/` (or point
`FERRALOOM_BUKKIT_DIR` at a folder holding them), plugins go in `plugins/`. Java is found through
`FERRALOOM_JVM` (a libjvm or a Java home), `JAVA_HOME`, then `java` on `PATH`; `FERRALOOM_JVM_OPTIONS` adds JVM
options (`-Xmx2G`). The JVM gets `-Xrs`, so Ctrl-C and SIGTERM stay the server's.

## Lifecycle

As CraftBukkit orders it in `DedicatedServer.initServer` and `MinecraftServer.stopServer`:

1. `start_plugins` before "Preparing level": JVM, natives bound, `Bukkit.setServer`, the NMS check, every
   plugin's `onLoad` ("[Name] Loading Name vX").
2. `enable_plugins` before "Done": both load orders (`STARTUP` and `POSTWORLD`, the worlds exist by then),
   `plugin.yml` permissions, commands registered by `SimplePluginManager.enablePlugin`, then `onEnable`.
3. Each tick, before the tick runs (`tickChildren`'s heartbeat; not while the server is paused empty):
   queued off-thread messages go out, due sync tasks run, the listener bits are latched, changed command
   labels are fetched.
4. `disable_plugins` right after "Stopping server": `onDisable` in reverse order, tasks cancelled, the chat
   thread drained.

## Threading and event dispatch

**The server thread is Bukkit's main thread.** It creates the JVM (so it stays attached) and is the only
thread that calls into Java, except `chat`, which only queues. `Bukkit.isPrimaryThread()` is true on it.

**Calls lend the host.** Every call into Java that can run plugin code takes a `&mut dyn PluginHost` and lends
it for its length (`natives::lend`). A native called on the server thread during that call reaches the
game through it, synchronously, as the call's own continuation; it is taken out of the slot while in use,
so no native can reach it twice at once, and a nested call lends its own. Off the server thread, or between
calls, there is no host: `sendMessage`, console lines, logging and `shutdown` are queued and delivered at
the next tick's start; everything else throws `IllegalStateException("Player.getLocation is answered on the
server thread only (schedule a sync task)")`, as Spigot's AsyncCatcher does.

**Host methods are leaves.** They read and change `Game` and never call into Java. When a host method must
fire an event (a future `PlayerTeleportEvent` from `teleport`), it lends itself to that call, never the host
it was reached through. This keeps the borrow chain sound: event site `&mut Game` → `GameHost` → lent
pointer → native → host method, each reborrowing the one before, none used while a later one lives.

**Where the game's state is.** Horizon swaps a guest into `Game` for their turn (`as_guest`) and takes the
hub (`LanHost`) out of `Game::lan` while it runs. `GameHost` resolves both: the acting guest's position is
the game's own player; other players are read from the hub, which is in `Game::lan`, passed in by a packet
handler, or lent by `as_guest` (`plugins::lend_hub`, a thread-local pointer to the hub the turn holds and
does not touch until the turn ends). What needs the hub in `Game::lan` while it is elsewhere (moving another
player through `for_player`, a vanilla command) is queued (`Pending`) for the next tick's start; it still
answers `true`. This is the one place the slice is not synchronous; it disappears when players stop being
swapped (`core.md`, step 3).

**Listener bits, latched per tick.** `Bridge.tick` answers a bit per bridged event: 1 when its `HandlerList`
has a registered listener. Rust stores them in one `AtomicU64` (`events.rs`) between ticks only, never during
one. An event site is

```rust
if ferraloom_plugins::listens(Event::BlockBreak) && self.plugin_block_break(pos) {
    return Ok(());
}
```

and every decision in a tick sees the same bits: the hub leaving the join message to plugins and the game
sending the plugin's agree. A listener registered mid-tick takes effect at the next tick.

**Cancellation before the mutation.** A cancellable event's site is placed where CraftBukkit fires it, before
the change it guards, and returns before it: `BlockBreakEvent` is checked in `break_block_as_player` after
`canDestroyBlock` and before `playerWillDestroy`; a cancelled break changes nothing and resends the block to
the player (whose client predicted the break), as CraftBukkit does.

**Parallel sections.** Mobs tick speculatively on the pool (`mobs/tick_loop.rs`: a tick that read something
another changed, or that called `note_impure`, reruns serially in mob order). An event whose site is inside a
parallel section follows one rule: *with a listener, that site takes the serial path.* Concretely, in the
mob loop: the site checks `listens(E)`; in a speculative tick it calls `tick_loop::note_impure()` and
returns the speculative tick's result unused, so the mob reruns in its serial turn, where the site calls the
plugin and can be cancelled before it mutates. The bits are latched for the whole tick, so the speculative and
serial passes agree on whether an event is listened to. Without a listener the parallel path is untouched.
The serial mob turn runs with the mobs taken out of `Game`, so its host is a restricted one: reads come from
the pass's own state (the mob, the snapshot), and changes to other things are queued as the pass already
queues `MobEvent`s, applied in mob order. The entities area builds that host (`EntityHost`, below); nothing in
the slice runs inside a parallel section. The network tick's parallel parts (encoding, compression, entity
tracking prefetch) fire no events: packets are handled serially in `lan_guest_packet`, and that is where
player events fire.

**Chat is asynchronous**, as in Spigot. The hub hands the line to `AsyncPlayerChatEvent` on Java's single "Async
Chat Thread" (`isAsynchronous()` true) and skips its own broadcast; the event's format and recipients decide
who gets what, delivered as system messages at the start of the next tick. The signed player-chat path is
not used while a plugin listens: plugin-handled chat goes out unsigned, like Spigot's with a format.

## Java objects and server state

Java objects hold identity, not state. A `FerraloomPlayer` is a UUID (its two halves cross JNI), its name,
entity id and operator status read at join; everything else is asked of the host when called. A
`FerraloomWorld` is an index into the fixed world list given at start (`world`, `world_nether`,
`world_the_end`, CraftBukkit's names). A `FerraloomBlock` is a world and a position; its type is read when
asked, or is the state the event saw.

A player's object lives from join to quit. During `PlayerQuitEvent` it is still online; after it, it is
offline for good: name and UUID answer, `getLocation()` is where they left, messages are dropped, teleports
answer false. A reconnect makes a new object (CraftBukkit does the same), so a plugin that kept the old one
sees `isOnline()` false. Entities, inventories and items follow the same rule when they arrive: a handle is
an id plus a generation the host checks, so a stale handle answers as removed, never as another entity.

## Entities

The entities area (`src/entities.rs`, `game/plugins/entities.rs`, `mobs/plugin_sites.rs`; Java `Entities`,
`FerraloomEntity`, `FerraloomLivingEntity`, `FerraloomMob`, `EntityNative`, `EntityEvents`).

**Handles.** A non-player entity crosses as its id and a generation, the FNV-1a hash of its type key.
`Game::next_entity` only grows during a run, so an id never names two entities, and the type check keeps
apart the game's separate lists (mobs, items) and conversions. A gone handle reads as removed: `isValid()`
false, `getLocation()` null, setters do nothing. An event naming a player passes the players area's object.

**Typed objects without a class per type.** A plugin gets a `java.lang.reflect.Proxy` over its type's own
interface (`EntityType.getEntityClass()`: `Zombie`, `Wolf`, `Item`), so `instanceof Monster`, `Animals`,
`Tameable`, `Ageable`, `Item`, `Projectile`... answer as on CraftBukkit for every type. A call goes to the first
of `FerraloomMob` (Mob, Tameable, Ageable), `FerraloomLivingEntity` and `FerraloomEntity` that declares it, a
default method runs as written, anything else throws the stub's `Ferraloom does not implement Zombie.isBaby()
yet`. Equality and hash are the handle's. Plugin-side state lives in the game's own fields or side tables keyed
by id; `Mob` does not grow.

**Two hosts.** `GameHost` answers the level in place (mobs, items, players), where a site holds the game: a
spawn, a death, an explosion, a packet. Other worlds' entities read as gone and their lists are empty;
`spawnEntity` there throws (`game/levels.rs` keeps them parked). Inside mob code the game is not reachable
(the mob is borrowed out of it), so events there get `SiteHost`: it reads the mob in hand, and in the mob
pass the pass's own view (the snapshot of the other mobs, the player the mob sees, the blocks, the time); a
change to the mob in hand is made at once, a change to another entity is queued (`plugin_sites::queue`) and
applied in mob order after the pass (or at the next level tick); a read it cannot answer throws
`IllegalStateException` rather than answer wrong.

**The mob pass.** The sites follow the rule in "Parallel sections". `Mob::damage_with` and the tick's watched
fields (target, fire) call `note_impure` in a speculative tick and reach the plugin in the mob's serial
turn. Block changes leave a tick as `MobEvent`s, so `EntityChangeBlockEvent` fires at each mob's commit, in mob
order, and costs no serial tick: a cancelled change's event is dropped. `FERRALOOM_CHECK_MOBS=1` runs the
serial pass on a copy and puts its queued changes back, so listeners are called twice there.
`speculative_ticks_match_the_serial_loop_with_entity_listeners` runs the crowd with listeners for all four
and checks mobs, events, random and queued changes against the serial loop.

What a listener turns serial, per tick:

| Listened event | Mobs whose tick runs again in turn |
|---|---|
| `EntityTargetEvent` | those whose target changed this tick (acquiring or losing one) |
| `EntityCombustEvent` | those whose fire grew: every undead burning in daylight, each tick it burns |
| `EntityDamageEvent` | those hurt inside their own tick (drowning, burning, falling, suffocation, effects); damage from others is dealt after the pass |
| `EntityChangeBlockEvent`, the rest | none |

| Event | Fired at | Notes |
|---|---|---|
| `EntityDamageEvent`, `...ByEntityEvent` | `Mob::damage_with` past the cooldown check, before the health changes; `hurt_player_from`/`hurt_player_by` for players | cancel, `setDamage`. Armor and enchantments cross as one ARMOR modifier. A player's cooldown and armor apply after the event. The damager is the causing entity (an arrow's shooter, not the arrow). |
| `EntityDeathEvent` | `drop_death_loot`, the loot collected and not dropped | experience and drops changes |
| `CreatureSpawnEvent` | as `spawn_mob_for` and natural spawning add the mob | cancel removes it; reasons NATURAL, CHUNK_GEN, CUSTOM (`spawnEntity`), DEFAULT for the rest (CraftBukkit: COMMAND, SPAWNER_EGG...) |
| `EntityTargetEvent` | the end of a mob's tick that changed its target | cancel puts the old target back, after a tick with the new one; a brain mob's memory may set it again |
| `EntityCombustEvent` | the end of a mob's tick that set it burning longer | mobs in their own tick only (not a player's fire aspect) |
| `EntityChangeBlockEvent` | a mob's commit in the pass | its BlockData comes from the blocks area; the mob's own state stays (a refused sheep is fed), but an enderman keeps what it held |
| `EntityExplodeEvent` | `server_explode_as`, before the blocks shuffle and break | blasts a mob set off; the block list can be edited |
| `PlayerInteractEntityEvent` | the interact packet, before the use | cancel |

Costs: each site is one flag test (the watched fields one test of both bits). `as_guest`, `pvp_land` and
`note_killer` add one `running()` check, `tick_world` and the end of the mob pass one each. `damage_with` keeps
the damage before armor in a local. With a damage listener, each level tick records every mob's type by id,
so a mob that attacks from inside mob code gets its Bukkit type.

Entry points for other areas (`Entities`): `inWorld`, `inChunk`, `nearby`, `spawn`, `of`.

## Errors and crash isolation

| Failure | What happens |
|---|---|
| A listener throws | `SimplePluginManager` logs "Could not pass event X to Y", other listeners run. |
| A task throws | "Task #N for Plugin vX generated an exception", the scheduler goes on. |
| `onLoad`/`onEnable` throws | Logged; `JavaPluginLoader` disables the plugin. |
| Anything else escapes a bridge entry point | Each `Bridge`/`*Events` method catches `Throwable`, logs it and answers the default (event not cancelled, vanilla message). |
| A Java exception crosses JNI anyway | `bridge::call` logs it with its trace and clears it. |
| A Rust panic in a native | Caught (`natives::guard`) and thrown into Java as `RuntimeException`; never unwinds into the JVM. |
| A plugin with server internals | Loads with one warning; reaching a missing internal disables it with one line at the next tick, the others run on. |
| A plugin loops or deadlocks on the main thread | The tick stalls, as on Spigot. Not isolated. The watchdog (`Watchdog.java`, Spigot's as a warning) logs `The server thread has been stuck in X v1.0 for 10 s (tick N):` and the thread's Java stack once the server thread has sat in one call into Java for 10 s with no tick starting, then every 5 s; a stall in the server's own code (no Java stack) is not reported. |
| The JVM crashes (a plugin's native library, OOM) | The process goes down with it: the price of in-process. `-Xmx` through `FERRALOOM_JVM_OPTIONS` bounds the heap. |

Java's own `SIGSEGV` handling replaces the process's once the JVM starts; a Rust stack overflow then crashes
through the JVM's handler instead of Rust's message. Only with plugins.

## Performance budget and how it is checked

| Situation | Cost | Checked by |
|---|---|---|
| No plugin jar | Nothing loaded. Each event site: one relaxed load and one branch, never taken. `lend_hub`: one `OnceLock` check. Plugin command check: one `OnceLock` check. | e2e check 1 (`/proc/<pid>/maps` has no libjvm); the disassembly below |
| Plugins, no listener for an event | The same branch at that event's sites. | `Bridge.tick`'s bits |
| Plugins, per tick | One JNI call (heartbeat + bits) and the queue drain; one more when command labels change. | |
| A command line | A lookup in a label set before any JNI; vanilla commands never reach the JVM. | |
| An event with listeners | One JNI call with its arguments (a few strings), the listeners' own time. | |
| Join/quit with plugins | One JNI call each, listener or not (the player objects). | |
| Plugin boss bars | None shown: one relaxed load per viewer in the tick's boss bar update. With bars: a lock and a scan per viewer, the titles converted per viewer as custom bars' are. | `boss_bars_for` |
| A command tree sent, plugins with commands | One JNI call (the labels the player may not use, `Command.testPermissionSilent`); none without plugin commands. | `Hub::extra_commands_for` |
| A completion request | Vanilla commands: one `OnceLock` check. A plugin command's arguments: one JNI call, the TabCompleter's time. | `plugin_suggestions` |
| Player lists, offline players | No tick cost: answered when asked, on any thread, under the lists' locks. `getOfflinePlayer(name)` in online mode may block on the profile service for a name the user cache lacks, as CraftBukkit's does. | |
| Watchdog | Plugins only: a volatile write per tick, a wake-up per second; the stack is read only when no tick started since the last wake-up. | |
| A log record with a throwable | One cause-chain walk for a missing server internal (`Internals.reached`); records without a throwable, none. | |
| Logins with plugins | `AsyncPlayerPreLoginEvent` on the login's thread, then `PlayerLoginEvent` handed to the server thread (the login waits for it; served each loop, paused or not): two JNI calls per login, listener or not. Without plugins one `OnceLock` check. | |
| `PlayerMoveEvent` and the toggles, no listener | One test of a constant mask (`listens_any`) per level pass per tick, in `game/lan.rs`; packet handling is untouched. | |
| `PlayerMoveEvent` with a listener | Each tick, per guest of the level: a walk over its queued packets (a few) before the hub handles them; per move, a squared distance and an angle against its last event (a thread-local map). Past CraftBukkit's thresholds (1/256 of a block squared, 10 degrees), which a walking player passes every tick: one JNI call with fifteen primitive arguments (no Rust allocation), in Java a `PlayerMoveEvent`, two `Location`s and a `UUID`; null answers an unchanged move. About a microsecond plus the listeners per moving player. | |
| A player getter (`getHealth`...) | One JNI call answering the whole state as a `double[41]`. Off the server thread the tick's snapshot answers: the first such read turns on storing one per player at each tick's start (one struct per player), never before. | |
| A container click, drag, open or close with a listener | One JNI call with ints and three short strings; no item is converted unless a plugin asks the event for one. | |
| A hopper transfer with an `InventoryMoveItemEvent` listener | One JNI call, the one item's NBT to Java and back, two inventory objects and the event. Without a listener: the flag test. | |
| An item stack sent to a client | One emptiness check of `StackData::other` more than before (`lan/item_extras.rs`); a stack with unmodelled components clones each one sent. | |

The event bit is a plain static and `listens` is `#[inline(always)]` over `FLAGS.load(Relaxed) & bit != 0`,
so the site compiles to the bit test and nothing else. `break_block_as_player` in the release build
(`--no-default-features`, x86-64), right after `canDestroyBlock`'s call:

```
mov    0x147de80(%rip),%rax   # GOT slot, relocated to ferraloom_plugins::events::FLAGS
mov    (%rax),%rax
test   $0x10000,%eax          # bit 16: BlockBreak
jne    <the plugin path>
```

(The address comes through the GOT because `FLAGS` is in another crate of a PIE; still no call.) The 500-player benchmark
(`tools/server_bench.py`) comparing a server with an empty `plugins/` folder, one with a listener-less plugin and
the baseline is the coordinator's next step; budget: no measurable MSPT difference for the first two.

## What works (the slice)

- Plugins load from `plugins/`; `plugin.yml` is read; `onLoad`, `onEnable`, `onDisable` in order; data
  folder, logger (lines in the server's log as `[Name] message`), `config.yml` with defaults from the jar.
- `Bukkit.getServer()`: name `Ferraloom`, version `<version>-Ferraloom (MC: 26.3)`, Bukkit version from the API
  jar, online players, worlds, `getPlayer`/`getPlayerExact`, `getPluginCommand`, `dispatchCommand` (plugin
  commands; vanilla ones from the console), `broadcastMessage`, services, messenger, `isPrimaryThread`,
  `shutdown`, the console sender.
- `plugin.yml` commands from players and the console, with permissions through `PermissibleBase` and
  operators from the ops list; they are in the command tree players receive (a literal and a greedy
  `args`). Bukkit's `/version` and `/plugins` work; its `/reload` and `/timings` are removed (vanilla
  `/reload` stays).
- `BukkitScheduler`, every method: sync tasks on the server thread with CraftScheduler's timing (due at
  `tick + delay`, never in the heartbeat that scheduled them), async ones on a pool, timers, cancel,
  `callSyncMethod`.
- Events with priorities and `ignoreCancelled` (Bukkit's own `SimplePluginManager`): `PlayerJoinEvent`
  (message replaced or removed), `PlayerQuitEvent` (same), `AsyncPlayerChatEvent` (format, message,
  recipients, cancel), `BlockBreakEvent` (cancel).
- `Player`: `sendMessage` (and `spigot().sendMessage` components), `getName`, `getDisplayName`,
  `getUniqueId`, `getLocation`, `getWorld`, `teleport` (across worlds too), `isOnline`, `isOp`, permissions.
- `World`: name, UID, environment, key, heights, game time and time of day, players, `getBlockAt`.
  `Block`: position, world, location, type (the loaded level in place).
- The server area: `getOfflinePlayer(String/UUID)` (online, else in online mode the user cache or profile
  service, else the offline UUID, CraftServer's order), `getOfflinePlayers` (the player files), `OfflinePlayer`
  (op, whitelist, ban, played before, first and last played from the player file), `getOperators`, the
  whitelist (`hasWhitelist`, enforced, `getWhitelistedPlayers`, `reloadWhitelist`), `getBanList` (profile and
  IP, `BanEntry` save and remove), `banIP`, `PlayerProfile` (no textures), `matchPlayer`, boss bars
  (`createBossBar`; keyed ones not yet), the help map (a topic per command), the command tree filtered by each
  player's permissions, tab completion of plugin commands (`TabCompleter`, `TabCompleteEvent`), server.properties
  getters (view distance, allow flight, world type, idle timeout...), spawn limits, `spigot().getConfig()`
  (defaults), `UnsafeValues.getDataVersion`. List changes are saved at once; an operator's new level and
  commands reach them at the next tick; `/op` and `/deop` recalculate online players' Bukkit permissions first,
  as CraftBukkit's sendPlayerPermissionLevel does. `Player`'s op, ban and whitelist methods answer from the
  same lists (`FerraloomLists`). `dispatchCommand` runs a vanilla command as a player too. A full server is
  decided with the bans and the whitelist (PlayerList.canPlayerLogin's order), so `PlayerLoginEvent` sees
  `KICK_FULL` and a plugin may let the player in, as on CraftBukkit.

- Items (inventory area): `ItemStack` to and from the server as one `byte[]` (`ItemStack.CODEC` in network NBT;
  `src/inventory.rs` `to_java`/`from_java`, Java `Items.read`/`write`). The meta is the item's component patch
  itself, so what neither side models survives a round trip; names and lore as CraftChatMessage converts them,
  plugin data in `custom_data`'s `PublicBukkitValues`, item flags as `tooltip_display`; skull, potion, leather,
  book and enchanted book metas; `ItemType` from the server's item table. Inventories: `PlayerInventory`,
  ender chests, block containers, `Server.createInventory` (a plugin's container lives in the server as
  `Slots::Custom`), `openInventory`, `closeInventory`, `updateInventory`, the cursor and held slot; a slot's
  item writes through to the slot as CraftItemStack's mirror does. Events: `InventoryClickEvent` (CraftBukkit's
  click types and actions; a cancelled click is corrected on the client), `InventoryDragEvent`,
  `InventoryOpenEvent`, `InventoryCloseEvent`, `InventoryMoveItemEvent` (hoppers), `PlayerItemConsumeEvent`,
  `PlayerSwapHandItemsEvent`. Menus the game runs itself (crafting table, anvil, trading, stations) fire no
  click events yet.

Blocks and worlds (registries, `BlockData`, blocks and worlds in every dimension, block and world events,
hook costs): [plugins-blocks-worlds.md](plugins-blocks-worlds.md).

**Players** (`src/players.rs`, `game/plugins/players.rs`): `Player`, `HumanEntity` (but its inventory) and
`OfflinePlayer` for online players: health, maximum health, absorption, food, saturation, exhaustion,
experience and levels (`giveExp`, `giveExpLevels`), game mode, flying (when allowed), walk and
fly speed (read), allow-flight (set too; a game mode change ends it, as `updatePlayerAbilities` does), sneaking, sprinting, swimming, gliding, sleeping, dead, in a vehicle, velocity (read and
set), fire, air, fall distance, freeze ticks, pose, facing, eyes, size and bounding box, respawn point,
effects (`PotionEffect` add, read, remove), ping, locale, address, client view distance, `hasPlayedBefore`,
first and last played (the player file's times), display name, player list name, tab list header and
footer, titles, the action bar (`spigot().sendMessage(ACTION_BAR, ...)`), sounds (by name or `Sound`, any
category) and `stopSound`, particles without data, `sendBlockChange`, plugin messages both ways (the
client's `minecraft:register`, the server's supported channels at join), `kickPlayer`, `chat`,
`performCommand` (plugin commands, then vanilla at the player's level), `updateCommands`, metadata. Events:
`AsyncPlayerPreLoginEvent`, `PlayerLoginEvent` (the same player object as the join's), `PlayerKickEvent`
(from `kickPlayer`, `/kick` and idling), `PlayerMoveEvent` (cancel, a changed destination as a `PLUGIN`
teleport), `PlayerTeleportEvent` (`PLUGIN`, `COMMAND`, `ENDER_PEARL`), `PlayerInteractEvent` (both
clicks on blocks and in the air; `useInteractedBlock` and `useItemInHand` act separately),
`PlayerCommandPreprocessEvent` (rewrite, cancel), `PlayerRespawnEvent` (the place), `PlayerDeathEvent`
(message, dropped experience, keepInventory), `PlayerGameModeChangeEvent`, `PlayerToggleSneakEvent`,
`PlayerToggleSprintEvent`, `PlayerToggleFlightEvent`, `PlayerChangedWorldEvent`, `PlayerItemHeldEvent`.
`FerraloomMetadataStore` is the one store behind every `Metadatable` (`player(name)`, `entity(uuid)`,
`block(world, x, y, z)` name subjects).

Players' differences from CraftBukkit: `PlayerDeathEvent.getDrops` throws until the bridge's `ItemStack`
arrives, `getDamageSource` until the entities area's;
`PlayerDeathEvent`'s keepLevel and new experience are not applied; a `PlayerRespawnEvent` place is reached
as a teleport at the next tick's start; `PlayerMoveEvent` sees the position from the tick's start (the
packets of one tick are checked before the hub handles any); `PlayerTeleportEvent` from a pearl cannot
change where it lands, and portal, spectate and gateway teleports fire none yet; the player list name is
not sent to players who join later; `setWalkSpeed`, `setFlySpeed`, `hidePlayer` and
`showPlayer` (`canSee` answers true), `setPlayerTime`/`Weather` (their getters answer the world's) and
health scaling are not bridged; `PlayerLoginEvent.getHostname` is the address, not the handshake's host.

Known differences from CraftBukkit, each for its area to remove: join/quit messages are sent as plain
translations (no hover); `AsyncPlayerChatEvent`'s lines go out unsigned;
`PlayerChatEvent` (the deprecated sync one) is not fired; a player's command tree follows their permissions
when it is sent (join, op changes, `Player.updateCommands`); legacy plugins
(no `api-version`) load without legacy-material support. The tracked list of what is still open is
[`vanilla-client-gaps.md`](vanilla-client-gaps.md).

## Fan-out

Each area owns a Rust module in the crate, a module of `GameHost` glue, its Java classes and its event bits.
Shared files take one line per addition, in the area's own block, so parallel branches merge as appends.

| Area | Bukkit surface | Rust (crate) | Game glue | Java | Event bits | Upstream hooks it needs |
|---|---|---|---|---|---|---|
| **players** | `Player`, `HumanEntity` (non-inventory), `OfflinePlayer`, chat, join/quit/kick/login, move/teleport, game mode, health/food/exp, `PlayerInteractEvent` | `src/players.rs` | `game/plugins/players.rs` | `FerraloomPlayer`, `PlayerEvents`, `PlayerNative`, package `dev.ferraloom.bukkit` (later `.entity.player`) | 0-15 | `game/lan_actions.rs` (packet handlers: move, interact, game mode), `lan/hub.rs` (login, kick), `game/lan.rs` (`HostEvent`s) |
| **blocks** | `Block`, `BlockState`, `BlockData`, `Material` registry, block events (break, place, physics, redstone, burn, grow, piston) | `src/worlds.rs` → split `src/blocks.rs` | `game/plugins/worlds.rs` → `blocks.rs` | `FerraloomBlock`, `BlockEvents`, `WorldNative`, `.block` | 16-31 | `game.rs` (`break_block_as_player`, place), `horizon-world/src/level/` (hooks for physics, via a callback the game installs) |
| **worlds** | `World` (time, weather, spawn, chunks, `getBlockAt` everywhere, `spawnEntity`), `Chunk`, `WorldBorder`, world events | `src/worlds.rs` | `game/plugins/worlds.rs` | `FerraloomWorld`, `.world` | 32-39 | `game/levels.rs` (other dimensions' chunks through `Parked`), `game/world_loading.rs` |
| **entities** | `Entity`, `LivingEntity`, mobs, projectiles, damage and death, `EntityDamageEvent`, `CreatureSpawnEvent`, targeting | `src/entities.rs` (new), `EntityHost` incl. the restricted host for the mob pass | `game/plugins/entities.rs` | `.entity` | 40-47 | `game/creatures.rs`, `mobs/tick_loop.rs` (`note_impure` at sites, the serial-turn host), `game/combat.rs` (after the combat agent's branch merges) |
| **inventory** | `Inventory`, `ItemStack` (to and from Horizon's `ItemStack`), `ItemMeta`, `InventoryClickEvent`, `InventoryOpenEvent`, `Player.openInventory`, `ItemFactory` | `src/inventory.rs` (new) | `game/plugins/inventory.rs` | `.inventory` | 48-55 | `game/lan_actions.rs` (container clicks), `menu.rs`, `game/stations.rs` |
| **server** | `Server` remainder (ops, whitelist, bans, boss bars, recipes, `getOfflinePlayer`, `createInventory`), scoreboard, `ServicesManager` already done, `HelpMap`, tab completion (`TabCompleter` through `CommandSuggestions`), watchdog | `src/server.rs` | `game/plugins/server.rs` | `FerraloomServer`, `FerraloomConsole`, `.command`, `.scoreboard` | 56-62 | `game/dedicated.rs`, `lan/hub.rs` (suggestions), `game/scoreboard.rs` |

Rules for every area:

- **Adding an API method**: override it in the area's `Ferraloom*` class. If it needs the game, add a
  method to the area's host trait, a native in the area's `*Native.java` and its Rust function in the area's
  module (`natives::register` in its `load`), and implement the trait method in its glue file.
- **Adding an interface**: add it to `java/stubs.txt` (the stub is generated), a class extending the stub,
  and the class to `IMPLEMENTATIONS` in `build.py` for coverage.
- **Adding an event**: a bit in the area's range in `Event` (`src/events.rs`) and in `Bridge.EVENTS`
  (`Bridge.java`), an entry point in the area's `*Events.java` with its line in the area's `Entry` table, a
  Rust function calling it, and one line at the site in upstream code behind `listens`, with a
  `// Ferraloom:` comment, at the place CraftBukkit fires it.
- **A new area module**: its line in `bridge::AREAS` (`src/bridge.rs`), its host trait in `PluginHost`'s
  supertraits (`src/lib.rs`), its `mod` line in `game/plugins.rs`.
- **Tests**: unit tests next to the code; behaviour visible to players goes in `tools/plugins_e2e.py` (its
  `plugin_bot` can be taught more inputs), and gameplay sites keep passing the parity harnesses
  `tools/parity.py --changed-files` names, in single- and parallel-region modes.
- Shared lines (`AREAS`, `PluginHost`, the `mod` list, `Event`, `Bridge.EVENTS`, `stubs.txt`, `IMPLEMENTATIONS`)
  are appends in each area's block: rebase conflicts there are resolved by keeping both.

**Coverage measure.** `build.py` prints and writes `coverage.md`: per stubbed interface, the abstract methods
and how many the bridge class overrides. At the foundation (14% overall):

| Interface | Implemented | Methods | % |
|---|---:|---:|---:|
| `org.bukkit.Server` | 34 | 165 | 20.6 |
| `org.bukkit.entity.Player` | 33 | 479 | 6.9 |
| `org.bukkit.World` | 11 | 253 | 4.3 |
| `org.bukkit.block.Block` | 8 | 53 | 15.1 |
| `org.bukkit.command.ConsoleCommandSender` | 21 | 27 | 77.8 |
| `org.bukkit.scheduler.BukkitScheduler` | 34 | 34 | 100.0 |
| `org.bukkit.UnsafeValues` | 2 | 28 | 7.1 |
| **Total** | 143 | 1039 | 13.8 |

The coordinator tracks this table per area, plus events bridged (bits set in `Bridge.EVENTS`) against
Bukkit's event classes. A method overridden only to throw differently does not count as implemented: review
checks overrides.

## Licensing

Decided 2026-11-26: **everything Ferraloom writes stays MIT, the Java bridge included.** This is the
project's reading of the licenses, not legal advice.

- The Bukkit and Spigot API is GPL-3.0. The bridge (`crates/ferraloom-plugins/java/`) implements its
  interfaces and is compiled against it, which the FSF and Bukkit's history treat as a work based on the API.
  The GPL asks that such a work be conveyed under terms the GPL allows; it does not ask that our own files
  carry the GPL. MIT is GPL-compatible and permits exactly this: our files stay MIT, and a compiled bridge is
  conveyed under the GPL-3.0's terms with this repository as its complete corresponding source.
- The Rust server loads the bridge into its own process through JNI. Under the FSF's reading of in-process
  combination, a running server with the bridge and the API is one combined program, so every part must be
  GPL-compatible. It is: Ferraloom is MIT and every crate the server links is under a permissive or
  GPL-compatible license (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0, Unicode-3.0, BSL-1.0, CDLA-Permissive-2.0;
  checked from `cargo metadata` on 2026-11-26). Keep it that way: a dependency under a GPL-incompatible license
  (the old OpenSSL license, SSPL, BUSL, a proprietary SDK) may not enter the server's tree.
- Ferraloom contains no Mojang code, so the conflict that ended CraftBukkit's distribution (GPL code shipped
  in one binary with Mojang's proprietary server) does not arise.

Release rules that follow:

1. **The API jar is never redistributed.** `build.py` fetches the build pinned in `java/api.lock.json` from
   Spigot's own repository and checks its SHA-256. It is Spigot's work under the GPL, and it carries a Mojang
   translation file we have no right to ship.
2. **A server release does not bundle the bridge.** The server is a plain MIT binary that runs without a JVM;
   the bridge is a separate artifact (built by `build.py`, or a separate download) that the server finds at
   run time, and only when `plugins/` holds a jar.
3. **A published bridge jar is offered under the GPL-3.0's terms**, names the commit it was built from, and
   links this repository as its source. Its source files remain MIT.
4. Plugins are their authors' works under their own licenses. We ship none; the usage corpus and the jars used
   in tests stay out of the repository. The test plugins under `java/test-plugin` and `java/nms-plugin` are our
   own, MIT.
