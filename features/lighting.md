# Lighting

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| lighting.block.emission | Propagate emitted block light with opacity and shape occlusion | `BlockLightEngine` | `ferraloom-light: room, directional, dampening` | pass | pass |
| lighting.block.removal | Remove obsolete light and repropagate surviving sources after block changes | `LightEngine` | `ferraloom-light: room, boundaries, overhang` | pass | pass |
| lighting.sky.sources | Maintain sky-light source heights and propagate skylight through sections | `SkyLightEngine; ChunkSkyLightSources` | `ferraloom-light: open, caves, overhang` | pass | pass |
| lighting.sky.canonical-proto-buffer | Preserve the native per-chunk source buffer across raw bulk writes and use its owned snapshot directly during sky propagation; keep derived caches only for existing callers without a supplied buffer | `ChunkAccess#skyLightSources; ChunkSkyLightSources#update; SkyLightEngine#getChunkSkyLightSources` | `ferraloom-worldgen: region::lighting_tests::raw_bulk_then_checked_write_keeps_canonical_sources_when_light_is_drained` (real native raw roof + checked torch leaves source Y=0 and sky=15 below roof; Rust unrun) | pass | partial |
| lighting.sky.column-gaps | Fill and clear skylight across missing and empty section boundaries | `SkyLightSectionStorage` | `ferraloom-light: gaps` | pass | pass |
| lighting.section.lifecycle | Retain, queue and remove light section data with chunk readiness changes | `LayerLightSectionStorage` | `ferraloom-light: lifecycle, queued_edits` | pass | pass |
| lighting.chunk.initialize | Initialize light sources and light chunks before marking light-correct | `ThreadedLevelLightEngine` | `ferraloom-light: caves (disk reload), open` | pass | pass |
| lighting.chunk.boundaries | Propagate and reconcile light at neighboring chunk and section boundaries | `LevelLightEngine; LightEngine` | `ferraloom-light: boundaries, gaps` | pass | pass |
| lighting.storage.network | Round-trip nibble arrays and expose correct empty and nonempty light masks | `DataLayer; ClientboundLightUpdatePacketData` | | - | todo |
| lighting.thread.order | Order pending light tasks against chunk generation and block changes | `ThreadedLevelLightEngine` | | - | todo |
| lighting.dimension.no-skylight | Disable skylight storage and propagation in dimensions without skylight | `LevelLightEngine` | `ferraloom-light: lifecycle, queued_edits` | pass | pass |
| lighting.storage.nibbles | Lazy/default nibble allocation, fill and byte indexing | `DataLayer` | `ferraloom-light: lazy_fill_and_nibble_order; scenario arrays` | pass | pass |
| lighting.occlusion.faces | Every ordered occlusion-shape pair on all six faces | `LightEngine; Shapes` | `ferraloom-light: all_occlusion_face_pairs_match_vanilla` | pass | pass |
| lighting.sky.empty-boundary | Reset source scan at an entirely empty section below shaped blocks | `ChunkSkyLightSources#findLowestSourceY` | `ferraloom-light: slab_boundary` | pass | pass |
