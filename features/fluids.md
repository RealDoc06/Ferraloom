# Fluids

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| fluids.water.flow | Spread water downward first and choose horizontal paths using slope distance | `FlowingFluid#spread; FlowingFluid#getSpread; WaterFluid` | `ferraloom-game: level::tests::water_spread_uses_scheduled_delay_and_crosses_a_chunk_boundary ; ferraloom-parity: parity_water_spreads_over_flat_ground_for_30_ticks (integration pending)` | - | todo |
| fluids.water.sources | Convert to a source only with enough neighboring sources, support and the source-conversion rule | `FlowingFluid#getNewLiquid; WaterFluid#canConvertToSource` | `ferraloom-game: level::tests::water_source_conversion_requires_support_and_its_rule_and_above_makes_falling_fluid` | - | todo |
| fluids.lava.dimension-flow | Use dimension-dependent lava tick delay, slope search and level drop-off | `LavaFluid#getTickDelay; LavaFluid#getSlopeFindDistance; LavaFluid#getDropOff` | | - | todo |
| fluids.lava.source-rule | Apply optional lava source conversion independently from water source conversion | `LavaFluid#canConvertToSource` | | - | todo |
| fluids.flow.wall-occlusion | Block flow using merged collision faces and fluid-container capacity | `FlowingFluid#canPassThroughWall; FlowingFluid#canHoldFluid` | `ferraloom-game: level::tests::fluid_wall_occlusion_matches_vanilla_native_partial_faces_in_all_directions (150 Java vectors)` | pass | todo |
| fluids.liquid.entity-inside | Inherit LiquidBlock's base block callback; apply water and lava effects separately through fluidInside and the canonical ordered collector | `LiquidBlock; Entity#checkInsideBlocks; Fluid#entityInside` | `ferraloom-game: level::tests::reflected_method_ownership_ports_inherited_defaults_without_masking_overrides; entity::inside_block (collector integration)` | - | todo |
| fluids.flow.falling-state | Preserve falling-fluid amount and shape when fluid exists above | `FlowingFluid#getNewLiquid; FlowingFluid#getHeight` | | - | todo |
| fluids.flow.entity-force | Compute flow vectors and apply fluid pushing, swimming and drag to entities | `FlowingFluid#getFlow; Entity; EntityFluidInteraction` | | - | todo |
| fluids.mixing.obsidian-cobblestone | Resolve lava and water contact into obsidian, cobblestone or stone by contact state | `LiquidBlock#shouldSpreadLiquid; LavaFluid#spreadTo` | `ferraloom-game: level::tests::liquid_block_mixes_lava_into_obsidian_cobblestone_and_basalt; ferraloom-parity: parity_lava_meets_water (integration pending)` | - | todo |
| fluids.mixing.basalt | Generate basalt when lava contacts the required soul-soil and blue-ice arrangement | `LiquidBlock#shouldSpreadLiquid` | `ferraloom-game: level::tests::liquid_block_mixes_lava_into_obsidian_cobblestone_and_basalt; ferraloom-parity: parity_lava_meets_water (integration pending)` | - | todo |
| fluids.lava.ignition | Ignite nearby flammable blocks during lava random ticks under fire rules | `LavaFluid#randomTick` | | - | todo |
| fluids.bucket.pickup-place | Pick up valid source fluids, place or waterlog and handle ultrawarm evaporation | `BucketItem` | | - | todo |
| fluids.bottle.water | Fill glass bottles from eligible water and cauldrons | `BottleItem; CauldronInteractions` | | - | todo |
| fluids.fluid.empty | `minecraft:empty` (fluid) | `net.minecraft.world.level.material.Fluids#register; net.minecraft.world.level.material.EmptyFluid` | | - | todo |
| fluids.fluid.flowing-lava | `minecraft:flowing_lava` (fluid) | `net.minecraft.world.level.material.Fluids#register` | | - | todo |
| fluids.fluid.flowing-water | `minecraft:flowing_water` (fluid) | `net.minecraft.world.level.material.Fluids#register` | | - | todo |
| fluids.fluid.lava | `minecraft:lava` (fluid) | `net.minecraft.world.level.material.Fluids#register` | | - | todo |
| fluids.fluid.water | `minecraft:water` (fluid) | `net.minecraft.world.level.material.Fluids#register` | | - | todo |

| fluids.waterlogged.simple-container | Dispatch inherited waterlogging defaults and actual overrides; reject double slabs and flowing-water identity, schedule source-water ticks and pick up buckets | `SimpleWaterloggedBlock; ChainBlock#updateShape; SlabBlock#canPlaceLiquid` | `ferraloom-game: level::tests::vanilla_simple_waterlogged_chain_places_source_ticks_and_picks_up_water; slab_waterlogging_matches_vanilla_interface_overrides_and_source_identity (9 Java vectors)` | pass | todo |
| fluids.liquid-block.legacy-state | Map source, flowing and falling fluid states to their canonical LiquidBlock state | `FlowingFluid#getLegacyLevel; LiquidBlock#getFluidState` | `ferraloom-game: level::tests::vanilla_liquid_legacy_state_canonicalizes_falling_levels` | pass | todo |
| fluids.state.liquid-and-ignition | Retain state-only liquid and lava-ignition facts independently of contained fluid | `BlockBehaviour.BlockStateBase#liquid; BlockBehaviour.BlockStateBase#ignitedByLava` | `ferraloom-data: tests::liquid_and_lava_ignition_are_state_facts_independent_of_waterlogging` | pass | todo |
| fluids.datapack.membership | Use loaded WATER, LAVA and BUBBLE_COLUMN_CAN_OCCUPY fluid tags for replacement, mixing and bubble eligibility | `WaterFluid#canBeReplacedWith; LavaFluid#spreadTo; LiquidBlock#shouldBubbleColumnOccupy` | `ferraloom-game: level::tests::liquid_block_mixes_lava_into_obsidian_cobblestone_and_basalt; Java fluid-tag membership vectors` | pass | todo |
| fluids.water.slope-limit-and-border | Search slopes across chunk borders and stop at water's exact distance limit | `FlowingFluid#getSlopeDistance` | `ferraloom-game: level::tests::water_slope_search_matches_real_vanilla_across_chunk_borders_and_at_its_limit` | pass | todo |
| fluids.lava.random-dispatch | Invoke LiquidBlock and fluid random callbacks separately and count each unsupported skip | `ServerLevel#tickChunk; LiquidBlock#randomTick` | `ferraloom-game: level::tests::random_phase_is_strict_and_warn_counts_both_vanilla_lava_callbacks` | pass | todo |
