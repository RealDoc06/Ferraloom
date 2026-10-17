# Architecture

Ferraloom ports the vanilla 26.3 dedicated server to Rust. Vanilla behavior is
the spec; multi-core execution is the reason the project exists. Every design
choice below serves one of those two goals.

## Crates

Leaf crates first, so independent work does not collide and the builder can
compile in parallel.

| Crate | Ports | Depends on |
|---|---|---|
| `ferraloom-util` | `Mth`, `BlockPos`/`Vec3`/`AABB`/`Direction`, `RandomSource` impls (`LegacyRandomSource`, `XoroshiroRandomSource`, positional factories), noise primitives | none |
| `ferraloom-nbt` | NBT binary, SNBT, `NbtOps`-style codec helpers | none |
| `ferraloom-data` | Generated registries: blocks and block states, items, entity types, sounds, particles, menus, packet ids, built-in tags. Loader for the vanilla datapack (`data/minecraft/**`) | util, nbt |
| `ferraloom-protocol` | Every 26.3 packet in every state, both directions, plus text components and item data component codecs | util, nbt, data |
| `ferraloom-worldgen` | Noise router, density functions, biome source, surface rules, aquifers, carvers, features, structures | util, nbt, data |
| `ferraloom-storage` | Anvil region files, chunk and entity serialization, `level.dat`, player data, data fixing of older saves | util, nbt, data |
| `ferraloom-brigadier` | Mojang's Brigadier (generic command dispatcher) | none |
| `ferraloom-game` | The simulation: levels, chunks, block behaviors, fluids, redstone, entities and AI, items, menus, recipes, loot, advancements, commands, scoreboard, the GameTest framework | all of the above |
| `ferraloom-server` | Binary: `server.properties`, networking, login and auth, the tick scheduler | all of the above |
| `ferraloom-parity` | Headless bot plus scenario runner. Drives vanilla and Ferraloom with identical scenarios and compares observations | protocol, data, nbt |

`ferraloom-game` is large because vanilla's `net.minecraft.world` is one
tightly coupled graph. Split it into modules that mirror vanilla packages
(`level`, `block`, `entity`, `item`, `inventory`, `crafting`, `loot`,
`commands`, ...), not into crates with artificial trait boundaries.

## Data

Two sources, both from the official 26.3 server jar:

1. **Data generator reports** (`java -DbundlerMainClass=net.minecraft.data.Main
   -jar server.jar --reports`): block states, registries, packets, commands.
2. **`tools/extractor`**, a small Java program run against the unobfuscated jar
   that bootstraps vanilla registries and dumps what reports lack: block
   properties (hardness, friction, sounds, shapes), item properties, entity
   dimensions and attributes, and so on.

Their JSON output is committed under `data/generated/` and turned into Rust by
`ferraloom-data`'s build script. Builds never need Java. The vanilla datapack
(recipes, loot tables, worldgen JSON, tags, advancements) is read at runtime
from the official jar, which the server downloads and verifies by SHA-1 on first
start, the way Paper does. Mojang assets are never committed.

## Game model

- **Block states** are `u16` global ids with generated typed property accessors.
  Block behavior is a `BlockBehavior` trait with vanilla's virtual methods
  (`on_place`, `neighbor_changed`, `tick`, `random_tick`, `use_item_on`,
  `get_shape`, ...). Each block class implements it once and a static table
  maps block id to behavior.
- **Block entities** implement a `BlockEntity` trait and are stored per chunk.
- **Entities** keep vanilla's hierarchy (Entity, LivingEntity, Mob,
  PathfinderMob, Animal, ...) as embedded parent structs plus one `Entity`
  trait with default methods. That makes porting a subclass mechanical. Entities
  refer to each other by `EntityRef` handles, never by pointers.
- **Ticking order inside a region is exactly vanilla's** (`ServerLevel#tick`,
  `ServerChunkCache#tick`, block events, scheduled ticks, entity ticking order).

## Threading

Vanilla runs the whole simulation on one thread. Ferraloom runs it on many
without changing what any contraption, farm or mob observes.

- **Regions.** Each dimension is partitioned into regions of loaded chunks.
  Regions that are far enough apart cannot affect each other within one tick,
  so they tick in parallel on a worker pool. A region's own tick is plain,
  single-threaded vanilla code with `&mut` access to its chunks and entities,
  so there are no locks inside game logic.
- **Merging.** Regions merge whenever their contents come within a
  safety margin (larger than any per-tick interaction radius: entity movement,
  explosions, redstone update reach, pathfinding range). They split again when
  their contents separate.
- **Cross-region work** (an entity crossing a boundary, a teleport, a command
  targeting another region's entities) goes through messages applied at the
  start of the receiving region's next tick. Global state (time, weather,
  scoreboard, command storage, game rules) lives in a global phase that runs
  between region ticks.
- **Single-region mode** (`--regions off`) ticks each dimension as one region,
  which gives vanilla order exactly. Parity tests run in both modes.
- **Off-tick parallelism.** Chunk generation, lighting, chunk IO, packet
  encoding, compression, encryption and pathfinding run on their own pools
  regardless of mode.

Build regions-shaped ownership from the start, even while only one region per
dimension exists. Retrofitting ownership later is the expensive path.

## Testing

- **Unit tests** in each crate, for example ported RNG sequences, NBT round
  trips and packet encodings checked against bytes captured from vanilla.
- **Golden worldgen tests.** Vanilla generates chunks for fixed seeds and
  Ferraloom must produce identical blocks, biomes and heightmaps.
- **Parity scenarios** in `ferraloom-parity`. A bot joins vanilla, runs a
  scenario (commands, movement, interactions), records what it observes and
  what the saved world contains, then does the same against Ferraloom. The two
  recordings must match. Each scenario first proves it passes on vanilla.
- **GameTest.** Vanilla's `/test` framework is itself a feature to port, and
  once ported its block-based tests run on both servers.

`FEATURES.md` ties these together: one row per vanilla feature, with vanilla and
Ferraloom test status.

## Phases

1. **Foundation**: util, nbt, data, protocol, storage, brigadier, decompiled
   source, the feature inventory.
2. **Playable core**: login (online mode, encryption, compression),
   configuration, join, chunk loading and sending, movement, block placing and
   breaking, inventory, chat, commands. The parity bot comes up here.
3. **World**: exact worldgen for all three dimensions, lighting, saving and
   loading vanilla worlds.
4. **Gameplay**: every block, item, entity, mob AI, redstone, fluids, crafting,
   loot, advancements, villages and raids, every command. Work through
   `FEATURES.md` until every row passes on both servers.
5. **Regions**: turn on parallel region ticking and measure it against vanilla.
6. **Plugins**: Java `.jar` plugin compatibility, only after phase 4 is done.
