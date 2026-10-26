use super::{ChunkStatus, WorldGenContext, parse_status, status_name};
use crate::level::chunk::LevelChunk;
use ferraloom_data::{Block, registries::fluid};
use ferraloom_storage::{self as storage, SectionRegistry, SerializableChunkData};
use ferraloom_util::ChunkPos;
use ferraloom_worldgen::{
    Error,
    region::{Heightmap, RegionChunk, SavedTick},
};

/// Packed sections and heightmaps from an unchanged generation-owned chunk.
/// Runtime invalidates this on biome/block changes and synchronizes saved light
/// independently. Metadata such as shared structure flags always stays fresh.
#[derive(Debug)]
pub struct CapturedChunkTerrain {
    sections: Vec<storage::ChunkSection>,
    heightmaps: std::collections::BTreeMap<ferraloom_nbt::NbtString, storage::LongArray>,
    before_terrain: bool,
}
impl CapturedChunkTerrain {
    /// Light propagation changes neither palettes nor heightmaps. Mirror only
    /// the canonical disk section's light, including padding outside build height.
    pub fn sync_light_section(&mut self, section: &storage::ChunkSection) {
        if let Some(cached) = self.sections.iter_mut().find(|s| s.y == section.y) {
            cached.block_light = section.block_light.clone();
            cached.sky_light = section.sky_light.clone();
        } else {
            self.sections.push(section.clone());
            self.sections.sort_by_key(|s| s.y);
        }
    }
}

