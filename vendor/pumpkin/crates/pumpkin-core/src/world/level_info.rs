use arc_swap::ArcSwap;
use pumpkin_world::world_info::LevelData;
use std::sync::Arc;

/// Retry a level-info edit against the latest snapshot until it commits.
/// Edits must be side-effect free: RCU may retry them. Send feedback and run
/// callbacks only after this function returns. Returns the committed previous snapshot.
pub fn update_level_info(
    level_info: &ArcSwap<LevelData>,
    edit: impl FnMut(&mut LevelData),
) -> Arc<LevelData> {
    update_level_info_if(level_info, |_| true, edit)
}

/// Check the condition on every retry, retaining the snapshot if it declines.
/// Returns the previous snapshot from the successful CAS even when unchanged;
/// callers can check the same condition on it before running side effects.
/// Both closures must be side-effect free.
pub fn update_level_info_if(
    level_info: &ArcSwap<LevelData>,
    mut condition: impl FnMut(&LevelData) -> bool,
    mut edit: impl FnMut(&mut LevelData),
) -> Arc<LevelData> {
    level_info.rcu(|current| {
        if !condition(current) {
            return current.clone();
        }
        let mut updated = (**current).clone();
        edit(&mut updated);
        Arc::new(updated)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_util::{Difficulty, world_seed::Seed};
    use std::{
        sync::{Arc, mpsc},
        thread,
        time::Duration,
    };

    const WAIT: Duration = Duration::from_secs(10);

    #[test]
    fn retried_rule_edit_preserves_other_rules_and_level_fields() {
        let mut initial = LevelData::default(Seed(42));
        initial.spawn_x = -47;
        initial.day_time = 12345;
        initial.map_id = 99;
        let shared = Arc::new(ArcSwap::from_pointee(initial));
        let writer = shared.clone();
        let (loaded_tx, loaded_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();

        let delayed = thread::spawn(move || {
            let mut attempts = 0;
            update_level_info(&writer, |info| {
                attempts += 1;
                if attempts == 1 {
                    // The production helper has cloned the old snapshot. Force
                    // another commit before its first compare-and-swap.
                    loaded_tx.send(()).unwrap();
                    resume_rx.recv_timeout(WAIT).unwrap();
                }
                info.game_rules.keep_inventory = true;
            });
            attempts
        });

        loaded_rx.recv_timeout(WAIT).unwrap();
        update_level_info(&shared, |info| {
            info.game_rules.random_tick_speed = 11;
            info.difficulty = Difficulty::Hard;
            info.difficulty_locked = true;
        });
        resume_tx.send(()).unwrap();
        assert_eq!(delayed.join().unwrap(), 2);

        let info = shared.load();
        assert!(info.game_rules.keep_inventory);
        assert_eq!(info.game_rules.random_tick_speed, 11);
        assert_eq!(info.difficulty, Difficulty::Hard);
        assert!(info.difficulty_locked);
        assert_eq!((info.spawn_x, info.day_time, info.map_id), (-47, 12345, 99));
        assert_eq!(info.world_gen_settings.seed, 42);
    }

    #[test]
    fn retried_same_rule_edit_follows_commit_order() {
        let mut initial = LevelData::default(Seed(42));
        initial.spawn_z = 81;
        let shared = Arc::new(ArcSwap::from_pointee(initial));
        let writer = shared.clone();
        let (loaded_tx, loaded_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();

        let delayed = thread::spawn(move || {
            let mut attempts = 0;
            update_level_info(&writer, |info| {
                attempts += 1;
                if attempts == 1 {
                    loaded_tx.send(()).unwrap();
                    resume_rx.recv_timeout(WAIT).unwrap();
                }
                info.game_rules.random_tick_speed = 7;
            });
            attempts
        });

        loaded_rx.recv_timeout(WAIT).unwrap();
        update_level_info(&shared, |info| {
            info.game_rules.random_tick_speed = 11;
            info.game_rules.keep_inventory = true;
        });
        let first_commit = shared.load_full();
        assert_eq!(first_commit.game_rules.random_tick_speed, 11);
        resume_tx.send(()).unwrap();
        assert_eq!(delayed.join().unwrap(), 2);

        let last_commit = shared.load();
        assert_eq!(last_commit.game_rules.random_tick_speed, 7);
        assert!(last_commit.game_rules.keep_inventory);
        assert_eq!(last_commit.spawn_z, 81);
        // Readers holding an earlier committed snapshot keep a coherent value.
        assert_eq!(first_commit.game_rules.random_tick_speed, 11);
    }
}
