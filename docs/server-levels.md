# A server's levels: one per dimension

Vanilla's `MinecraftServer` holds a `ServerLevel` per dimension, and each player is in one of them.
Horizon's server does the same, so each player of the dedicated server can be in a different dimension.

## How the single-level `Game` maps onto vanilla's levels

The game's tick code reads one level: the one held in `Game`'s own fields (the world, its streaming
and save, block ticks, entities, block entities and their per-level queues). `game/levels.rs` keeps
the other dimensions' levels as `Parked` values and swaps one into place (`enter_level`). The swap is
`mem::swap` of each per-level field, so the tick code runs unchanged, one level at a time.

| Vanilla | Horizon |
| --- | --- |
| `MinecraftServer.levels` | the level in place plus `Levels::parked` |
| `MinecraftServer.tickChildren`, every level | `tick_server`: each level with a player in it, in Overworld, Nether, End order. The first pass reads the network and runs the shared parts (clock, weather, functions, autosave). |
| `ServerLevel.players` | `Hub::here`: the guests whose `dimension` is the level in place |
| `ServerPlayer.teleport` into another level | `server_travel.rs`: a Respawn packet, then the teleport once the destination has loaded (portals and `execute in … tp`). An End gateway teleports within the End. |
| `PlayerList.respawn` | `guest_respawn`: into the respawn point's level, the Overworld unless the player has an anchor or a `spawnpoint` elsewhere (`Guest::respawn_elsewhere`), resolved through its block once that level has loaded |
| `ServerLevel.noPlayersEmptyTime`, chunk unloading | `close_idle_levels`: a level other than the Overworld with nobody in it for 300 ticks is saved and closed |
| `CommandSourceStack.getLevel` | `with_source` puts the source's level in place for a command (`execute in`, `execute at` a player in another level) |
| the levels' shared `LevelData` | not swapped: game time, day time, weather, game rules, scoreboard, maps, entity ids (`next_entity`, unique across levels) |

The packets of a guest are read in its own level's pass (`Hub::tick_level`), so its actions and
commands run with its level in place.

## The execution role

Each `Game` has an explicit `ExecutionRole` (`game/execution_role.rs`), fixed at construction:
`DedicatedServer` for `--dedicated`, `IntegratedServer` for the component fixtures' server `Game`
(`game/integrated.rs`). Startup, world loading and opening other dimensions use it.
`server_dimension_players` checks the levels with vanilla-codec clients against vanilla's dedicated server.

## Cost of an extra level

Measured on the Mac mini (M4 Pro) at render distance 12, on 2026-10-22, before the client was removed: the host
and a guest each played a fresh world on single player's server thread. RSS is the host's whole process, window
included. Tick time
is `/tick query`'s average over 100 ticks.

| Where the players are | Average tick | P95 | Host RSS |
| --- | --- | --- | --- |
| both in the Overworld | 1.0 ms | 1.3 ms | 1194 MB |
| guest in the Nether (two levels tick) | 2.2 ms | 2.9 ms | 1238 MB |
| guest in the End (the Nether still parked) | 0.9 ms | 1.1 ms | 1324 MB |
| both back, the Nether and End closed after 300 idle ticks | 0.8 ms | 0.9 ms | 1335 MB |

- A level costs only while a player is in it: its chunks around that player, their entities, and the
  tick. With nobody outside the Overworld, `tick_server` runs one pass, as before.
- A closed level's memory goes back to the allocator, not always to the system.
- Entering a level the first time opens its region files and streaming (`open_level`). That cost
  comes once, before the player's chunks load.

## Saving

`save_game` saves the level in place, then every parked level (`save_parked_levels`). Each level has
its own region folder, as in vanilla (`DIM-1`, `DIM1`). A level closed for being idle is saved first.
Each player's saved data holds its dimension (`Dimension`). A guest who leaves in
the Nether, or whose host saves and quits, joins again in the Nether where it was.

## What's checked

`python3 tools/parity.py server_dimension_players`: vanilla-codec players on Ferraloom's dedicated server and
vanilla's, each in a level of its own, through portals and `execute in … tp`. The broader LAN checks that used
to cover portals, respawn anchors and rejoining in the Nether drove Horizon's own client and were removed with it.

## Not done yet

- **End credits for server players.** The first exit from the End should show a player the credits
  (`EndPortalBlock.entityInside`, `seenCredits`).
- **Entities through portals between levels.** Mobs, items and minecarts that enter a portal on a
  server, and portal tickets (`TeleportTransition.PLACE_PORTAL_TICKET`), are not handled.
- **Unlimited selectors.** `@e` without a distance or volume reads only the source's level. Vanilla's
  reads every level.
