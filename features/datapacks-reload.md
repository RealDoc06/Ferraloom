# Datapacks and resource reload

Includes reload behavior, optional bundled pack selection, dialogs and server-approved debug subscriptions. Dialog resource entries and their type/action/input registries are tracked here.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| datapacks-reload.packs.discovery | Discover built-in, directory and zip datapacks with metadata and required feature sets | `ServerPacksSource; FolderRepositorySource; PackRepository` | | - | todo |
| datapacks-reload.packs.priority | Apply pack order and resource replacement with higher-priority packs winning | `MultiPackResourceManager; FallbackResourceManager` | | - | todo |
| datapacks-reload.packs.filters-overlays | Apply pack filters and conditional overlays for the active pack version | `PackMetadataSection; OverlayMetadataSection; ResourceFilterSection` | | - | todo |
| datapacks-reload.packs.format-compatibility | Check supported data-pack format range and requested feature flags | `Pack; PackCompatibility; FeatureFlags` | | - | todo |
| datapacks-reload.packs.enable-disable | Enable, disable and reorder available packs while preserving required packs | `DataPackCommand; PackRepository` | | - | todo |
| datapacks-reload.registry.static-dynamic | Separate bootstrapped registries, worldgen registries and reloadable registries | `BuiltInRegistries; RegistryDataLoader; ReloadableServerRegistries` | | - | todo |
| datapacks-reload.registry.holder-references | Resolve registry keys, holder references and tags and reject unresolved references | `RegistryOps; RegistryCodecs; MappedRegistry` | | - | todo |
| datapacks-reload.reload.prepare-apply | Prepare reloadable resources asynchronously and apply coordinated reload results | `ReloadableServerResources; SimpleReloadInstance` | | - | todo |
| datapacks-reload.reload.fail-safe | Report failed reloads while preserving the currently active resource set | `MinecraftServer#reloadResources` | | - | todo |
| datapacks-reload.reload.players | Refresh recipes, tags, commands and advancement listeners for connected players | `PlayerList#reloadResources; MinecraftServer#reloadResources` | | - | todo |
| datapacks-reload.reload.safe-mode | Load the vanilla pack selection in safe mode when requested at startup | `WorldLoader; Main` | | - | todo |
| datapacks-reload.dialog.open-clear | Open registered or inline dialogs and clear dialogs for selected players | `DialogCommand; ServerPlayer#openDialog` | | - | todo |
| datapacks-reload.dialog.actions | Handle permitted custom click actions and dialog input submissions | `ServerCommonPacketListenerImpl#handleCustomClickAction; ActionTypes` | | - | todo |
| datapacks-reload.dialog.command-template | Resolve dialog input into command templates and execute with the originating source | `ActionTypes; InputControlTypes` | | - | todo |
| datapacks-reload.debug.subscriptions | Manage server-approved debug subscriptions and stream their selected world data | `ServerGamePacketListenerImpl#handleDebugSubscriptionRequest; DebugSubscriptions` | | - | todo |
| datapacks-reload.builtin-pack.minecart-improvements | Bundled optional datapack `minecart_improvements`; feature gates and overrides | `data/minecraft/datapacks/minecart_improvements/pack.mcmeta`; `ServerPacksSource` | | - | todo |
| datapacks-reload.builtin-pack.redstone-experiments | Bundled optional datapack `redstone_experiments`; feature gates and overrides | `data/minecraft/datapacks/redstone_experiments/pack.mcmeta`; `ServerPacksSource` | | - | todo |
| datapacks-reload.builtin-pack.trade-rebalance | Bundled optional datapack `trade_rebalance`; feature gates and overrides | `data/minecraft/datapacks/trade_rebalance/pack.mcmeta`; `ServerPacksSource` | | - | todo |
| datapacks-reload.debug-subscription.bee-hives | `minecraft:bee_hives` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.bees | `minecraft:bees` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.brains | `minecraft:brains` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.breezes | `minecraft:breezes` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.dedicated-server-tick-time | `minecraft:dedicated_server_tick_time` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.entity-block-intersections | `minecraft:entity_block_intersections` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.entity-paths | `minecraft:entity_paths` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.game-event-listeners | `minecraft:game_event_listeners` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.game-events | `minecraft:game_events` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.goal-selectors | `minecraft:goal_selectors` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.neighbor-updates | `minecraft:neighbor_updates` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.pois | `minecraft:pois` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.raids | `minecraft:raids` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.redstone-wire-orientations | `minecraft:redstone_wire_orientations` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.structures | `minecraft:structures` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.debug-subscription.village-sections | `minecraft:village_sections` (debug subscription) | `net.minecraft.util.debug.DebugSubscriptions#bootstrap; net.minecraft.util.debug.DebugSubscription` | | - | todo |
| datapacks-reload.dialog-action-type.change-page | `minecraft:change_page` (dialog action type) | `net.minecraft.server.dialog.action.ActionTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-action-type.copy-to-clipboard | `minecraft:copy_to_clipboard` (dialog action type) | `net.minecraft.server.dialog.action.ActionTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-action-type.custom | `minecraft:custom` (dialog action type) | `net.minecraft.server.dialog.action.ActionTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-action-type.dynamic.custom | `minecraft:dynamic/custom` (dialog action type) | `net.minecraft.server.dialog.action.ActionTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-action-type.dynamic.run-command | `minecraft:dynamic/run_command` (dialog action type) | `net.minecraft.server.dialog.action.ActionTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-action-type.open-url | `minecraft:open_url` (dialog action type) | `net.minecraft.server.dialog.action.ActionTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-action-type.run-command | `minecraft:run_command` (dialog action type) | `net.minecraft.server.dialog.action.ActionTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-action-type.show-dialog | `minecraft:show_dialog` (dialog action type) | `net.minecraft.server.dialog.action.ActionTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-action-type.suggest-command | `minecraft:suggest_command` (dialog action type) | `net.minecraft.server.dialog.action.ActionTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-body-type.item | `minecraft:item` (dialog body type) | `net.minecraft.server.dialog.body.DialogBodyTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-body-type.plain-message | `minecraft:plain_message` (dialog body type) | `net.minecraft.server.dialog.body.DialogBodyTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-type.confirmation | `minecraft:confirmation` (dialog type) | `net.minecraft.server.dialog.DialogTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-type.dialog-list | `minecraft:dialog_list` (dialog type) | `net.minecraft.server.dialog.DialogTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-type.multi-action | `minecraft:multi_action` (dialog type) | `net.minecraft.server.dialog.DialogTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-type.notice | `minecraft:notice` (dialog type) | `net.minecraft.server.dialog.DialogTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog-type.server-links | `minecraft:server_links` (dialog type) | `net.minecraft.server.dialog.DialogTypes#bootstrap` | | - | todo |
| datapacks-reload.dialog.custom-options | `minecraft:custom_options` (dialog) | `data/minecraft/dialog/custom_options.json`; `Dialog#DIRECT_CODEC` | | - | todo |
| datapacks-reload.dialog.quick-actions | `minecraft:quick_actions` (dialog) | `data/minecraft/dialog/quick_actions.json`; `Dialog#DIRECT_CODEC` | | - | todo |
| datapacks-reload.dialog.server-links | `minecraft:server_links` (dialog) | `data/minecraft/dialog/server_links.json`; `Dialog#DIRECT_CODEC` | | - | todo |
| datapacks-reload.input-control-type.boolean | `minecraft:boolean` (input control type) | `net.minecraft.server.dialog.input.InputControlTypes#bootstrap` | | - | todo |
| datapacks-reload.input-control-type.number-range | `minecraft:number_range` (input control type) | `net.minecraft.server.dialog.input.InputControlTypes#bootstrap` | | - | todo |
| datapacks-reload.input-control-type.single-option | `minecraft:single_option` (input control type) | `net.minecraft.server.dialog.input.InputControlTypes#bootstrap` | | - | todo |
| datapacks-reload.input-control-type.text | `minecraft:text` (input control type) | `net.minecraft.server.dialog.input.InputControlTypes#bootstrap` | | - | todo |
| datapacks-reload.config.native-codec | Decode world DataPacks and enabled_features with native lenient defaults, default-object encoding and JDK feature order; split initial property lists (44 native observations, Rust replay unrun) | `WorldDataConfiguration.CODEC; DataPackConfig.CODEC; FeatureFlagRegistry; DedicatedServerProperties#getDatapackConfig` | `datapack::config::tests::native_world_data_configuration_defaults_features_encoding_and_properties` | pass | partial |
