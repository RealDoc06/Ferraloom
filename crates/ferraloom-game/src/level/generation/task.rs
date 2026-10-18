use super::{ChunkPyramid, ChunkStatus, STATUSES};
use ferraloom_util::ChunkPos;

/// A failed dependency cancels its consumers; it never becomes an empty chunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerationError { Cancelled, Failed { position: ChunkPos, status: ChunkStatus, message: String } }

/// Port of GenerationChunkHolder's status futures, kept separate from ticket
/// eligibility. A persisted status and completed runtime hooks are distinct.
#[derive(Debug)]
pub struct GenerationChunkHolder<T> {
    pub chunk: T,
    pub persisted: ChunkStatus,
    pub completed: ChunkStatus,
    pub failure: Option<GenerationError>,
}
impl<T> GenerationChunkHolder<T> {
    pub fn new(chunk: T, persisted: ChunkStatus) -> Self { Self { chunk, persisted, completed: ChunkStatus::Empty, failure: None } }
    pub fn complete(&mut self, status: ChunkStatus) { self.completed = self.completed.max(status); self.persisted = self.persisted.max(status); }
    pub fn step_to(&self, status: ChunkStatus) -> &super::ChunkStep {
        if self.persisted < status { ChunkPyramid::generation() } else { ChunkPyramid::loading() }.step_to(status)
    }
}

#[derive(Clone, Debug)]
pub struct GenerationLayer {
    pub status: ChunkStatus,
    pub positions: Vec<ChunkPos>,
}
/// Port of ChunkGenerationTask. Layers are X-major, and a layer must finish
/// before the next is scheduled. Cancellation waits for already-submitted work.
#[derive(Debug)]
pub struct ChunkGenerationTask {
    pub request: u64,
    pub center: ChunkPos,
    pub target: ChunkStatus,
    scheduled: Option<ChunkStatus>,
    needs_generation: bool,
    cancelled: bool,
}
impl ChunkGenerationTask {
    pub fn new(request: u64, center: ChunkPos, target: ChunkStatus) -> Self { Self { request, center, target, scheduled: None, needs_generation: false, cancelled: false } }
    pub fn cancel(&mut self) { self.cancelled = true; }
    pub fn is_cancelled(&self) -> bool { self.cancelled }
    /// Call only after the previous layer's futures have all succeeded. The
    /// first EMPTY loads the loading footprint; a failed disk-only check repeats
    /// EMPTY with the generation footprint, exactly as vanilla does.
    pub fn next_layer(&mut self, mut persisted: impl FnMut(ChunkPos) -> Option<ChunkStatus>) -> Result<Option<GenerationLayer>, GenerationError> {
        if self.cancelled { return Err(GenerationError::Cancelled); }
        if self.scheduled == Some(self.target) { return Ok(None); }
        let status = match self.scheduled {
            None => ChunkStatus::Empty,
            Some(ChunkStatus::Empty) if !self.needs_generation && !self.can_load(&mut persisted) => { self.needs_generation = true; ChunkStatus::Empty }
            Some(previous) => STATUSES[previous as usize + 1],
        };
        self.scheduled = Some(status);
        let pyramid = if self.needs_generation { ChunkPyramid::generation() } else { ChunkPyramid::loading() };
        let radius = pyramid.step_to(self.target).accumulated_radius_of(status).expect("layer at or below target") as i32;
        Ok(Some(GenerationLayer { status, positions: square(self.center, radius).collect() }))
    }
    fn can_load(&self, persisted: &mut impl FnMut(ChunkPos) -> Option<ChunkStatus>) -> bool {
        if self.target == ChunkStatus::Empty { return true; }
        if persisted(self.center).is_none_or(|s| s < self.target) { return false; }
        let dependencies = &ChunkPyramid::loading().step_to(self.target).accumulated;
        square(self.center, dependencies.radius().unwrap_or(0) as i32).all(|pos| {
            let distance = pos.x.abs_diff(self.center.x).max(pos.z.abs_diff(self.center.z)) as usize;
            persisted(pos).is_some_and(|status| Some(status) >= dependencies.get(distance))
        })
    }
}
fn square(center: ChunkPos, radius: i32) -> impl Iterator<Item = ChunkPos> {
    (center.x - radius..=center.x + radius).flat_map(move |x| (center.z - radius..=center.z + radius).map(move |z| ChunkPos::new(x, z)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_full_uses_loading_footprint_but_a_missing_neighbor_expands_generation() {
        let mut task = ChunkGenerationTask::new(0, ChunkPos::new(2, -1), ChunkStatus::Full);
        assert_eq!(task.next_layer(|_| None).unwrap().unwrap().positions.len(), 9);
        assert_eq!(task.next_layer(|_| None).unwrap().unwrap().positions.len(), 529);
        assert_eq!(task.next_layer(|_| None).unwrap().unwrap().status, ChunkStatus::StructureStarts);
        let mut loaded = ChunkGenerationTask::new(1, task.center, ChunkStatus::Full);
        loaded.next_layer(|_| Some(ChunkStatus::Full)).unwrap();
        let layer = loaded.next_layer(|_| Some(ChunkStatus::Full)).unwrap().unwrap();
        assert_eq!(layer.status, ChunkStatus::StructureStarts);
        assert_eq!(layer.positions[0], ChunkPos::new(1, -2));
        assert_eq!(layer.positions[1], ChunkPos::new(1, -1));
        loaded.cancel();
        assert_eq!(loaded.next_layer(|_| None).unwrap_err(), GenerationError::Cancelled);
    }
}
