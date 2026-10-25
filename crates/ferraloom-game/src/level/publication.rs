//! Main-owner publication boundary for an owned, prepared FULL chunk.
use super::{
    Level, LevelError,
    acquisition::{ChunkKind, ChunkReference},
    chunk::LevelChunk,
    weather::ChunkLightSnapshot,
};
use crate::ticks::LevelChunkTicks;
use ferraloom_data::registries::fluid::Id as Fluid;
use ferraloom_nbt::CompoundTag;
use std::{
    cell::{Cell, RefCell},
    fmt,
};

/// Produced by the pipeline after native LIGHT and source-order metadata import.
/// The pipeline retains entity post-load ownership after staging the center.
pub struct PreparedFullChunk {
    pub chunk: LevelChunk,
    pub light: ChunkLightSnapshot,
    pub worldgen_entities: Vec<CompoundTag>,
}

/// A staging error precedes canonical insertion and owns the original input.
pub struct ChunkPublicationError {
    pub source: LevelError,
    pub prepared: PreparedFullChunk,
}

/// Identity of the center installed by ChunkStatusTasks#full, before its FULL
/// future settles. A reference is identity, not permission for a FULL read.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct FullChunkStage {
    reference: ChunkReference,
    // Safe address identity of the stable canonical record, never dereferenced.
    // Per-region generations alone could alias a token from a different Level.
    identity: usize,
}
impl fmt::Debug for FullChunkStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("FullChunkStage")
            .field(&self.reference)
            .finish()
    }
}
impl FullChunkStage {
    pub fn reference(self) -> ChunkReference {
        self.reference
    }
}

/// The reachable runtime continuation owns this input and its entity checkpoint.
/// Entity Publishing failures retain their actual WorldgenEntityLoadError there.
#[derive(Debug)]
pub struct StagedFullChunk {
    pub token: FullChunkStage,
    pub worldgen_entities: Vec<CompoundTag>,
}

/// Native source-order checkpoints after runPostLoad. Unavailable means that
/// the supplied identity no longer names a canonical staged center.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FullChunkFinalizationStep {
    Unavailable,
    Staged,
    Loaded,
    BlockEntitiesRegistered,
    BlockTicksRegistered,
    FluidTicksRegistered,
    Finalized,
}
#[derive(Debug, Eq, PartialEq)]
pub enum ChunkFinalizationFailure {
    AlreadyFinalized,
    StaleStage,
    Registration(LevelError),
}
#[derive(Debug, Eq, PartialEq)]
pub struct ChunkFinalizationError {
    pub token: FullChunkStage,
    pub checkpoint: FullChunkFinalizationStep,
    pub failure: ChunkFinalizationFailure,
}
impl fmt::Display for ChunkFinalizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FULL finalization at {:?}, {:?}: {:?}",
            self.token.reference.pos, self.checkpoint, self.failure
        )
    }
}
impl std::error::Error for ChunkFinalizationError {}

/// Only cold staging/finalization/save paths borrow pending queues. Block reads
/// and normal scheduled ticks never borrow this record. No guard spans a call.
pub(super) struct StagedPublication {
    reference: ChunkReference,
    pub(super) step: Cell<FullChunkFinalizationStep>,
    pub(super) block_ticks: RefCell<Option<LevelChunkTicks>>,
    pub(super) fluid_ticks: RefCell<Option<LevelChunkTicks<Fluid>>>,
}
impl fmt::Debug for ChunkPublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChunkPublicationError")
            .field("source", &self.source)
            .field("pos", &self.prepared.chunk.pos())
            .finish()
    }
}

impl Level {
    /// Port of LevelChunk's proto constructor and holder.replaceProtoChunk.
    /// Installs center/light only. The caller replaces parent futures, executes
    /// shared postLoad once without guards, then commits entities_registered.
    pub fn stage_full_chunk(
        &self,
        mut prepared: PreparedFullChunk,
    ) -> Result<StagedFullChunk, Box<ChunkPublicationError>> {
        if let Err(source) = self.validate_full_publication(&prepared) {
            return Err(Box::new(ChunkPublicationError { source, prepared }));
        }
        let region = self.region();
        let pos = prepared.chunk.pos();
        prepared
            .chunk
            .prime_heightmaps(self.block_tags().expect("publication validated tags"));
        let generation = region
            .next_chunk_generation
            .get()
            .checked_add(1)
            .expect("chunk identities exhausted");
        region.next_chunk_generation.set(generation);
        prepared.chunk.publication_generation = generation;
        prepared.chunk.loaded.set(false);
        region
            .block_entity_state
            .prepare_published_chunk(&mut prepared.chunk, region.game_time);
        let reference = ChunkReference {
            pos,
            generation,
            kind: ChunkKind::Full,
        };
        prepared.chunk.staged_publication = Some(StagedPublication {
            reference,
            step: Cell::new(FullChunkFinalizationStep::Staged),
            block_ticks: RefCell::new(Some(std::mem::take(&mut prepared.chunk.block_ticks))),
            fluid_ticks: RefCell::new(Some(std::mem::take(&mut prepared.chunk.fluid_ticks))),
        });
        self.publish_light_snapshot(pos, prepared.light);
        let chunk = region
            .chunks
            .publish(pos, prepared.chunk)
            .unwrap_or_else(|_| panic!("validated center publication"));
        Ok(StagedFullChunk {
            token: FullChunkStage {
                reference,
                identity: std::ptr::from_ref(chunk) as usize,
            },
            worldgen_entities: prepared.worldgen_entities,
        })
    }

