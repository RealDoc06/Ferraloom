# Chunks and tickets

Mechanics are identified from the 26.3 server source. Registry and resource entries are mechanically enumerated from the official reports, bootstrapped registries and built-in datapack.

| ID | Feature | Vanilla source | Test | Vanilla | Ferraloom |
|---|---|---|---|---|---|
| chunks.pipeline.dependencies | Generate chunk statuses with their required neighbor status and radius dependencies | `ChunkPyramid; ChunkStep; ChunkGenerationTask` | `generation::pyramid::tests::vanilla_pyramids_match_every_direct_and_accumulated_dependency`; `generation::task::tests::saved_full_uses_loading_footprint_but_a_missing_neighbor_expands_generation`; `generation::range::tests::accessible_needs_initialized_neighbors_but_ticking_requires_full_neighbors` | pass | partial |
| chunks.pipeline.loading | Distinguish loading an existing chunk from generating missing stages | `ChunkStatusTasks; GenerationChunkHolder` | `generation::task::tests::saved_full_uses_loading_footprint_but_a_missing_neighbor_expands_generation`; live proto reload and full promotion preserve saved ticks/NBT; old-world blending and retrogen pending | pass | partial |
| chunks.pipeline.light-full | Promote chunks through light and full states before exposing ticking access | `ChunkStatusTasks; ChunkMap` | `join::offline_client_joins_walks_chats_and_saves` | - | partial |
| chunks.tickets.level-propagation | Propagate ticket distance levels and choose loaded, block-ticking and entity-ticking states | `DistanceManager; ChunkLevel` | | - | todo |
| chunks.tickets.expiry | Expire timeout-based tickets before distance-manager updates | `TicketStorage#purgeStaleTickets; ServerChunkCache#tick` | | - | todo |
| chunks.tickets.player-view | Update player view-distance tracking and pending chunk sends after movement | `ChunkMap#move; PlayerChunkSender` | `join::offline_client_joins_walks_chats_and_saves`; `ferraloom-parity::starter::parity_chunk_loading_while_walking` | pass | partial |
| chunks.tickets.simulation-distance | Use simulation distance independently from view distance for entity and block ticking | `DistanceManager; ChunkMap` | | - | todo |
| chunks.tickets.forced | Add and remove persistent forced-chunk tickets | `ServerLevel#setChunkForced; ForceLoadCommand` | | - | todo |
| chunks.tickets.portal | Keep destination portal chunks loaded while portal travel completes | `PortalForcer; TicketType` | | - | todo |
| chunks.tickets.ender-pearl | Maintain owner-linked ender pearl tickets across entity ticks and loading | `ThrownEnderpearl` | | - | todo |
| chunks.unload.safe | Defer chunk unload until pending saves and work are safe to complete | `ChunkMap#processUnloads` | | - | todo |
| chunks.unload.entities | Persist and unload entity sections as chunk tracking visibility changes | `PersistentEntitySectionManager` | | - | todo |
| chunks.save.dirty | Save dirty chunks and avoid overwriting full chunks with protochunks | `ChunkMap#save` | `join::offline_client_joins_walks_chats_and_saves` | - | partial |
| chunks.network.batch-feedback | Adapt chunk batch sending using client batch feedback and pending acknowledgements | `PlayerChunkSender` | `chunk_map::tests::batches_wait_for_acknowledgement` | - | partial |
| chunks.network.block-light-updates | Batch changed blocks, sections and light data for chunk watchers | `ChunkHolder#broadcastChanges` | | - | todo |
| chunks.tracking.entities | Track entity range, passengers, metadata, movement deltas and removal per player | `ChunkMap; ServerEntity#sendChanges` | | - | todo |
| chunks.tracking.chunk-cache | Clear the immediate chunk lookup cache after chunk-source tick | `ServerChunkCache#clearCache` | | - | todo |
| chunks.chunk-status.biomes | `minecraft:biomes` (chunk status) | `net.minecraft.world.level.chunk.status.ChunkStatus#register; net.minecraft.world.level.chunk.status.ChunkStatus` | | - | todo |
| chunks.chunk-status.empty | `minecraft:empty` (chunk status) | `net.minecraft.world.level.chunk.status.ChunkStatus#register; net.minecraft.world.level.chunk.status.ChunkStatus` | | - | todo |
| chunks.chunk-status.features | `minecraft:features` (chunk status) | `net.minecraft.world.level.chunk.status.ChunkStatus#register; net.minecraft.world.level.chunk.status.ChunkStatus` | | - | todo |
| chunks.chunk-status.full | `minecraft:full` (chunk status) | `net.minecraft.world.level.chunk.status.ChunkStatus#register; net.minecraft.world.level.chunk.status.ChunkStatus` | | - | todo |
| chunks.chunk-status.initialize-light | `minecraft:initialize_light` (chunk status) | `net.minecraft.world.level.chunk.status.ChunkStatus#register; net.minecraft.world.level.chunk.status.ChunkStatus` | | - | todo |
| chunks.chunk-status.light | `minecraft:light` (chunk status) | `net.minecraft.world.level.chunk.status.ChunkStatus#register; net.minecraft.world.level.chunk.status.ChunkStatus` | | - | todo |
| chunks.chunk-status.spawn | `minecraft:spawn` (chunk status) | `net.minecraft.world.level.chunk.status.ChunkStatus#register; net.minecraft.world.level.chunk.status.ChunkStatus` | | - | todo |
| chunks.chunk-status.structure-references | `minecraft:structure_references` (chunk status) | `net.minecraft.world.level.chunk.status.ChunkStatus#register; net.minecraft.world.level.chunk.status.ChunkStatus` | | - | todo |
| chunks.chunk-status.structure-starts | `minecraft:structure_starts` (chunk status) | `net.minecraft.world.level.chunk.status.ChunkStatus#register; net.minecraft.world.level.chunk.status.ChunkStatus` | | - | todo |
| chunks.chunk-status.terrain | `minecraft:terrain` (chunk status) | `net.minecraft.world.level.chunk.status.ChunkStatus#register; net.minecraft.world.level.chunk.status.ChunkStatus` | | - | todo |
| chunks.ticket-type.dragon | `minecraft:dragon` (ticket type) | `net.minecraft.server.level.TicketType#register` | | - | todo |
| chunks.ticket-type.ender-pearl | `minecraft:ender_pearl` (ticket type) | `net.minecraft.server.level.TicketType#register` | | - | todo |
| chunks.ticket-type.forced | `minecraft:forced` (ticket type) | `net.minecraft.server.level.TicketType#register` | | - | todo |
| chunks.ticket-type.player-loading | `minecraft:player_loading` (ticket type) | `net.minecraft.server.level.TicketType#register` | | - | todo |
| chunks.ticket-type.player-simulation | `minecraft:player_simulation` (ticket type) | `net.minecraft.server.level.TicketType#register` | | - | todo |
| chunks.ticket-type.player-spawn | `minecraft:player_spawn` (ticket type) | `net.minecraft.server.level.TicketType#register` | | - | todo |
| chunks.ticket-type.portal | `minecraft:portal` (ticket type) | `net.minecraft.server.level.TicketType#register` | | - | todo |
| chunks.ticket-type.spawn-search | `minecraft:spawn_search` (ticket type) | `net.minecraft.server.level.TicketType#register` | | - | todo |
| chunks.ticket-type.unknown | `minecraft:unknown` (ticket type) | `net.minecraft.server.level.TicketType#register` | | - | todo |

| chunks.runtime.batch-quota | Apply PlayerChunkSender quota and acknowledgement window; pending-set tie traversal awaits shared collections | `PlayerChunkSender#sendNextChunks; #onChunkBatchReceivedByClient` | `chunk_map::tests::batches_wait_for_acknowledgement` | - | partial |

| chunks.pipeline.warn-units | Strict fails missing game callbacks; Warn abandons one placed feature or structure piece, retaining partial writes and reporting shared counters | `ChunkGenerator#applyBiomeDecoration; StructureStart#placeInChunk` | `generation::default_noise_generation_warn_reaches_full_and_strict_rejects_missing_behavior` | - | partial |
