//! Frozen inputs for a center-only generation computation. Preparing the
//! Beardifier observes canonical starts before any later feature-piece mutation.
use super::{ChunkStatus, ChunkStatusTask, ChunkStep, WorldGenContext};
use ferraloom_worldgen::{
    Error,
    generator::ChunkGenerator,
    region::{Heightmap, RegionChunk},
    structure::Beardifier,
};
use std::sync::Arc;

/// Port of the inputs captured by ChunkGenerator#createBiomes and
/// NoiseBasedChunkGenerator#buildTerrain before their background computations.
/// This owns geometry, never mutable starts or a writable neighbor snapshot.
pub struct PreparedCenterStep {
    position: [i32; 2],
    task: CenterTask,
}
enum CenterTask {
    Biomes,
    Terrain(Arc<Beardifier>),
}
impl WorldGenContext {
    /// Prepare while the native logical lane owns its status-valid region.
    /// The caller must finish conflicting piece writes before this observation
    /// or retain its geometry read lease until the center computation returns.
    pub fn prepare_center_step(
        &self,
        chunks: &mut [RegionChunk],
        center: [i32; 2],
        step: &ChunkStep,
    ) -> Result<PreparedCenterStep, Error> {
        if !chunks.iter().any(|chunk| chunk.chunk.position == center) {
            return Err(Error("generation center missing".into()));
        }
        let task = match step.task {
            ChunkStatusTask::GenerateBiomes => CenterTask::Biomes,
            ChunkStatusTask::BuildTerrain => {
                let region = self.region(chunks, center, step)?;
                let order = self
                    .registries
                    .structure_steps
                    .iter()
                    .map(|s| s.id.clone())
                    .collect::<Vec<_>>();
                CenterTask::Terrain(Arc::new(Beardifier::from_region(&region, &order)?))
            }
            _ => return Err(Error("status is not a center computation".into())),
        };
        Ok(PreparedCenterStep {
            position: center,
            task,
        })
    }
}
impl PreparedCenterStep {
    pub fn status(&self) -> ChunkStatus {
        match self.task {
            CenterTask::Biomes => ChunkStatus::Biomes,
            CenterTask::Terrain(_) => ChunkStatus::Terrain,
        }
    }

    /// Execute with exclusive access to only the center. All samplers remain
    /// job-local; returning an error leaves ownership with the caller.
    pub fn apply(self, context: &WorldGenContext, center: &mut RegionChunk) -> Result<(), Error> {
        if center.chunk.position != self.position {
            return Err(Error(
                "prepared generation center differs from target".into(),
            ));
        }
        let status = self.status();
        match self.task {
            CenterTask::Biomes => {
                if let ChunkGenerator::Noise(noise) = context.generator.as_ref() {
                    noise.create_biomes(&mut center.chunk);
                }
            }
            CenterTask::Terrain(beardifier) => {
                let mut generated = match context.generator.as_ref() {
                    ChunkGenerator::Noise(noise) => {
                        let mut generated = center.chunk.clone();
                        let mut noise_chunk = noise.noise_chunk(&generated);
                        noise_chunk.samplers.set_beardifier(Some(beardifier));
                        noise.fill_from_noise(&mut generated, &mut noise_chunk);
                        noise.build_surface(&mut generated, &mut noise_chunk);
                        noise.generate_carvers(&mut generated, &mut noise_chunk, false)?;
                        generated
                    }
                    _ => context.generator.build_terrain(
                        self.position,
                        context.dimension.clone(),
                        Some(beardifier),
                    )?,
                };
                for (section, previous) in generated.sections.iter_mut().zip(&center.chunk.sections)
                {
                    *section.biomes_mut() = previous.biomes().clone();
                }
                let previous = std::mem::replace(center, RegionChunk::new(generated, status));
                center.structures = previous.structures;
                center.block_entities = previous.block_entities;
                center.instantiated_block_entities = previous.instantiated_block_entities;
                center.entities = previous.entities;
                center.block_ticks = previous.block_ticks;
                center.fluid_ticks = previous.fluid_ticks;
                center.poi_changes = previous.poi_changes;
                if matches!(context.generator.as_ref(), ChunkGenerator::Debug(_)) {
                    center.replace_heightmap_snapshots([]);
                }
                // ChunkStatusTasks#buildTerrain's thenApply retains WG maps and
                // primes all four final maps before settling TERRAIN.
                center.prime_heightmaps([
                    Heightmap::WorldSurface,
                    Heightmap::OceanFloor,
                    Heightmap::MotionBlocking,
                    Heightmap::MotionBlockingNoLeaves,
                ]);
            }
        }
        center.status = center.status.max(status);
        Ok(())
    }
}
