# Bukkit plugins: blocks and worlds

The blocks and worlds areas of the plugin bridge ([plugins.md](plugins.md), "Fan-out"): what works,
where each piece lives, what each hook costs, and what CraftBukkit does that Ferraloom does not yet.

## Layout

| Piece | Rust (crate) | Game glue | Java |
|---|---|---|---|
| Registries (`Server.getRegistry`, every area's) | | | `FerraloomRegistries` |
| Blocks, `BlockData`, `Material`'s block side | `src/blocks.rs` (`BlockHost`, natives, events) | `game/plugins/blocks.rs` | `Blocks`, `FerraloomBlockData`, `FerraloomBlockType`, `FerraloomBlock`, `FerraloomBlockState`, `BlockNative`, `BlockEvents` |
| Worlds, chunks, the border | `src/worlds.rs` (`WorldHost`, natives, events) | `game/plugins/worlds.rs` | `FerraloomWorld`, `FerraloomChunk`, `FerraloomWorldBorder`, `FerraloomGameRule`, `WorldNative`, `WorldEvents` |
| Events from block behaviour | | `GameLevel::plugin_level_event` | |
| | `horizon-world`: `plugin_hooks.rs` (flag word, `PluginEvent`), `level/plugin_sites.rs`, one site per event | | |

## How it works

**Registries.** `Registry`'s static fields ask `Server.getRegistry` for about thirty types at once. Each
registry's entries are the keys its type's API constants ask for while the type initializes
(`Sound.ENTITY_PIG_AMBIENT = getSound("entity.pig.ambient")`), in field order; after that the list is
closed. A type gets real entries with one line in `FerraloomRegistries.FACTORIES`
(`FACTORIES.put(ItemType.class, FerraloomItemType::new)`); others get key-only proxies (interfaces) or
key-only subclasses of generated stubs (the six abstract classes). `BlockType` and `GameRule` are real.

**Blocks are state ids.** The server sends its block table once at start (`BlockHost::block_table`: each
block's states, properties in vanilla's order, a few facts of its default state). A `BlockData` is a
proxy over one global state id implementing the interface its type names (`BlockType.Typed<Stairs>
OAK_STAIRS` gives `Stairs`), so casts and `instanceof` behave; its accessors are the properties by name
(`getFacing` reads `facing`, `getMaximumAge` the largest `age`, `hasFace(NORTH)` reads `north`) and a
change is arithmetic on the id. Reading a block is one native call answering an int.

**Every dimension.** `World` and `Block` reach the level in place and the parked ones (`Game::level_world`,
`Levels::parked_world`): reads go to the level the tick thread owns, never a generation worker's copy, and
a chunk not yet integrated reads as unloaded (air). Writes to another dimension put its level in place
for the write (`GameHost::in_level`, as `execute in` does). A level that is not open reads as unloaded.

**Events from block behaviour.** `horizon-world` knows nothing of plugins. Each site is

```rust
if plugin_hooks::listens(Hook::Burn) && self.plugin_cancels(PluginEvent::Burn { pos, fire }) {
    return;
}
```

`listens` is `#[inline(always)]` over a relaxed load of a static `AtomicU32`; the game copies the
latched Bukkit bits into it after each latch (`sync_level_hooks`). `plugin_cancels` calls
`LevelAccess::plugin_event`, a default method (pass) that only the server's `GameLevel` overrides: static
dispatch, no function pointer. With a listener the event goes to Java with the level itself as the host:
plugins read its blocks during the event; their messages, teleports and commands wait for the next tick,
and block writes are refused (the level is mid-update).

## Costs

| Hook | No plugin / no listener | With a listener |
|---|---|---|
| Every `horizon-world` site (physics, from-to, burn, ignite, grow, fade, form, spread, leaves, pistons, redstone) | one relaxed load and a branch not taken; no call, no event built | one JNI call per event; physics and redstone fire per neighbour update, so a physics listener costs per block update |
| `BlockPlaceEvent` (`game.rs`, before `place_block`) | one bit test | the placement computed twice (once for the event) and one JNI call |
| `BlockBreakEvent` drops and experience | one bit test at each of the two sites | the experience computed before the event (CraftBukkit's order; its RNG draw moves) |
| `BlockExplodeEvent` | one bit test per explosion | one JNI call, for explosions no entity caused |
| `SignChangeEvent` | one bit test per sign edit | the old lines copied, one JNI call |
| Weather (`WeatherChangeEvent`, `ThunderChangeEvent`) | two bit tests per tick | the weather copied each tick, one JNI call per change |
| `WorldSaveEvent` | one bit test per level save | one JNI call |
| `ChunkUnloadEvent` | one bit test per streaming update | one JNI call per unloaded chunk |
| `ChunkLoadEvent` | one bit test per streaming update | the loaded chunk keys copied before each update and compared after (only when membership changed): O(loaded chunks) per update while a plugin listens |
| `WorldInitEvent`, `WorldLoadEvent` | nothing at run time (fired once at start) | |
| Block table | read once, at start, only with plugins | |

## Known differences from CraftBukkit

- Chunks load asynchronously: `getChunkAt`, `loadChunk` and plugin chunk tickets ask for the chunk and it
  loads over the next ticks; a block in a chunk not loaded reads as air (CraftBukkit loads it on the
  spot). Plugin tickets are forced chunks while held; `isChunkGenerated` reads the region file's header.
- A behaviour event's host refuses block writes; `BlockGrowEvent`'s (and fade, form, spread) new state is
  vanilla's: a plugin changing the event's `BlockState` is not applied.
- `BlockPlaceEvent` fires before the block is placed, with the item in hand as a plain `ItemStack` of the
  placed block's item (no components until the inventory area's conversion lands); a cancelled place
  resends the blocks and the inventory. Multi-block places (beds, doors) fire one event.
- `BlockBreakEvent.setDropItems(false)` also keeps the experience back (CraftBukkit still drops it).
- `BlockExplodeEvent` fires only for `World.createExplosion`; its yield is not applied.
- `BlockPhysicsEvent`'s source block is the updated block itself (Horizon's neighbour updater does not
  carry the source position).
- `World.getSeaLevel` answers 63 (32 in the Nether) whatever the generator; the spawn, time, weather, game
  rules and border are the server's, shared by every world, as vanilla's derived levels share them.
- `strikeLightning` returns null until the entities area supplies entity handles.
- Block entity states implement their interface (`Chest`, `Furnace`...) but only signs answer their own
  methods; the rest throw until the inventory area lands.