/// Port of ProtoChunk's owned payload. Keep inherited disk fields until the
/// corresponding runtime subsystem explicitly takes ownership of them.
#[derive(Clone, Debug)]
pub struct ProtoChunk {
    pub region: RegionChunk,
    pub disk: SerializableChunkData,
    pub light_correct: bool,
    pub initial_mob_spawning_pending: bool,
}
impl ProtoChunk {
    pub fn empty(context: &WorldGenContext, pos: ChunkPos) -> Result<Self, Error> {
        Ok(Self {
            region: context.empty_chunk([pos.x, pos.z])?,
            disk: SerializableChunkData {
                data_version: Some(storage::DATA_VERSION),
                x_pos: Some(pos.x),
                z_pos: Some(pos.z),
                y_pos: Some(context.dimension.min_y >> 4),
                status: Some(status_name(ChunkStatus::Empty).into()),
                ..Default::default()
            },
            light_correct: false,
            initial_mob_spawning_pending: false,
        })
    }
    pub fn load(
        context: &WorldGenContext,
        registry: &impl SectionRegistry,
        pos: ChunkPos,
        disk: SerializableChunkData,
    ) -> Result<Self, String> {
        if disk.position() != pos {
            return Err("chunk position differs from storage key".into());
        }
        let status = disk
            .status
            .as_ref()
            .and_then(|s| parse_status(&s.to_string_lossy()))
            .ok_or("unknown saved chunk status")?;
        if disk.below_zero_retrogen.is_some() || disk.blending_data.is_some() {
            return Err("Unsupported: old-world retrogen/blending generation neighborhood".into());
        }
        // SerializableChunkData.parse constructs DataLayers before queueing
        // any neighbor's light. Reject malformed arrays before propagation can
        // replace them with valid output from a neighboring initialization.
        for section in disk.sections.as_deref().unwrap_or_default() {
            for light in [&section.block_light, &section.sky_light]
                .into_iter()
                .flatten()
            {
                if light.0.len() != ferraloom_light::DataLayer::SIZE {
                    return Err("invalid saved light length".into());
                }
            }
        }
        let mut chunk = Self::empty(context, pos).map_err(|e| e.to_string())?;
        for section in disk.sections.as_deref().unwrap_or_default() {
            if let Some(y) = section.y {
                let index = i32::from(y) - (context.dimension.min_y >> 4);
                if let Ok(index) = usize::try_from(index)
                    && index < chunk.region.chunk.sections.len()
                    && let Some(runtime) =
                        section.to_runtime(registry).map_err(|e| e.to_string())?
                {
                    chunk.region.chunk.sections[index] = runtime;
                }
            }
        }
        // Reconstruct WG height caches from the actual sections before placing
        // the RegionChunk around them; stale empty caches would corrupt features.
        let data = context.generator.access().data();
        for (map, surface) in [
            (Heightmap::WorldSurfaceWg, true),
            (Heightmap::OceanFloorWg, false),
        ] {
            let heights = std::array::from_fn(|index| {
                let x = (index & 15) as u8;
                let z = (index >> 4) as u8;
                column_height(&chunk.region.chunk, data, map, x, z)
            });
            if surface {
                chunk.region.chunk.world_surface = heights;
            } else {
                chunk.region.chunk.ocean_floor = heights;
            }
        }
        chunk.region = RegionChunk::new(chunk.region.chunk, status);
        if let Some(structures) = &disk.structures {
            chunk.region.structures = context
                .restore_structures([pos.x, pos.z], &structures.to_nbt())
                .map_err(|e| e.to_string())?;
        }
        for tag in disk.block_entities.as_deref().unwrap_or_default() {
            let position = [
                tag.get_int_or("x", 0),
                tag.get_int_or("y", 0),
                tag.get_int_or("z", 0),
            ];
            chunk.region.block_entities.insert(position, tag.clone());
        }
        chunk.region.entities = disk.entities.clone().unwrap_or_default();
        for tick in disk.block_ticks.as_deref().unwrap_or_default() {
            if let Some(kind) = tick
                .id
                .as_ref()
                .and_then(|id| Block::from_name(&id.to_string_lossy()))
            {
                chunk.region.block_ticks.schedule(SavedTick {
                    position: [
                        tick.x.unwrap_or(0),
                        tick.y.unwrap_or(0),
                        tick.z.unwrap_or(0),
                    ],
                    kind,
                    priority: tick.priority.unwrap_or(0),
                });
            }
        }
        for tick in disk.fluid_ticks.as_deref().unwrap_or_default() {
            if let Some(kind) = tick
                .id
                .as_ref()
                .and_then(|id| fluid::Id::from_name(&id.to_string_lossy()))
            {
                chunk.region.fluid_ticks.schedule(SavedTick {
                    position: [
                        tick.x.unwrap_or(0),
                        tick.y.unwrap_or(0),
                        tick.z.unwrap_or(0),
                    ],
                    kind,
                    priority: tick.priority.unwrap_or(0),
                });
            }
        }
        for (section, offsets) in disk
            .post_processing
            .as_deref()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            for &offset in offsets {
                let offset = offset as u16;
                chunk.region.chunk.post_processing.push([
                    pos.x * 16 + i32::from(offset & 15),
                    context.dimension.min_y + section as i32 * 16 + i32::from((offset >> 4) & 15),
                    pos.z * 16 + i32::from((offset >> 8) & 15),
                ]);
            }
        }
        chunk.light_correct = disk.is_light_on.unwrap_or(false) && status >= ChunkStatus::Light;
        chunk.disk = disk;
        Ok(chunk)
    }
    /// Capture a proto at its real persisted status. FULL disk chunks are never
    /// overwritten by a lower-status proto save.
    pub fn capture(
        &self,
        context: &WorldGenContext,
        registry: &impl SectionRegistry,
    ) -> Result<SerializableChunkData, String> {
        self.capture_with_cached_terrain(context, registry, &mut None)
    }
    /// Reuse serialized terrain, keeping all status/structure/entity/tick data
    /// fresh. The owner must invalidate before biome/block writes, including
    /// partial effects and live FULL refresh, and synchronize light-only changes.
    pub fn capture_with_cached_terrain(
        &self,
        context: &WorldGenContext,
        registry: &impl SectionRegistry,
        terrain: &mut Option<CapturedChunkTerrain>,
    ) -> Result<SerializableChunkData, String> {
        let before_terrain = self.region.status < ChunkStatus::Terrain;
        if terrain
            .as_ref()
            .is_some_and(|cached| cached.before_terrain != before_terrain)
        {
            *terrain = None;
        }
        let mut disk = self.disk.clone();
        disk.status = Some(status_name(self.region.status).into());
        disk.is_light_on = Some(self.light_correct);
        if let Some(cached) = terrain.as_ref() {
            disk.sections = Some(cached.sections.clone());
        } else {
            let mut sections = disk.sections.take().unwrap_or_default();
            for (index, runtime) in self.region.chunk.sections.iter().enumerate() {
                let y = (self.region.chunk.min_y >> 4) + index as i32;
                let entry = if let Some(index) = sections.iter().position(|s| s.y == Some(y as i8))
                {
                    &mut sections[index]
                } else {
                    sections.push(storage::ChunkSection {
                        y: Some(y as i8),
                        ..Default::default()
                    });
                    sections.last_mut().unwrap()
                };
                entry
                    .update_from_runtime(runtime, registry)
                    .map_err(|e| e.to_string())?;
            }
            sections.sort_by_key(|s| s.y);
            disk.sections = Some(sections);
        }
        disk.structures = Some(
            storage::StructureData::from_nbt(
                self.region.structures.to_nbt().map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
        );
        disk.block_entities = Some(
            self.region
                .saved_block_entities()
                .map_err(|e| e.to_string())?,
        );
        disk.entities = Some(self.region.entities.clone());
        // Full chunks keep trigger delays from LevelChunkTicks. ProtoChunkTicks
        // discards time, preserving only insertion order, kind and priority.
        if self.region.status < ChunkStatus::Full {
            disk.block_ticks = Some(
                self.region
                    .block_ticks
                    .ticks()
                    .iter()
                    .map(|t| {
                        retained_tick(
                            self.disk.block_ticks.as_deref(),
                            t.position,
                            t.kind.name(),
                            t.priority,
                        )
                    })
                    .collect(),
            );
            disk.fluid_ticks = Some(
                self.region
                    .fluid_ticks
                    .ticks()
                    .iter()
                    .map(|t| {
                        retained_tick(
                            self.disk.fluid_ticks.as_deref(),
                            t.position,
                            t.kind.name(),
                            t.priority,
                        )
                    })
                    .collect(),
            );
        }
        let mut post_processing = vec![Vec::new(); self.region.chunk.sections.len()];
        for &[x, y, z] in &self.region.chunk.post_processing {
            post_processing[((y - self.region.chunk.min_y) >> 4) as usize]
                .push(((x & 15) | ((y & 15) << 4) | ((z & 15) << 8)) as i16);
        }
        disk.post_processing = Some(post_processing);
        if let Some(cached) = terrain.as_ref() {
            disk.heightmaps = Some(cached.heightmaps.clone());
        } else {
            let maps = disk.heightmaps.get_or_insert_default();
            maps.retain(|name, _| {
                let wg = matches!(
                    name.to_string_lossy().as_str(),
                    "WORLD_SURFACE_WG" | "OCEAN_FLOOR_WG"
                );
                if self.region.status < ChunkStatus::Terrain {
                    wg
                } else {
                    !wg
                }
            });
            let data = context.generator.access().data();
            for (map, name) in [
                (Heightmap::WorldSurfaceWg, "WORLD_SURFACE_WG"),
                (Heightmap::OceanFloorWg, "OCEAN_FLOOR_WG"),
                (Heightmap::WorldSurface, "WORLD_SURFACE"),
                (Heightmap::OceanFloor, "OCEAN_FLOOR"),
                (Heightmap::MotionBlocking, "MOTION_BLOCKING"),
                (
                    Heightmap::MotionBlockingNoLeaves,
                    "MOTION_BLOCKING_NO_LEAVES",
                ),
            ] {
                if (self.region.status < ChunkStatus::Terrain)
                    != matches!(map, Heightmap::WorldSurfaceWg | Heightmap::OceanFloorWg)
                {
                    continue;
                }
                let heights: [u64; 256] = std::array::from_fn(|index| {
                    (column_height(
                        &self.region.chunk,
                        data,
                        map,
                        (index & 15) as u8,
                        (index >> 4) as u8,
                    ) - self.region.chunk.min_y) as u64
                });
                let bits = 32 - (self.region.chunk.height as u32).leading_zeros();
                let per_word = 64 / bits as usize;
                let mut words = vec![0i64; 256usize.div_ceil(per_word)];
                for (index, height) in heights.into_iter().enumerate() {
                    words[index / per_word] |=
                        (height << ((index % per_word) * bits as usize)) as i64;
                }
                maps.insert(name.into(), storage::LongArray(words));
            }
            *terrain = Some(CapturedChunkTerrain {
                sections: disk.sections.as_ref().unwrap().clone(),
                heightmaps: disk.heightmaps.as_ref().unwrap().clone(),
                before_terrain,
            });
        }
        Ok(disk)
    }
}

