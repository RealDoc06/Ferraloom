# Vanilla client gaps

What a vanilla 26.3 client sees differently on Ferraloom's dedicated server than on Mojang's. Audited on
`ferraloom/next` at `9981c0b55` (2026-11-25), read-only.

## Method and evidence

- **Serverbound.** Every `handle*` of `ServerGamePacketListenerImpl`, `ServerCommonPacketListenerImpl` and
  `ServerConfigurationPacketListenerImpl` was matched to its path: `lan/conn.rs` (`Player::receive`), the
  hub's `process_inbox` (`lan/hub.rs:2451`), then `Game::lan_guest_packet` (`game/lan_actions.rs:1331`),
  whose last arm `_ => {}` (`lan_actions.rs:1836`) drops whatever is left. Ids with no `Serverbound` variant
  decode as `Unknown` and fall through to that arm too.
- **Clientbound.** Every id in `horizon-protocol/src/ids.rs` was searched for construction (`C::Name(`,
  `raw(ids::NAME`) in the server code, skipping the client's `remote*.rs` and `net.rs`. 23 ids are never sent.
- **Entity data.** The player's synced data (`hub.rs:1876 player_state`, `hub.rs:2217 sync_player_state`)
  was checked index by index against `Entity`, `LivingEntity`, `Avatar` and `Player`'s `defineId` list.
- **server.properties.** Each field of `horizon-server-files/src/properties.rs` was searched for a reader.
- **Confirmed** below means both sides were read: the vanilla trigger in the decompiled 26.3 server and the
  missing or different Ferraloom path. **Inferred** means the client-side effect comes from knowledge of the
  vanilla client, since only the server is decompiled. No harness was run (see the end).
- Combat gaps are listed but owned by **fix-combat**, and were not investigated further.

## Ranked gaps

Size: S under a day, M a few days, L a week or more. "Harness" is the existing one to extend.

