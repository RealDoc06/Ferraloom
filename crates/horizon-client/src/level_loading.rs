/// `LevelLoadProgressTracker.EXPECTED_PLAYER_CHUNKS`: the 7x7 chunks
/// `PrepareSpawnTask` waits for around the player.
const PLAYER_CHUNKS: usize = 49;
/// `LevelLoadProgressTracker.PREPARE_SERVER_WEIGHT`.
const PREPARE_WEIGHT: usize = 10;

/// `LevelLoadingScreen.Reason`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    NetherPortal,
    EndPortal,
    Other,
}

/// `LevelLoadProgressTracker.get` once the server is prepared, with `ready`
/// of the player's 49 chunks loaded (no forced chunks to load first).
pub fn progress(ready: usize) -> f32 {
    let total = PREPARE_WEIGHT + PLAYER_CHUNKS;
    (PREPARE_WEIGHT + ready.min(PLAYER_CHUNKS)) as f32 / total as f32
}

/// `LevelLoadingScreen.tick`: the bar eases a fifth of the way each tick.
pub fn smooth(smoothed: f32, progress: f32) -> f32 {
    smoothed + (progress - smoothed) * 0.2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_weighs_the_prepared_server_and_the_player_chunks() {
        assert_eq!(progress(0), 10.0 / 59.0);
        assert_eq!(progress(49), 1.0);
        assert_eq!(progress(100), 1.0);
        let eased = smooth(0.0, 1.0);
        assert_eq!(eased, 0.2);
        assert!((smooth(eased, 1.0) - 0.36).abs() < 1e-6);
    }
}