/// Heightmap#primeHeightmaps starts below empty upper sections. All six native
/// predicates reject air, so lower empty sections can be skipped as well.
fn column_height(
    chunk: &ferraloom_worldgen::generator::GeneratedChunk,
    data: &ferraloom_worldgen::generator::GenerationData,
    map: Heightmap,
    x: u8,
    z: u8,
) -> i32 {
    for (index, section) in chunk.sections.iter().enumerate().rev() {
        if section.has_only_air() {
            continue;
        }
        for y in (0..16).rev() {
            if data.heightmap_matches(map, section.get_block_state(x, y, z)) {
                return chunk.min_y + index as i32 * 16 + i32::from(y) + 1;
            }
        }
    }
    chunk.min_y
}

fn retained_tick(
    saved: Option<&[storage::SavedTick]>,
    position: [i32; 3],
    name: &str,
    priority: i32,
) -> storage::SavedTick {
    saved
        .unwrap_or_default()
        .iter()
        .find(|tick| {
            tick.id
                .as_ref()
                .is_some_and(|id| id.to_string_lossy() == name)
                && [
                    tick.x.unwrap_or(0),
                    tick.y.unwrap_or(0),
                    tick.z.unwrap_or(0),
                ] == position
        })
        .cloned()
        .unwrap_or_else(|| saved_tick(position, name, priority))
}
fn saved_tick(position: [i32; 3], name: &str, priority: i32) -> storage::SavedTick {
    storage::SavedTick {
        id: Some(name.into()),
        x: Some(position[0]),
        y: Some(position[1]),
        z: Some(position[2]),
        delay: Some(0),
        priority: Some(priority),
        ..Default::default()
    }
}
/// Port of LevelChunk(ServerLevel, ProtoChunk, postLoad). Runtime attaches the
/// result before registering tick queues and post-processing callbacks.
pub fn promote(
    pos: ChunkPos,
    sections: Vec<ferraloom_chunk::LevelChunkSection>,
    disk: &SerializableChunkData,
    structures: ferraloom_worldgen::structure::pipeline::ChunkStructures,
    structures_revision: u64,
) -> Result<LevelChunk, crate::level::LevelError> {
    let mut chunk = LevelChunk::from_sections(pos, disk.y_pos.unwrap_or(-4), sections);
    chunk.set_inhabited_time(disk.inhabited_time.unwrap_or(0));
    chunk.load_saved_ticks(disk)?;
    chunk.load_pending_generation_data(disk)?;
    chunk.set_structures(structures, structures_revision)?;
    chunk.mark_unsaved();
    Ok(chunk)
}