| # | Gap | What the player sees | Vanilla source | Ferraloom location | Evidence | Size | Harness |
|---|---|---|---|---|---|---|---|
| 1 | Server admin commands don't run in game | An op typing `/op`, `/deop`, `/ban`, `/ban-ip`, `/pardon`, `/banlist`, `/whitelist`, `/stop`, `/save-all`, `/save-on`, `/save-off`, `/transfer`, `/perf` or `/setidletimeout` gets "Unknown or incomplete command", though the command tree sent to the client offers them | `Commands#<init>` (`CommandSelection.DEDICATED` registers `OpCommand`, `BanPlayerCommands`, `StopCommand`, `TransferCommand`...) | Tree sends them: `lan/commands.rs:24` `DEDICATED_ONLY`. Executor lacks them: `horizon-command/src/vanilla.rs:46` `registered`. Guests reach only that: `lan_actions.rs:2140` `guest_command` → `run_command`. Only the console runs them: `game/dedicated.rs:892` | Confirmed (reading) | M | `test_server_ops_feedback_parity.py` (players are ops already) |
| 2 | Hurt players don't flash for others | No red tint, hurt tilt or walk-animation kick on another player taking any damage (fall, fire, mobs, PvP) | `LivingEntity#hurtServer` → `ServerLevel#broadcastDamageEvent` (`sendToTrackingPlayersAndSelf`) | `damage_event` only for mobs: `game/lan.rs:2085`. The hurt guest gets `HurtAnimation` only: `lan_actions.rs:1011` | Confirmed (reading). Client effect inferred | S | `test_server_player_combat_parity.py`, `KnockbackClient` (two clients). Coordinate with fix-combat |
| 3 | Player-stat scoreboard criteria never update for remote players | Objectives on `health`, `food`, `air`, `armor`, `xp`, `level` (health under names, in the tab list) stay empty or stale for every server player | `ServerPlayer#doTick` (`updateScoreForCriteria`) | `game/scoreboard.rs:359` `tick_scoreboard` reads only `self.player` as `PLAYER_NAME`, which on a dedicated server is nobody. `air` is hardcoded to 300 even for the host | Confirmed (reading) | S | `test_server_ops_feedback_parity.py` (its `LocatorBarClient` records `SetScore`) |
| 4 | Spectators are sent to non-spectators | Survival players see spectating players (as floating heads) and learn where they are. Vanilla never pairs them | `ServerPlayer#broadcastToPlayer` | `lan/hub.rs:2027` `track_entities` adds every guest's state with no per-viewer game-mode filter. `lan/entity.rs` has none either | Confirmed (server). Client render inferred | S | `test_server_entities_parity.py` (EntityTranscript pairing) |
| 5 | Spectators can't spectate entities | Left-clicking a mob in spectator mode does nothing, and `/spectate <target>` on a remote player says it succeeded but the camera doesn't move | `handleSpectatorAction`, `ServerPlayer#setCamera` (sends `ClientboundSetCameraPacket`), `SpectateCommand` | No `SpectatorAction` variant (falls to `lan_actions.rs:1836`). `SET_CAMERA` never sent. `commands/extra.rs:159` `set_camera` moves only the local `Game` camera | Confirmed (reading) | M | `lan_e2e.py` (add a check) |
| 6 | Held-item attribute modifiers not applied or synced (attack speed) | No attack cooldown indicator, and sword or axe timing feels instant. This is Doc's example | `LivingEntity#collectEquipmentChanges` → `UpdateAttributes` | No equipment modifiers in `game/lan_attributes.rs`, `lan/attributes.rs` or `player_on_equip` | Inferred | M | **Owned by fix-combat** |
| 7 | No floating kick (`allow-flight` ignored) | Survival players with a fly hack never get "Flying is not enabled on this server". Vehicles too | `ServerGamePacketListenerImpl#tickPlayer` (`aboveGroundTickCount`, `getMaximumFlyingTicks`) | `properties.rs:29` `allow_flight` has no reader. No floating counter in `hub.rs:2659` `position_change` | Confirmed (reading) | S | `test_dedicated_server_parity.py` |
| 8 | Freezing never synced | In powder snow: no frost vignette on your own screen, and other players don't shake or turn blue. The damage still happens | `LivingEntity#aiStep` (freezing, server-only) → `Entity.DATA_TICKS_FROZEN` (7) | Server simulates `ticks_frozen` (`motion.rs:364`) but neither `player_state` nor `sync_player_state` sends index 7. Mobs do: `game/lan.rs:935` | Confirmed (reading) | S | `test_server_entities_parity.py` |
| 9 | Riding jump ignored | Horses and llamas don't rear or play their jump sound, and a camel's dash has no sound or dash pose for anyone. The jump itself works, since the client drives it | `handlePlayerCommand` `START/STOP_RIDING_JUMP` → `AbstractHorse#handleStartJump`, `Camel#handleStartJump`, `AbstractNautilus` | `lan/hub.rs:2571`: actions 3 and 4 hit `_ => {}`. The host-only version is `game/mounts.rs:275` | Confirmed (reading) | S | `lan_e2e.py` |
| 10 | No chat or command spam kick | Spammers are never kicked with "disconnect.spam". `chat-spam-threshold-seconds` and `command-spam-threshold-seconds` are ignored | `#detectRateSpam`, `chatSpamThrottler`, `commandSpamThrottler` | `properties.rs:63-64` fields with no reader. Nothing in `lan/hub/chat.rs:185` `handle_chat`. Only the creative-drop throttler exists (`lan_actions.rs:1580`) | Confirmed (reading) | S | `test_secure_chat_parity.py` |
| 11 | Stuck arrows and bee stingers never shown | No arrows sticking out of players or mobs after hits, and no stingers | `AbstractArrow#onHitEntity` → `LivingEntity#setArrowCount`, `Bee#doHurtTarget` → `setStingerCount`, `DATA_ARROW_COUNT_ID` (12), `DATA_STINGER_COUNT_ID` (13) | No arrow or stinger count anywhere (`projectiles.rs`, `arrows.rs`, `mobs/bee.rs`). Indices 12 and 13 never sent for living entities | Confirmed (reading) | M | `test_server_entities_parity.py` |
| 12 | Shoulder parrots invisible | A tamed parrot that lands on a player vanishes from the world and shows on nobody's shoulder, the player's own included | `Player.DATA_SHOULDER_PARROT_LEFT/RIGHT` (19, 20), `ServerPlayer#setEntityOnShoulder` | `game/shoulders.rs:19` stores it in `player_extra`. Indices 19 and 20 never sent (`hub.rs:1876`) | Confirmed (reading) | S | `test_server_entities_parity.py` |
| 13 | `/dialog` doesn't reach remote players | `/dialog show @a ...` reports success but no dialog opens, and `/dialog clear` does nothing | `ServerPlayer#openDialog` (`ClientboundShowDialogPacket`), `DialogCommand` | `commands/extra.rs:426` `cmd_dialog` opens it on the server's own `Game`. `SHOW_DIALOG` and `CLEAR_DIALOG` never sent | Confirmed (reading) | S | `server_e2e.py --check dialog` (reverse it: vanilla client, Ferraloom server) |
| 14 | `/fillbiome` invisible until chunks reload | Grass, foliage and water colours and fog stay as before | `ChunkMap#resendBiomesForChunks` → `ClientboundChunksBiomesPacket` | `commands/terrain_cmds.rs:229` marks only local mesh sections dirty. `CHUNKS_BIOMES` never sent | Confirmed (reading) | S | `test_dedicated_server_parity.py` |
| 15 | No reach or sanity checks on block use and digging | A modified client can place, use and mine blocks at any distance. The `block_interaction_range` attribute is ignored (a fixed 4.5 or 5) | `handleUseItemOn` (`isWithinBlockInteractionRange(pos, 1.0)`, hit within 1.0000001 of the block), `ServerPlayerGameMode#handleBlockBreakAction` | `lan_actions.rs:1473` builds a `forced_hit` that `game.rs:2237` `pick` trusts. `lan_actions.rs:1348` has no range check. Reach is hardcoded at `game.rs:2170` | Confirmed (reading) | S | `test_dedicated_server_parity.py` |
| 16 | No build-limit message | Placing above Y 319 or below the floor fails silently instead of showing "Height limit for building is 320" | `handleUseItemOn` → `ServerPlayer#sendBuildLimitMessage` | No `build.tooHigh` or `build.tooLow` anywhere | Confirmed (reading) | S | `test_dedicated_server_parity.py` |
| 17 | Code of conduct never shown | `enable-code-of-conduct=true` and its texts do nothing, so players never get the screen to accept | `ServerCodeOfConductConfigurationTask`, `DedicatedServer#readCodeOfConducts` | `properties.rs:31` with no reader. The configuration phase (`lan/conn.rs:1020`) sends no `CodeOfConduct` | Confirmed (reading) | S | `test_dedicated_server_parity.py` (login and config sequence) |
| 18 | Death-screen score always 0 | "Score: 0" on the death screen whatever XP was earned | `Player#increaseScore` (from `giveExperiencePoints`), `DATA_SCORE_ID` (18) | Index 18 never sent | Confirmed (server). Client screen inferred | S | `test_dedicated_server_parity.py` (death sequence) |
| 19 | "Allow Server Listings" off is ignored | Your name still shows in the server list hover. Vanilla shows "Anonymous Player" | `MinecraftServer#buildPlayerStatus` (`allowsListing`) | `lan/mod.rs:316` `status_json` samples every online name | Confirmed (reading) | S | `test_dedicated_server_parity.py` (status) |
| 20 | Recipe-book highlights never clear | Recipes keep their "new" highlight on every join | `handleRecipeBookSeenRecipePacket` → `ServerRecipeBook#removeHighlight` | No variant, so it is dropped. `toBeDisplayed` (`src/recipe_book.rs:772`) is saved with the highlights still set | Confirmed (reading) | S | `test_dedicated_server_parity.py` |
| 21 | Structure, jigsaw and test blocks and command-block minecarts can't be edited | Save, Load, Detect and Done in those screens change nothing | `handleSetStructureBlock`, `handleSetJigsawBlock`, `handleJigsawGenerate`, `handleSetTestBlock`, `handleTestInstanceBlockAction`, `handleSetCommandMinecart` | No variants. Dropped at `lan_actions.rs:1836`. `TEST_INSTANCE_BLOCK_STATUS` never sent | Confirmed (reading) | M | `lan_e2e.py` |
| 22 | F3+I copies no NBT | With cheats, F3+I on an entity or block entity copies without its data | `handleEntityTagQuery`, `handleBlockEntityTagQuery` → `ClientboundTagQueryPacket` | No variants. `TAG_QUERY` never sent | Confirmed (reading) | S | `test_dedicated_server_parity.py` |
| 23 | F3 ping chart empty | The debug network and ping chart never fills | `handlePingRequest` (play) | Play `PingRequest` reaches `lan_actions.rs:1836` unanswered. Only the status phase answers (`conn.rs:714`) | Confirmed (reading) | S | `test_dedicated_server_parity.py` |
| 24 | Advancements screen forgets its tab | Reopening advancements doesn't return to the last tab | `handleSeenAdvancements` → `PlayerAdvancements#setSelectedTab` → `ClientboundSelectAdvancementsTabPacket` | No variant. `SELECT_ADVANCEMENTS_TAB` never sent | Confirmed (reading) | S | `test_dedicated_server_parity.py` |
| 25 | Hat layer in the tab list | The hat layer always shows in the tab list, and toggling it mid-game isn't seen | `handleClientInformation` (`UPDATE_HAT` broadcast) | `hub.rs:896` `show_hat: Some(true)`. `hub.rs:2551` never broadcasts `UPDATE_HAT` | Confirmed (reading) | S | `test_dedicated_server_parity.py` |
| 26 | Entity-bound sounds play at a point | Sounds vanilla binds to a moving entity (`Level#playSound(Player, Entity, ...)`, for example a trident's riptide or a firework) stay where they started | `ServerLevel#playSeededSound` (entity form) → `ClientboundSoundEntityPacket` | Every sound goes through `lan/containers.rs:52` as positional `Sound`. `SOUND_ENTITY` never sent | Confirmed (server). Audible difference inferred | S | `test_server_entities_parity.py` |
| 27 | Entity teleports interpolate | Endermen, shulkers and `/tp`'d mobs glide instead of snapping. A controlled vehicle that is teleported may be pulled back by the rider's client | `Entity#sendTeleportTransitionToPlayers` → `ClientboundTeleportEntityPacket` | `TELEPORT_ENTITY` never sent. Moves go out as `EntityPositionSync` and relative moves (`lan/entity.rs`) | Confirmed (server). Client motion inferred | M | `test_server_entities_parity.py` |
| 28 | `/rotate` and `facing` teleport the player | `/rotate` and `tp ... facing` on a player send a full position teleport, so the client snaps position and loses velocity | `ServerPlayer#forceSetRotation` (`ClientboundPlayerRotationPacket`), `ServerPlayer#lookAt` (`ClientboundPlayerLookAtPacket`) | `commands/world_cmds.rs:210` → `teleport_to`. `PLAYER_ROTATION` and `PLAYER_LOOK_AT` never sent | Confirmed (server). Client effect inferred | S | `test_dedicated_server_parity.py` |
| 29 | Landing falling blocks flicker | A landing sand or anvil can vanish for a frame before its block update shows | `FallingBlockEntity#tick` → `ClientboundAddTransientBlockPacket` | `ADD_TRANSIENT_BLOCK` never sent (not even decoded) | Confirmed (server). Flicker inferred | S | `test_server_entities_parity.py` (it already drops sand) |
| 30 | RCON, Query and the management server missing | `enable-rcon`, `enable-query` and `management-server-*` do nothing, so owners' tools can't connect | `RconThread`, `QueryThreadGs4`, `ManagementServer` | `properties.rs:41-55` fields with no reader | Confirmed (reading) | L | `test_server_files_parity.py` |
| 31 | Other ignored properties | `rate-limit` (packet-rate kick), `max-world-size`, `function-permission-level`, `max-chained-neighbor-updates`, `max-tick-time` (watchdog), `broadcast-rcon-to-ops`, `initial-enabled-packs` and `initial-disabled-packs`, `region-file-compression`, `sync-chunk-writes` | `DedicatedServerProperties` readers | `properties.rs` fields with no reader outside the crate | Confirmed (reading) | M | `test_server_files_parity.py` |
| 32 | Own potion swirls missing in first person | A player's own effect particles aren't drawn around them | `LivingEntity` `DATA_EFFECT_PARTICLES` (10) and `DATA_EFFECT_AMBIENCE_ID` (11) sent to self (`sendDirtyEntityData`) | `sync_player_state` sends 10 and 11 to others only (`hub.rs:1930`) | Inferred | S | `test_server_entities_parity.py` |

Never sent, and not worth a fix now: `TAB_LIST` (no vanilla trigger), `CUSTOM_CHAT_COMPLETIONS`, `DELETE_CHAT`,
`PLAYER_COMBAT_ENTER` and `PLAYER_COMBAT_END` (narration and telemetry only), `DEBUG_*`, `GAME_TEST_HIGHLIGHT_POS`,
`LOW_DISK_SPACE_WARNING`, `CUSTOM_REPORT_DETAILS`, `MOVE_MINECART_ALONG_TRACK` (experimental minecarts),
`START_CONFIGURATION`, `COOKIE_REQUEST`, `STORE_COOKIE`, `RESOURCE_PACK_POP`, `PING`, `SET_PLAYER_INVENTORY`
(the slots it would carry go out as `ContainerSetSlot` on window 0, which the client applies the same way).

### Combat, owned by fix-combat

Seen while reading, not investigated: #6 (attribute modifiers, attack cooldown) and the combat side of #2.
`handleAttack` (`lan_actions.rs:1538` → `guest_attack`) checks neither `isWithinAttackRange` nor the world
border, and doesn't kick for attacking item entities, experience orbs or oneself
(`multiplayer.disconnect.invalid_entity_attacked`).

### Stale docs

- `docs/server.md:94` "Left:" is out of date. Guest arrows hit players (`game/lan_pvp.rs:20`), and a guest's kills
  are theirs (`game/creatures.rs:888`).
- `docs/server-levels.md`, "Not done yet": entities already go through portals between levels
  (`game/nether_portal.rs:603` `handle_entity_portals`), and the End poem is handled (`won_game`,
  `lan_actions.rs:1795`).
- The docstring of `tools/test_dedicated_server_parity.py` says guests' death messages aren't Horizon's yet.
  They are now (`lan_actions.rs:2649`).

## Clusters for parallel fixing

Each cluster owns its files. A cluster that needs a line in another's file sends a message instead of
editing it. New code goes in new modules, with one-line hooks in the busy upstream files.

| Cluster | Gaps | Files it owns |
|---|---|---|
| **A. Hub: player sync and protocol hygiene** | 3 (criteria), 4, 7, 8, 10, 12 (emit 19 and 20), 17, 18, 19, 20, 23, 24, 25, 32, and the hub side of 9 (forward actions 3 and 4 to the game, as 5 and 6 already are, at `hub.rs:2568`) | `lan/hub.rs`, `lan/guest.rs`, `lan/hub/chat.rs`, `lan/conn.rs`, `lan/mod.rs`, `lan/advancements.rs`, new `lan/hub/player_meta.rs`, `game/scoreboard.rs` (criteria per guest), `game/shoulders.rs` (shoulder state per player) |
| **B. Game: serverbound actions as the guest** | 2 (`DamageEvent` for players beside `HurtAnimation`, coordinated with fix-combat), 5, 9 (the handler), 13, 15, 16, 21, 22 | `game/lan_actions.rs` (match arms only), `game/mounts.rs`, `game/commands/extra.rs` (`set_camera` and `cmd_dialog` per guest), new `game/lan_spectate.rs`, new `game/lan_editors.rs` (structure, jigsaw, test block, command minecart, tag queries), `horizon-protocol/src/packets/play.rs` (new serverbound variants and `SetCamera`, `ShowDialog` use) |
| **C. Operator commands** | 1 (with `/transfer` sending `Transfer`) | `horizon-command/src/vanilla.rs`, a new `horizon-command/src/tree_server.rs`, `horizon-command/src/vanilla_order.rs`, `game/commands.rs` (one dispatch arm), new `game/commands/server_cmds.rs`, `game/dedicated.rs` (share the console handlers) |
| **D. Entity data the world owns** | 11, 14, 26, 27, 29 | `projectiles.rs`, `arrows.rs`, `mobs/bee.rs`, `game/lan.rs` (`mob_meta` for 12 and 13), `lan/entity.rs`, `game/commands/terrain_cmds.rs`, `lan/containers.rs` (`sound`), `game/falling_*` or wherever landing lives |
| **E. Server properties and remote admin** | 30, 31 (also 28 if wanted, it is small) | `horizon-server-files/`, `crates/horizon-server/`, `game/dedicated.rs` after C lands, new modules for RCON and Query, `game/commands/world_cmds.rs` (28) |

Conflicts left: D's arrow counts on players reach A's `player_state` (D sends A the counts as a field on
`Guest`). B and fix-combat both touch `lan_actions.rs` near `HurtAnimation` (`:1011`). E waits for C on
`dedicated.rs`.

## Appendix A: serverbound coverage

Handled = like vanilla as far as read. Path abbreviations: `hub` = `lan/hub.rs`, `act` = `game/lan_actions.rs`.

| Vanilla handler | Ferraloom | Status |
|---|---|---|
| Common `handleKeepAlive` | `lan/conn.rs:1248` | Handled |
| Common `handlePong` | `act:1836` | Ignored (vanilla only uses it for its own pings, which Ferraloom never sends) |
| Common `handleCustomPayload` | `act:1689` (quiet channel), `conn.rs:1022` (brand) | Handled |
| Common `handleCustomClickAction` | `act:1836` | Ignored (vanilla hands it to `MinecraftServer#handleCustomClickAction`, which only logs it) |
| Common `handleResourcePackResponse` | `conn.rs:1131`, `conn.rs:1249` | Handled |
| Common `handleCookieResponse` | `conn.rs:970` | Ignored (no cookies are ever requested) |
| Config `handleClientInformation` | `conn.rs:1021`, `conn.rs:1059` | Handled |
| Config `handleSelectKnownPacks` | `conn.rs:1027` | Handled |
| Config `handleAcceptCodeOfConduct` | none | Missing with #17 |
| Config `handleConfigurationFinished` | `conn.rs:1058` | Handled |
| `handlePlayerInput` | `hub:2558` | Handled |
| `handleMoveVehicle` | `act:1559` → `game/lan_vehicles.rs` | Partly: no floating-vehicle kick (#7) |
| `handleAcceptTeleportPacket` | `hub:2488` | Handled |
| `handleAcceptPlayerLoad` | `hub:2526` | Handled |
| `handleRecipeBookSeenRecipePacket` | none | Ignored (#20) |
| `handleBundleItemSelectedPacket` | `act:1649` | Handled |
| `handleRecipeBookChangeSettingsPacket` | `hub:2543` | Handled |
| `handleSeenAdvancements` | none | Ignored (#24) |
| `handleCustomCommandSuggestions` | `act:1787` | Handled |
| `handleSetCommandBlock` | `act:1632` | Handled |
| `handleSetCommandMinecart` | none | Ignored (#21) |
| `handlePickItemFromBlock` | `act:1812` | Partly: fixed reach, not the `block_interaction_range` attribute |
| `handlePickItemFromEntity` | `act:1828` | Handled |
| `handleRenameItem` | `act:1630` | Handled |
| `handleSetBeaconPacket` | `act:1637` | Handled |
| `handleSetGameRule` | `act:1733` | Handled |
| `handleSetStructureBlock` | none | Ignored (#21) |
| `handleSetTestBlock` | none | Ignored (#21) |
| `handleTestInstanceBlockAction` | none | Ignored (#21) |
| `handleSetJigsawBlock` | none | Ignored (#21) |
| `handleJigsawGenerate` | none | Ignored (#21) |
| `handleSelectTrade` | `act:1638` | Handled |
| `handleEditBook` | `act:1633` | Handled |
| `handleEntityTagQuery` | none | Ignored (#22) |
| `handleContainerSlotStateChanged` | `act:1634` | Handled |
| `handleBlockEntityTagQuery` | none | Ignored (#22) |
| `handleMovePlayer` | `hub:2513-2525`, `hub:2601`, `hub:2659` | Partly: no floating kick (#7) |
| `handlePlayerAction` | `act:1348` | Partly: no reach check (#15). `CHANGE_DESTROY_DIRECTION` (1) isn't acked (harmless) |
| `handleUseItemOn` | `act:1473` | Partly: no range or hit-location check (#15), no build-limit message (#16) |
| `handleUseItem` | `act:1518` | Handled |
| `handleTeleportToEntityPacket` | `act:1759` | Handled |
| `handlePaddleBoat` | `act:1560` | Handled |
| `handleSetCarriedItem` | `act:1455` | Handled |
| `handleChat` | `hub/chat.rs:185` | Partly: no spam kick (#10) |
| `handleChatCommand` | `act:1688` → `lan_chat.rs:59` | Partly: no spam kick (#10). Admin commands fail (#1) |
| `handleSignedChatCommand` | `act:1692` → `lan_chat.rs:87` | As above |
| `handleChatAck` | `hub:2579` | Handled |
| `handleChatSessionUpdate` | `hub:2580` | Handled |
| `handlePunch` | `act:1467` | Handled |
| `handlePlayerCommand` | `hub:2568-2577`, `act:1621-1626` | Partly: `START_RIDING_JUMP` and `STOP_RIDING_JUMP` dropped (#9) |
| `handlePingRequest` (play) | `act:1836` | Ignored (#23) |
| `handleAttack` | `act:1538` | fix-combat's |
| `handleInteract` | `act:1539` | Handled |
| `handleSpectatorAction` | none | Ignored (#5) |
| `handleClientCommand` | `act:1719`, `act:1795-1808` | Handled |
| `handleContainerClose` | `act:1639-1648`, `act:1677` | Handled |
| `handleContainerClick` | `act:1612-1676` | Handled |
| `handlePlaceRecipe` | `act:1792` | Handled |
| `handleContainerButtonClick` | `act:1627` | Handled |
| `handleSetCreativeModeSlot` | `act:1563`, `act:1590` | Handled |
| `handleSignUpdate` | `act:1631` | Handled |
| `handlePlayerAbilities` | `hub:2509` | Handled |
| `handleClientInformation` (play) | `hub:2551` | Partly: no `UPDATE_HAT` broadcast (#25) |
| `handleChangeDifficulty` | `act:1693` | Handled |
| `handleChangeGameMode` | `act:1760` | Handled |
| `handleLockDifficulty` | `act:1711` | Handled |
| `handleChatSessionUpdate` | `hub:2580` | Handled |
| `handleConfigurationAcknowledged` | none | Not needed (the server never sends `StartConfiguration`) |
| `handleChunkBatchReceived` | `hub:2528` | Handled |
| `handleDebugSubscriptionRequest` | none | Ignored (debug renderers only) |
| `handleClientTickEnd` | `hub:2541` | Handled |

## Appendix B: clientbound coverage

"Sent" names one file that constructs the packet. The triggers were spot-checked, not proven complete.

| Packet | Status |
|---|---|
| `ADD_ENTITY`, `BUNDLE_DELIMITER`, `MOVE_ENTITY_POS`, `MOVE_ENTITY_POS_ROT`, `MOVE_ENTITY_ROT`, `ENTITY_POSITION_SYNC`, `ROTATE_HEAD`, `SET_ENTITY_LINK`, `SET_ENTITY_MOTION`, `SET_EQUIPMENT`, `SET_PASSENGERS`, `UPDATE_ATTRIBUTES`, `PROJECTILE_POWER` | Sent: `lan/entity.rs` |
| `SET_ENTITY_DATA` | Sent: `lan/hub.rs`, `lan/entity.rs`. Player indices 7, 12, 13, 18, 19 and 20 never (#8, #11, #12, #18), 10 and 11 not to self (#32) |
| `ANIMATE`, `BLOCK_EVENT`, `CONTAINER_SET_SLOT`, `DAMAGE_EVENT` (mobs only, #2), `ENTITY_EVENT`, `LEVEL_EVENT`, `MOUNT_SCREEN_OPEN`, `OPEN_SCREEN`, `SET_CURSOR_ITEM`, `SOUND`, `SWING_ANIMATION`, `TAKE_ITEM_ENTITY` | Sent: `lan/containers.rs` |
| `BLOCK_CHANGED_ACK`, `BLOCK_ENTITY_DATA`, `CHANGE_DIFFICULTY`, `CHUNK_BATCH_START`, `CHUNK_BATCH_FINISHED`, `COMMANDS`, `DISGUISED_CHAT`, `FORGET_LEVEL_CHUNK`, `GAME_EVENT`, `INITIALIZE_BORDER`, `LOGIN`, `PLACE_GHOST_RECIPE`, `PLAYER_ABILITIES`, `PLAYER_INFO_REMOVE`, `PLAYER_INFO_UPDATE` (no `UPDATE_HAT`, #25), `PLAYER_POSITION`, `REMOVE_ENTITIES`, `REMOVE_MOB_EFFECT`, `RESPAWN`, `SECTION_BLOCKS_UPDATE`, `SERVER_DATA`, `SET_CHUNK_CACHE_CENTER`, `SET_CHUNK_CACHE_RADIUS`, `SET_DEFAULT_SPAWN_POSITION`, `SET_EXPERIENCE`, `SET_HEALTH`, `SET_HELD_SLOT`, `SET_SIMULATION_DISTANCE`, `SET_TIME`, `TICKING_STEP`, `UPDATE_MOB_EFFECT` | Sent: `lan/hub.rs` |
| `AWARD_STATS`, `RECIPE_BOOK_ADD`, `RECIPE_BOOK_SETTINGS`, `UPDATE_ADVANCEMENTS`, `UPDATE_RECIPES`, `TICKING_STATE`, `POST_EFFECTS`, `COMMAND_SUGGESTIONS`, `WAYPOINT`, `LEVEL_CHUNK_WITH_LIGHT` | Sent, written by hand (`raw(ids::...)`, `lan/hub.rs`, `lan/chunks.rs`, `lan/waypoints.rs`, `lan_actions.rs:2327`) |
| `LIGHT_UPDATE` | Sent: `lan/chunks.rs` |
| `SET_BORDER_CENTER`, `SET_BORDER_LERP_SIZE`, `SET_BORDER_SIZE`, `SET_BORDER_WARNING_DELAY`, `SET_BORDER_WARNING_DISTANCE` | Sent: `lan/border.rs` |
| `RESET_SCORE`, `SET_DISPLAY_OBJECTIVE`, `SET_OBJECTIVE`, `SET_PLAYER_TEAM`, `SET_SCORE` | Sent: `lan/scoreboard.rs` (player-stat criteria never update, #3) |
| `PLAYER_CHAT`, `SYSTEM_CHAT` | Sent: `lan/hub/chat.rs`, `lan_actions.rs` |
| `CUSTOM_PAYLOAD`, `DISCONNECT`, `KEEP_ALIVE`, `PONG_RESPONSE` (status only, #23), `RESOURCE_PACK_PUSH`, `SERVER_LINKS`, `UPDATE_TAGS` | Sent: `lan/conn.rs` |
| `BLOCK_DESTRUCTION`, `BLOCK_UPDATE`, `COOLDOWN`, `GAME_RULE_VALUES`, `HURT_ANIMATION`, `PLAYER_COMBAT_KILL` | Sent: `game/lan_actions.rs` |
| `CONTAINER_CLOSE`, `CONTAINER_SET_CONTENT`, `CONTAINER_SET_DATA` | Sent: `game/lan_stations.rs` and the other menu files |
| `EXPLODE`, `LEVEL_PARTICLES` | Sent: `game/lan_particles.rs` |
| `BOSS_EVENT` (`game/bossbars.rs`), `MAP_ITEM_DATA` (`game/maps.rs`), `MERCHANT_OFFERS` (`game/lan_trading.rs`), `MOVE_VEHICLE` (`game/lan_vehicles.rs`), `OPEN_BOOK` (`game/books.rs`), `OPEN_SIGN_EDITOR` (`game/sign_edit.rs`), `RECIPE_BOOK_REMOVE`, `STOP_SOUND` (`game/commands/world_cmds.rs`), `CLEAR_TITLES`, `SET_ACTION_BAR_TEXT`, `SET_SUBTITLE_TEXT`, `SET_TITLE_TEXT`, `SET_TITLES_ANIMATION` (`game/commands/text_commands.rs`) | Sent |
| `SET_CAMERA` | **Never** (#5) |
| `SHOW_DIALOG`, `CLEAR_DIALOG` | **Never** (#13) |
| `CHUNKS_BIOMES` | **Never** (#14) |
| `TAG_QUERY` | **Never** (#22) |
| `SELECT_ADVANCEMENTS_TAB` | **Never** (#24) |
| `SOUND_ENTITY` | **Never** (#26) |
| `TELEPORT_ENTITY` | **Never** (#27) |
| `PLAYER_ROTATION`, `PLAYER_LOOK_AT` | **Never** (#28) |
| `ADD_TRANSIENT_BLOCK` | **Never** (#29) |
| `TRANSFER` | **Never** (#1, `/transfer`) |
| `TEST_INSTANCE_BLOCK_STATUS` | **Never** (#21) |
| `TAB_LIST`, `CUSTOM_CHAT_COMPLETIONS`, `DELETE_CHAT`, `PLAYER_COMBAT_ENTER`, `PLAYER_COMBAT_END`, `DEBUG_BLOCK_VALUE`, `DEBUG_CHUNK_VALUE`, `DEBUG_ENTITY_VALUE`, `DEBUG_EVENT`, `DEBUG_SAMPLE`, `GAME_TEST_HIGHLIGHT_POS`, `LOW_DISK_SPACE_WARNING`, `CUSTOM_REPORT_DETAILS`, `MOVE_MINECART_ALONG_TRACK`, `START_CONFIGURATION`, `COOKIE_REQUEST`, `STORE_COOKIE`, `RESOURCE_PACK_POP`, `PING`, `SET_PLAYER_INVENTORY` | **Never**, low impact (see the end of "Ranked gaps") |

## Not checked

- Nothing was run. The builder queue is shared, and the scenarios that would show these gaps (two vanilla-codec
  clients, ops, spectators) aren't in any existing harness, so they need new harness code, which this audit
  doesn't own. Each row names the harness to extend.
- The vanilla client isn't decompiled here (`minecraft.jar` in the cache holds server classes only), so every
  "what the player sees" for the client's rendering is inferred.
- The handlers marked "Handled" were checked for routing and their main side effects, not line by line. Their
  replies (state ids, resyncs) were trusted where Horizon cites the vanilla method.
- Plugins and Java plugins were out of scope.
