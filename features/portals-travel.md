# Portals and dimension travel

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| portals-travel.nether.frame-create | Validate portal frame dimensions, axis and interior before creating portal blocks | `PortalShape` | | - | todo |
| portals-travel.nether.entry-delay | Apply creative and default portal wait time rules with portal cooldown | `NetherPortalBlock#getPortalTransitionTime; PortalProcessor` | | - | todo |
| portals-travel.nether.coordinate-scale | Scale dimension coordinates and clamp target position to destination border | `NetherPortalBlock#getPortalDestination; DimensionType#getTeleportationScale` | | - | todo |
| portals-travel.nether.search-radius | Search existing portals within 16 blocks in the Nether and 128 blocks outside it | `PortalForcer#findClosestPortalPosition` | | - | todo |
| portals-travel.nether.exit-rectangle | Preserve relative entry position and rotate yaw and velocity for exit orientation | `NetherPortalBlock#getDimensionTransitionFromExit` | | - | todo |
| portals-travel.nether.create-fallback | Build a valid destination frame or fallback platform when no portal exists | `PortalForcer#createPortal` | | - | todo |
| portals-travel.end.frame-eyes | Activate an End portal only for a complete correctly oriented eye-filled frame | `EnderEyeItem; EndPortalFrameBlock` | | - | todo |
| portals-travel.end.entry-platform | Create or repair the End arrival platform during entry | `EndPortalBlock#getPortalDestination; EndPlatformFeature#createEndPlatform` | | - | todo |
| portals-travel.end.return-credits | Return players through the End exit and handle credits and respawn behavior | `EndPortalBlock#getPortalDestination; ServerPlayer` | | - | todo |
| portals-travel.gateway.exit | Find gateway exit terrain, create an exit gateway and use its cooldown | `TheEndGatewayBlockEntity` | | - | todo |
| portals-travel.entity.dimension-change | Move entities and passengers between levels with UUID, velocity and post-teleport handling | `Entity#teleport; TeleportTransition` | | - | todo |
| portals-travel.player.dimension-sync | Send respawn, level info, effects, abilities and position state on dimension change | `ServerPlayer#teleport; PlayerList#sendLevelInfo` | | - | todo |
| portals-travel.commands.cross-dimension | Teleport and execute across dimensions with coordinate scaling and relative movement flags | `TeleportCommand; ExecuteCommand` | | - | todo |
