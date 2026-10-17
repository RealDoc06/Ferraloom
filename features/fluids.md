# Fluids

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| fluids.water.flow | Spread water downward first and choose horizontal paths using slope distance | `FlowingFluid#spread; FlowingFluid#getSpread; WaterFluid` | | - | todo |
| fluids.water.sources | Convert to a source only with enough neighboring sources, support and the source-conversion rule | `FlowingFluid#getNewLiquid; WaterFluid#canConvertToSource` | | - | todo |
| fluids.lava.dimension-flow | Use dimension-dependent lava tick delay, slope search and level drop-off | `LavaFluid#getTickDelay; LavaFluid#getSlopeFindDistance; LavaFluid#getDropOff` | | - | todo |
| fluids.lava.source-rule | Apply optional lava source conversion independently from water source conversion | `LavaFluid#canConvertToSource` | | - | todo |
| fluids.flow.wall-occlusion | Block flow using merged collision faces and fluid-container capacity | `FlowingFluid#canPassThroughWall; FlowingFluid#canHoldFluid` | | - | todo |
| fluids.flow.falling-state | Preserve falling-fluid amount and shape when fluid exists above | `FlowingFluid#getNewLiquid; FlowingFluid#getHeight` | | - | todo |
| fluids.flow.entity-force | Compute flow vectors and apply fluid pushing, swimming and drag to entities | `FlowingFluid#getFlow; Entity; EntityFluidInteraction` | | - | todo |
| fluids.mixing.obsidian-cobblestone | Resolve lava and water contact into obsidian, cobblestone or stone by contact state | `LiquidBlock#shouldSpreadLiquid; LavaFluid#spreadTo` | | - | todo |
| fluids.mixing.basalt | Generate basalt when lava contacts the required soul-soil and blue-ice arrangement | `LiquidBlock#shouldSpreadLiquid` | | - | todo |
| fluids.lava.ignition | Ignite nearby flammable blocks during lava random ticks under fire rules | `LavaFluid#randomTick` | | - | todo |
| fluids.bucket.pickup-place | Pick up valid source fluids, place or waterlog and handle ultrawarm evaporation | `BucketItem` | | - | todo |
| fluids.bottle.water | Fill glass bottles from eligible water and cauldrons | `BottleItem; CauldronInteractions` | | - | todo |
| fluids.fluid.empty | `minecraft:empty` (fluid) | `net.minecraft.world.level.material.Fluids#register; net.minecraft.world.level.material.EmptyFluid` | | - | todo |
| fluids.fluid.flowing-lava | `minecraft:flowing_lava` (fluid) | `net.minecraft.world.level.material.Fluids#register` | | - | todo |
| fluids.fluid.flowing-water | `minecraft:flowing_water` (fluid) | `net.minecraft.world.level.material.Fluids#register` | | - | todo |
| fluids.fluid.lava | `minecraft:lava` (fluid) | `net.minecraft.world.level.material.Fluids#register` | | - | todo |
| fluids.fluid.water | `minecraft:water` (fluid) | `net.minecraft.world.level.material.Fluids#register` | | - | todo |
