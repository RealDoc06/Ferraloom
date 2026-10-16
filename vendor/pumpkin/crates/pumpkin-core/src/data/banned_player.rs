use std::path::Path;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::net::{GameProfile, is_legacy_offline_profile, offline_uuid};

use super::{LoadJSONConfiguration, SaveJSONConfiguration, banlist_serializer::BannedPlayerEntry};

#[derive(Deserialize, Serialize, Default)]
#[serde(transparent)]
pub struct BannedPlayerList {
    pub banned_players: Vec<BannedPlayerEntry>,
}

impl BannedPlayerList {
    #[must_use]
    pub fn get_entry(&mut self, profile: &GameProfile) -> Option<&BannedPlayerEntry> {
        self.remove_invalid_entries();
        self.banned_players.iter().find(|entry| {
            entry.uuid == profile.id
                    // Retain bans under the old deterministic offline identity.
                    // An arbitrary same-name UUID is never treated as an alias.
                    || (entry.name == profile.name
                        && is_legacy_offline_profile(entry.uuid, &entry.name)
                        && offline_uuid(&entry.name).is_ok_and(|uuid| uuid == profile.id))
        })
    }

    fn remove_invalid_entries(&mut self) {
        let original_len = self.banned_players.len();

        self.banned_players.retain(|entry| {
            entry
                .expires
                .is_none_or(|expires| expires >= OffsetDateTime::now_utc())
        });

        if original_len != self.banned_players.len() {
            self.save();
        }
    }
}

impl LoadJSONConfiguration for BannedPlayerList {
    fn get_path() -> &'static Path {
        Path::new("banned-players.json")
    }
    fn validate(&self) {
        // TODO: Validate the list
    }
}

impl SaveJSONConfiguration for BannedPlayerList {}

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use uuid::Uuid;

    fn profile(id: Uuid, name: &str) -> GameProfile {
        GameProfile {
            id,
            name: name.into(),
            properties: ArcSwap::from_pointee(Vec::new()),
            profile_actions: None,
        }
    }

    #[test]
    fn legacy_offline_ban_matches_corrected_uuid_without_rewriting_the_entry() {
        let legacy = profile(
            Uuid::parse_str("14209f43-09d3-4876-d114-4c2568f51fc3").unwrap(),
            "SpamObserver",
        );
        let canonical = profile(offline_uuid(&legacy.name).unwrap(), &legacy.name);
        let mut list = BannedPlayerList {
            banned_players: vec![BannedPlayerEntry::new(
                &legacy,
                "test".into(),
                None,
                "retained ban".into(),
            )],
        };
        assert_eq!(list.get_entry(&canonical).unwrap().uuid, legacy.id);
        assert_eq!(list.banned_players.len(), 1);
        assert_eq!(list.banned_players[0].reason, "retained ban");
        assert!(
            list.get_entry(&profile(Uuid::from_u128(7), &legacy.name))
                .is_none()
        );
        assert!(
            list.get_entry(&profile(
                offline_uuid("spamobserver").unwrap(),
                "spamobserver",
            ))
            .is_none()
        );
    }

    #[test]
    fn ordinary_bans_match_uuid_and_do_not_become_name_bans() {
        let original = profile(Uuid::from_u128(7), "SpamObserver");
        let mut list = BannedPlayerList {
            banned_players: vec![BannedPlayerEntry::new(
                &original,
                "test".into(),
                None,
                "ordinary ban".into(),
            )],
        };
        assert!(list.get_entry(&profile(original.id, "OtherName")).is_some());
        assert!(
            list.get_entry(&profile(
                offline_uuid(&original.name).unwrap(),
                &original.name
            ))
            .is_none()
        );
    }
}