    /// Port of setLoaded -> registerAllBlockEntitiesAfterLevelLoad ->
    /// registerTickContainerInLevel -> setUnsavedListener. Invoke only after the
    /// continuation's successful, permanent entity postLoad checkpoint.
    /// Failed registration retains each completed step; it never replays entities.
    pub fn finalize_full_chunk(
        &self,
        token: FullChunkStage,
    ) -> Result<ChunkReference, ChunkFinalizationError> {
        let reference = token.reference;
        let error = |checkpoint, failure| ChunkFinalizationError {
            token,
            checkpoint,
            failure,
        };
        let Some(chunk) = self.chunk(reference.pos).filter(|chunk| {
            chunk.publication_generation == reference.generation
                && std::ptr::from_ref(*chunk) as usize == token.identity
        }) else {
            return Err(error(
                FullChunkFinalizationStep::Unavailable,
                ChunkFinalizationFailure::StaleStage,
            ));
        };
        let Some(stage) = chunk
            .staged_publication
            .as_ref()
            .filter(|stage| stage.reference == reference)
        else {
            return Err(error(
                FullChunkFinalizationStep::Unavailable,
                ChunkFinalizationFailure::StaleStage,
            ));
        };
        if stage.step.get() == FullChunkFinalizationStep::Finalized {
            return Err(error(
                stage.step.get(),
                ChunkFinalizationFailure::AlreadyFinalized,
            ));
        }
        if stage.step.get() == FullChunkFinalizationStep::Staged {
            chunk.loaded.set(true);
            stage.step.set(FullChunkFinalizationStep::Loaded);
        }
        if stage.step.get() == FullChunkFinalizationStep::Loaded {
            self.region()
                .block_entity_state
                .register_published_chunk(chunk);
            stage
                .step
                .set(FullChunkFinalizationStep::BlockEntitiesRegistered);
        }
        if stage.step.get() == FullChunkFinalizationStep::BlockEntitiesRegistered {
            let ticks = stage
                .block_ticks
                .borrow_mut()
                .take()
                .expect("unregistered staged block ticks");
            if let Err(ticks) = self
                .region()
                .block_ticks
                .publish_container(reference.pos, ticks)
            {
                *stage.block_ticks.borrow_mut() = Some(ticks);
                return Err(error(
                    stage.step.get(),
                    ChunkFinalizationFailure::Registration(LevelError::DuplicateChunk(
                        reference.pos,
                    )),
                ));
            }
            stage
                .step
                .set(FullChunkFinalizationStep::BlockTicksRegistered);
        }
        if stage.step.get() == FullChunkFinalizationStep::BlockTicksRegistered {
            let ticks = stage
                .fluid_ticks
                .borrow_mut()
                .take()
                .expect("unregistered staged fluid ticks");
            if let Err(ticks) = self
                .region()
                .fluid_ticks
                .publish_container(reference.pos, ticks)
            {
                *stage.fluid_ticks.borrow_mut() = Some(ticks);
                return Err(error(
                    stage.step.get(),
                    ChunkFinalizationFailure::Registration(LevelError::DuplicateChunk(
                        reference.pos,
                    )),
                ));
            }
            stage
                .step
                .set(FullChunkFinalizationStep::FluidTicksRegistered);
        }
        // This port's canonical dirty marker is already installed in LevelChunk;
        // setting the listener requires no gameplay callback or fallback action.
        stage.step.set(FullChunkFinalizationStep::Finalized);
        Ok(reference)
    }

    fn validate_full_publication(&self, prepared: &PreparedFullChunk) -> Result<(), LevelError> {
        let region = self.region();
        let pos = prepared.chunk.pos();
        if region.chunks.contains_key(&pos)
            || region.block_ticks.container(pos).is_some()
            || region.fluid_ticks.container(pos).is_some()
        {
            return Err(LevelError::DuplicateChunk(pos));
        }
        if prepared.chunk.min_y() != region.min_y || prepared.chunk.max_y() != region.max_y {
            return Err(LevelError::WrongChunkHeight(pos));
        }
        if prepared
            .chunk
            .staged_publication
            .as_ref()
            .is_some_and(|stage| {
                stage.block_ticks.borrow().is_some() || stage.fluid_ticks.borrow().is_some()
            })
        {
            return Err(LevelError::InvalidPendingGenerationData);
        }
        self.block_tags()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
