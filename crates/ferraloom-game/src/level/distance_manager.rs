use ferraloom_util::ChunkPos;
/// ChunkLevel.MAX_LEVEL for the 26.3 generation pyramid's radius of 11.
/// Checked against the real LoadingChunkTracker Java vectors. A future
/// generation-pipeline descriptor supplies this radius to the loading tracker.
pub const MAX_LOADED_LEVEL: i32 = 44;
/// Port of ChunkLevel's full, block-ticking and entity-ticking thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FullChunkStatus {
    Inaccessible,
    Full,
    BlockTicking,
    EntityTicking,
}
pub fn full_status(level: i32) -> FullChunkStatus {
    match level {
        i32::MIN..=31 => FullChunkStatus::EntityTicking,
        32 => FullChunkStatus::BlockTicking,
        33 => FullChunkStatus::Full,
        _ => FullChunkStatus::Inaccessible,
    }
}
/// Port of ChunkTrackingView#isWithinDistance, including the neighbor buffer.
pub fn in_view(center: ChunkPos, distance: i32, pos: ChunkPos, neighbors: bool) -> bool {
    let buffer = if neighbors { 2 } else { 1 };
    let x = i64::from((pos.x - center.x).abs().saturating_sub(buffer).max(0));
    let z = i64::from((pos.z - center.z).abs().saturating_sub(buffer).max(0));
    x * x + z * z < i64::from(distance) * i64::from(distance)
}
pub fn tracking_view(center: ChunkPos, distance: i32) -> Vec<ChunkPos> {
    let mut chunks = Vec::new();
    for x in center.x - distance - 1..=center.x + distance + 1 {
        for z in center.z - distance - 1..=center.z + distance + 1 {
            let pos = ChunkPos::new(x, z);
            if in_view(center, distance, pos, true) {
                chunks.push(pos);
            }
        }
    }
    chunks
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rounded_view_and_ticking_levels() {
        let center = ChunkPos::new(0, 0);
        assert!(in_view(center, 2, ChunkPos::new(3, 3), true));
        assert!(!in_view(center, 2, ChunkPos::new(4, 0), true));
        assert!(!in_view(center, 2, ChunkPos::new(3, 3), false));
        assert_eq!(full_status(31), FullChunkStatus::EntityTicking);
        assert_eq!(full_status(34), FullChunkStatus::Inaccessible);
    }
}
