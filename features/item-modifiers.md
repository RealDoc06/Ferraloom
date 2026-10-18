# Item modifiers

Named custom item modifiers are a supported datapack surface. The default built-in datapack supplies no item_modifier JSON entries; function types are inventoried in loot.md.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| item-modifiers.loader.entry | Load namespaced item_modifier JSON as a function or sequence of functions | `ReloadableServerRegistries; LootItemFunctions` | | - | todo |
| item-modifiers.application.context | Apply item modifiers through commands with the correct entity and block context | `ItemCommands; LootContext` | | - | todo |
| item-modifiers.application.components | Preserve component changes, count limits and conditional modifier ordering | `LootItemFunction; LootItemFunctions` | `loot::tests::synthetic_tables_and_providers_match_real_java` | pass | todo |
| item-modifiers.reference.holders | Resolve named modifier registry holders against the active resource registry | `LootItemFunctions#CODEC; RegistryCodecs` | | - | todo |
