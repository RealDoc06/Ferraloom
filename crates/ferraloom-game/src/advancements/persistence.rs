//! Player-local advancement/stat IO. Capture serialized values at the player
//! save call, then move only those owned bytes to the existing IO worker.
use super::{PlayerAdvancements, Result, ServerAdvancementManager};
use crate::{
    entity::player::ServerPlayer,
    loot::{LootError, codec},
    stats::ServerStatsCounter,
};
use ferraloom_protocol::Uuid;
use jiff::{Timestamp, tz::TimeZone};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(crate) enum PlayerProgressDirectory {
    Advancements,
    Stats,
}
/// Port of LevelResource.PLAYER_ADVANCEMENTS_DIR / PLAYER_STATS_DIR and the
/// PlayerList UUID filename. Both resources live under the native `players` root.
pub(crate) fn player_progress_path(
    world: &Path,
    uuid: Uuid,
    directory: PlayerProgressDirectory,
) -> PathBuf {
    world
        .join("players")
        .join(match directory {
            PlayerProgressDirectory::Advancements => "advancements",
            PlayerProgressDirectory::Stats => "stats",
        })
        .join(format!("{uuid}.json"))
}

pub struct LoadedPlayerProgress {
    pub advancements: PlayerAdvancements,
    pub stats: ServerStatsCounter,
    pub diagnostics: Vec<String>,
}
/// Port of PlayerList#getPlayerAdvancements and #getPlayerStats. Unsupported
/// data versions propagate; codec/IO errors retain the native empty state and
/// are reported to the caller for logging, as vanilla's loaders do.
pub fn load_player_progress(
    world: &Path,
    uuid: Uuid,
    manager: &ServerAdvancementManager,
) -> Result<LoadedPlayerProgress> {
    let mut advancements = PlayerAdvancements::new(manager);
    let mut diagnostics = Vec::new();
    match advancements.load_file(manager, &PlayerAdvancements::save_path(world, uuid)) {
        Ok(unknown) => diagnostics.extend(unknown.into_iter().map(|id| {
            format!("Ignored advancement '{id}' in progress file {uuid}: it doesn't exist anymore")
        })),
        Err(error @ LootError::Unsupported(_)) => return Err(error),
        Err(error) => {
            diagnostics.push(format!(
                "Couldn't parse player advancements for {uuid}: {error}"
            ));
            advancements = PlayerAdvancements::new(manager);
        }
    }
    let stats = match ServerStatsCounter::load_file(&ServerStatsCounter::save_path(world, uuid)) {
        Ok((stats, problems)) => {
            diagnostics.extend(problems);
            stats
        }
        Err(error @ LootError::Unsupported(_)) => return Err(error),
        Err(error) => {
            diagnostics.push(format!(
                "Couldn't parse statistics file for {uuid}: {error}"
            ));
            ServerStatsCounter::default()
        }
    };
    Ok(LoadedPlayerProgress {
        advancements,
        stats,
        diagnostics,
    })
}
/// Immutable serialization captured before any asynchronous save begins.
pub struct PlayerProgressSave {
    advancement_path: PathBuf,
    advancements: Vec<u8>,
    stats_path: PathBuf,
    stats: Vec<u8>,
}
impl PlayerProgressSave {
    /// Production-only ZoneId.systemDefault lookup. Tests pass explicit zones
    /// to capture_in_zone, never the host's zone or environment variables.
    pub fn capture(player: &ServerPlayer, world: &Path) -> Result<Self> {
        let zone = TimeZone::try_system().map_err(|error| {
            LootError::Unsupported(format!("advancement.system_time_zone: {error}"))
        })?;
        Self::capture_in_zone(player, world, &zone)
    }
    pub fn capture_in_zone(player: &ServerPlayer, world: &Path, zone: &TimeZone) -> Result<Self> {
        let mut offsets = BTreeMap::new();
        for (_, progress) in &player.advancements.progress {
            for (_, criterion) in progress.criteria.iter() {
                if let Some(time) = criterion.obtained {
                    let instant =
                        Timestamp::new(time.seconds, time.nanos as i32).map_err(|error| {
                            LootError::Unsupported(format!(
                                "advancement.time_zone_instant: {error}"
                            ))
                        })?;
                    offsets.insert(time.seconds, zone.to_offset(instant).seconds());
                }
            }
        }
        let advancements = player.advancements.to_json(|time| offsets[&time.seconds])?;
        Ok(Self {
            advancement_path: PlayerAdvancements::save_path(world, player.profile_id()),
            advancements: serde_json::to_vec_pretty(&advancements)
                .map_err(|error| codec::error(error.to_string()))?,
            stats_path: ServerStatsCounter::save_path(world, player.profile_id()),
            stats: serde_json::to_vec_pretty(&player.stats.to_json())
                .map_err(|error| codec::error(error.to_string()))?,
        })
    }
    /// Called in the player-save worker, alongside player NBT persistence.
    pub fn write(self) -> Result<()> {
        for (path, bytes) in [
            (self.stats_path, self.stats),
            (self.advancement_path, self.advancements),
        ] {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| codec::error(error.to_string()))?;
            }
            std::fs::write(path, bytes).map_err(|error| codec::error(error.to_string()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        advancements::{Advancement, AdvancementHolder, ObtainedTime},
        entity::player::GameType,
    };
    use ferraloom_protocol::Identifier;
    use ferraloom_util::Vec3;
    use serde_json::json;

    #[test]
    fn captured_save_uses_each_instant_offset_across_dst() {
        let manager = ServerAdvancementManager::new(vec![AdvancementHolder {
            id: Identifier::parse("test:dst").unwrap(),
            value: Advancement::decode(
                &json!({"criteria":{"before":{"trigger":"tick"},"after":{"trigger":"tick"}}}),
            )
            .unwrap(),
        }]);
        let mut player =
            ServerPlayer::new(Uuid::NIL, "DST".into(), 1, Vec3::ZERO, GameType::Survival);
        player.advancements = PlayerAdvancements::new(&manager);
        for (criterion, timestamp) in [
            ("before", "2024-03-10T06:59:59Z"),
            ("after", "2024-03-10T07:00:00Z"),
        ] {
            let timestamp: Timestamp = timestamp.parse().unwrap();
            player.advancements.award_immediate(
                &manager,
                &manager.advancements[0].id,
                criterion,
                ObtainedTime {
                    seconds: timestamp.as_second(),
                    nanos: 0,
                },
            );
        }
        let zone = TimeZone::posix("EST5EDT,M3.2.0,M11.1.0").unwrap();
        let save = PlayerProgressSave::capture_in_zone(&player, Path::new("world"), &zone).unwrap();
        assert_eq!(
            save.advancement_path,
            Path::new("world/players/advancements/00000000-0000-0000-0000-000000000000.json")
        );
        assert_eq!(
            save.stats_path,
            Path::new("world/players/stats/00000000-0000-0000-0000-000000000000.json")
        );
        let value: serde_json::Value = serde_json::from_slice(&save.advancements).unwrap();
        assert_eq!(
            value["test:dst"]["criteria"]["before"],
            "2024-03-10 01:59:59 -0500"
        );
        assert_eq!(
            value["test:dst"]["criteria"]["after"],
            "2024-03-10 03:00:00 -0400"
        );
        // Captured IO values do not observe subsequent player mutations.
        player
            .advancements
            .revoke(&manager, &manager.advancements[0].id, "before");
        let mut loaded = PlayerAdvancements::new(&manager);
        loaded.load_json(&manager, &value).unwrap();
        assert!(
            loaded
                .get_progress(&manager.advancements[0].id)
                .unwrap()
                .is_done()
        );
    }
}
