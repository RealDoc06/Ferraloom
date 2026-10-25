# Server operations

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| server-operations.startup.cli | Honor dedicated-server command-line options and reject invalid combinations | `Main#main` |  | - | partial |
| server-operations.startup.eula | Create and enforce eula.txt before starting the dedicated server | `Eula; Main#main` |  | - | partial |
| server-operations.startup.properties | Load, normalize and persist server.properties with typed defaults | `Settings#loadFromFile; Settings#store; DedicatedServerSettings` | `properties::tests::java_properties_and_validation` | - | partial |
| server-operations.startup.world-lock | Acquire the session lock and reject concurrent access to the same save | `DirectoryLock#create; LevelStorageSource` |  | - | partial |
| server-operations.startup.bonus-seed-preset | Apply seed parsing, world preset, generator settings and structure-generation settings | `DedicatedServerProperties#<init>; WorldOptions#parseSeed` |  | - | partial |
| server-operations.startup.initial-packs | Load configured initial enabled and disabled datapacks on new-world creation | `DedicatedServerProperties; WorldLoader` |  | - | partial |
| server-operations.ops.persist | Persist operator profiles, permission levels and player-limit bypass in ops.json | `ServerOpList; ServerOpListEntry` |  | - | partial |
| server-operations.whitelist.persist | Persist whitelist profiles and apply enforcement to existing and joining players | `UserWhiteList; PlayerList#reloadWhiteList` |  | - | partial |
| server-operations.bans.profile | Persist player bans, expiry, source and reason and reject banned profiles | `UserBanList; UserBanListEntry; PlayerList#canPlayerLogin` |  | - | partial |
| server-operations.bans.ip | Persist IP bans and reject banned addresses with expiry handling | `IpBanList; IpBanListEntry; PlayerList#canPlayerLogin` |  | - | partial |
| server-operations.users.cache | Resolve and cache names and UUIDs with expiry and offline fallback | `CachedUserNameToIdResolver` |  | - | partial |
| server-operations.users.legacy-conversion | Convert legacy user lists and player data to UUID-based formats | `OldUsersConverter` | | - | todo |
| server-operations.console.input | Queue console commands on the server thread with console permissions and output | `DedicatedServer#handleConsoleInputs; ConsoleInput` |  | - | partial |
| server-operations.console.gui | Provide the optional dedicated-server GUI and console command entry | `MinecraftServerGui` | | - | todo |
| server-operations.save.autosave | Autosave players and worlds on the tick-rate-aware interval with minimum interval | `MinecraftServer#autoSave; MinecraftServer#computeNextAutosaveInterval` |  | - | partial |
| server-operations.save.manual | Save all levels and players with explicit flush and save-enabled flags | `MinecraftServer#saveEverything; SaveAllCommand` | `join::offline_client_joins_walks_chats_and_saves` | - | partial |
| server-operations.shutdown.flush | Disconnect players and flush world and storage state during orderly shutdown | `MinecraftServer#stopServer` | `join::offline_client_joins_walks_chats_and_saves`; `ferraloom-parity::starter::parity_save_and_reload_player_position` | pass | partial |
| server-operations.watchdog.stall | Produce a crash report and terminate after max-tick-time is exceeded | `ServerWatchdog#run` | | - | todo |
| server-operations.monitor.jmx | Expose dedicated-server tick metrics through the optional JMX bean | `MinecraftServerStatistics` | | - | todo |
| server-operations.monitor.jfr | Start and stop flight recording and publish profiling results | `JvmProfiler; JfrCommand` | | - | todo |
| server-operations.monitor.debug-report | Write debug reports, tick profiles and performance capture on command | `MinecraftServer#saveDebugReport; PerfCommand; DebugCommand` | | - | todo |
| server-operations.pause.empty | Autosave once and pause simulation while continuing connection ticks when empty | `MinecraftServer#tickServer` | | - | todo |
| server-operations.players.idle-kick | Disconnect idle players after configured timeout | `ServerGamePacketListenerImpl#tick` | | - | todo |
| server-operations.players.spawn-protection | Apply operator-sensitive spawn protection in the spawn dimension | `DedicatedServer#isUnderSpawnProtection` | | - | todo |
| server-operations.players.difficulty | Apply and persist world difficulty and locking; hardcore difficulty constraints | `MinecraftServer#setDifficulty; PrimaryLevelData` | | - | todo |
| server-operations.cli.bonus-chest | Dedicated-server option `--bonusChest` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.demo | Dedicated-server option `--demo` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.erase-cache | Dedicated-server option `--eraseCache` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.force-upgrade | Dedicated-server option `--forceUpgrade` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.help | Dedicated-server option `--help` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.init-settings | Dedicated-server option `--initSettings` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.jfr-profile | Dedicated-server option `--jfrProfile` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.nogui | Dedicated-server option `--nogui` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.pid-file | Dedicated-server option `--pidFile` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.port | Dedicated-server option `--port` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.recreate-region-files | Dedicated-server option `--recreateRegionFiles` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.safe-mode | Dedicated-server option `--safeMode` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.server-id | Dedicated-server option `--serverId` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.universe | Dedicated-server option `--universe` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.cli.world | Dedicated-server option `--world` | `net.minecraft.server.Main#main` | | - | todo |
| server-operations.property.accepts-transfers | Parse and apply `server.properties` key `accepts-transfers` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.allow-flight | Parse and apply `server.properties` key `allow-flight` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.announce-player-achievements | Parse and apply `server.properties` key `announce-player-achievements` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.broadcast-console-to-ops | Parse and apply `server.properties` key `broadcast-console-to-ops` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.broadcast-rcon-to-ops | Parse and apply `server.properties` key `broadcast-rcon-to-ops` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.bug-report-link | Parse and apply `server.properties` key `bug-report-link` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.chat-spam-threshold-seconds | Parse and apply `server.properties` key `chat-spam-threshold-seconds` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.command-spam-threshold-seconds | Parse and apply `server.properties` key `command-spam-threshold-seconds` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.difficulty | Parse and apply `server.properties` key `difficulty` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.enable-code-of-conduct | Parse and apply `server.properties` key `enable-code-of-conduct` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.enable-jmx-monitoring | Parse and apply `server.properties` key `enable-jmx-monitoring` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.enable-query | Parse and apply `server.properties` key `enable-query` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.enable-rcon | Parse and apply `server.properties` key `enable-rcon` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.enable-status | Parse and apply `server.properties` key `enable-status` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.enforce-secure-profile | Parse and apply `server.properties` key `enforce-secure-profile` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.enforce-whitelist | Parse and apply `server.properties` key `enforce-whitelist` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.entity-broadcast-range-percentage | Parse and apply `server.properties` key `entity-broadcast-range-percentage` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.force-gamemode | Parse and apply `server.properties` key `force-gamemode` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.function-permission-level | Parse and apply `server.properties` key `function-permission-level` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.gamemode | Parse and apply `server.properties` key `gamemode` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.generate-structures | Parse and apply `server.properties` key `generate-structures` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.generator-settings | Parse and apply `server.properties` key `generator-settings` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.hardcore | Parse and apply `server.properties` key `hardcore` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.hide-online-players | Parse and apply `server.properties` key `hide-online-players` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.initial-disabled-packs | Parse and apply `server.properties` key `initial-disabled-packs` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.initial-enabled-packs | Parse and apply `server.properties` key `initial-enabled-packs` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.level-name | Parse and apply `server.properties` key `level-name` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.level-seed | Parse and apply `server.properties` key `level-seed` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.level-type | Parse and apply `server.properties` key `level-type` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.log-ips | Parse and apply `server.properties` key `log-ips` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.management-server-allowed-origins | Parse and apply `server.properties` key `management-server-allowed-origins` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.management-server-enabled | Parse and apply `server.properties` key `management-server-enabled` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.management-server-host | Parse and apply `server.properties` key `management-server-host` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.management-server-port | Parse and apply `server.properties` key `management-server-port` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.management-server-secret | Parse and apply `server.properties` key `management-server-secret` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.management-server-tls-enabled | Parse and apply `server.properties` key `management-server-tls-enabled` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.management-server-tls-keystore | Parse and apply `server.properties` key `management-server-tls-keystore` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.management-server-tls-keystore-password | Parse and apply `server.properties` key `management-server-tls-keystore-password` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.max-chained-neighbor-updates | Parse and apply `server.properties` key `max-chained-neighbor-updates` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.max-players | Parse and apply `server.properties` key `max-players` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.max-tick-time | Parse and apply `server.properties` key `max-tick-time` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.max-world-size | Parse and apply `server.properties` key `max-world-size` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.motd | Parse and apply `server.properties` key `motd` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.network-compression-threshold | Parse and apply `server.properties` key `network-compression-threshold` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.online-mode | Parse and apply `server.properties` key `online-mode` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.op-permission-level | Parse and apply `server.properties` key `op-permission-level` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.pause-when-empty-seconds | Parse and apply `server.properties` key `pause-when-empty-seconds` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.player-idle-timeout | Parse and apply `server.properties` key `player-idle-timeout` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.prevent-proxy-connections | Parse and apply `server.properties` key `prevent-proxy-connections` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.query-port | Parse and apply `server.properties` key `query.port` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.rate-limit | Parse and apply `server.properties` key `rate-limit` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.rcon-password | Parse and apply `server.properties` key `rcon.password` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.rcon-port | Parse and apply `server.properties` key `rcon.port` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.region-file-compression | Parse and apply `server.properties` key `region-file-compression` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.require-resource-pack | Parse and apply `server.properties` key `require-resource-pack` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.resource-pack | Parse and apply `server.properties` key `resource-pack` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.resource-pack-hash | Parse and apply `server.properties` key `resource-pack-hash` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.resource-pack-id | Parse and apply `server.properties` key `resource-pack-id` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.resource-pack-prompt | Parse and apply `server.properties` key `resource-pack-prompt` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.resource-pack-sha1 | Parse and apply `server.properties` key `resource-pack-sha1` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.server-ip | Parse and apply `server.properties` key `server-ip` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.server-port | Parse and apply `server.properties` key `server-port` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.simulation-distance | Parse and apply `server.properties` key `simulation-distance` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.spawn-protection | Parse and apply `server.properties` key `spawn-protection` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.status-heartbeat-interval | Parse and apply `server.properties` key `status-heartbeat-interval` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.sync-chunk-writes | Parse and apply `server.properties` key `sync-chunk-writes` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.text-filtering-config | Parse and apply `server.properties` key `text-filtering-config` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.text-filtering-version | Parse and apply `server.properties` key `text-filtering-version` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.use-native-transport | Parse and apply `server.properties` key `use-native-transport` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.view-distance | Parse and apply `server.properties` key `view-distance` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.property.white-list | Parse and apply `server.properties` key `white-list` | `DedicatedServerProperties#<init>` | | - | todo |
| server-operations.levels.dimension-construction | Construct overworld first and each actual registered stem with its generator, static context and independent generation/IO runtime; unknown dimensions are nullable at server lookup and required errors at construction | `MinecraftServer#createLevels; ServerLevel` | `world::root::tests::dimensions_share_one_root_and_keep_their_actual_generators_and_io; unknown_dimension_does_not_fall_back_to_overworld_or_mutate_metadata` (source-only; multi-level ticking/routing follows) | - | partial |
