# Permissions

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| permissions.level-based | Map operator and function permission levels to the supported permission sets | `LevelBasedPermissionSet; PermissionLevel` | | - | todo |
| permissions.checks.composition | Evaluate permission predicates against command sources and player permission providers | `PermissionCheck; PermissionProviderCheck` | | - | todo |
| permissions.sync.player-status | Send changed operator status and command trees after permission updates | `PlayerList#sendPlayerPermissionLevel; Commands#sendCommands` | | - | todo |
| permissions.permission-check-type.always-pass | `minecraft:always_pass` (permission check type) | `net.minecraft.server.permissions.PermissionCheckTypes#bootstrap` | | - | todo |
| permissions.permission-check-type.require | `minecraft:require` (permission check type) | `net.minecraft.server.permissions.PermissionCheckTypes#bootstrap` | | - | todo |
| permissions.permission-type.atom | `minecraft:atom` (permission type) | `net.minecraft.server.permissions.PermissionTypes#bootstrap` | | - | todo |
| permissions.permission-type.command-level | `minecraft:command_level` (permission type) | `net.minecraft.server.permissions.PermissionTypes#bootstrap` | | - | todo |
