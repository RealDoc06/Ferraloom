# Lighting

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| lighting.block.emission | Propagate emitted block light with opacity and shape occlusion | `BlockLightEngine` | | - | todo |
| lighting.block.removal | Remove obsolete light and repropagate surviving sources after block changes | `LightEngine` | | - | todo |
| lighting.sky.sources | Maintain sky-light source heights and propagate skylight through sections | `SkyLightEngine; ChunkSkyLightSources` | | - | todo |
| lighting.sky.column-gaps | Fill and clear skylight across missing and empty section boundaries | `SkyLightSectionStorage` | | - | todo |
| lighting.section.lifecycle | Retain, queue and remove light section data with chunk readiness changes | `LayerLightSectionStorage` | | - | todo |
| lighting.chunk.initialize | Initialize light sources and light chunks before marking light-correct | `ThreadedLevelLightEngine` | | - | todo |
| lighting.chunk.boundaries | Propagate and reconcile light at neighboring chunk and section boundaries | `LevelLightEngine; LightEngine` | | - | todo |
| lighting.storage.network | Round-trip nibble arrays and expose correct empty and nonempty light masks | `DataLayer; ClientboundLightUpdatePacketData` | | - | todo |
| lighting.thread.order | Order pending light tasks against chunk generation and block changes | `ThreadedLevelLightEngine` | | - | todo |
| lighting.dimension.no-skylight | Disable skylight storage and propagation in dimensions without skylight | `LevelLightEngine` | | - | todo |
