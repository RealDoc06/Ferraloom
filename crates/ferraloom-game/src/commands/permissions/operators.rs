//! Dedicated-server ServerOpList and MinecraftServer#getProfilePermissions.
use super::{Permission, PermissionLevel, PermissionSet};
use crate::{
    entity::{EntityRef, player::ServerPlayer},
    level::{Level, LevelError},
};
use ferraloom_protocol::{Uuid, login::GameProfile};
use ferraloom_util::collections::HashMap;
use serde::{Deserialize, Deserializer, Serialize};

/// Port of ServerOpListEntry. Omitted levels are ALL (0), and IDs clamp at decode.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerOpListEntry {
    pub uuid: String,
    pub name: String,
    #[serde(default, deserialize_with = "read_level")]
    level: i32,
    #[serde(default, rename = "bypassesPlayerLimit")]
    pub bypasses_player_limit: bool,
}
fn read_level<'de, D: Deserializer<'de>>(input: D) -> Result<i32, D::Error> {
    i32::deserialize(input).map(|id| PermissionLevel::by_id(id).id())
}
impl ServerOpListEntry {
    pub fn new(profile: &GameProfile, level: PermissionLevel) -> Self {
        Self {
            uuid: profile.id.to_string(),
            name: profile.name.clone(),
            level: level.id(),
            bypasses_player_limit: false,
        }
    }
    pub fn permissions(&self) -> PermissionSet {
        PermissionSet::Level(self.level())
    }
    pub fn level(&self) -> PermissionLevel {
        PermissionLevel::by_id(self.level)
    }
}

/// The sole native StoredUserList map, moved with ServerShared at phase boundaries.
#[derive(Debug, Default)]
pub struct ServerOpList {
    entries: HashMap<String, ServerOpListEntry>,
}
impl ServerOpList {
    pub fn from_entries(entries: impl IntoIterator<Item = ServerOpListEntry>) -> Self {
        let mut ops = Self::default();
        ops.replace_entries(entries);
        ops
    }
    /// StoredUserList#load clears and refills its existing JDK map in file order.
    pub fn replace_entries(&mut self, entries: impl IntoIterator<Item = ServerOpListEntry>) {
        self.entries.clear();
        for mut entry in entries {
            if let Some(id) = crate::commands::arguments::selectors::parse_uuid(&entry.uuid) {
                entry.uuid = id.to_string();
                self.entries.insert(entry.uuid.clone(), entry);
            }
        }
    }
    pub fn get(&self, id: Uuid) -> Option<&ServerOpListEntry> {
        self.entries.peek(&id.to_string())
    }
    pub fn contains(&self, id: Uuid) -> bool {
        self.get(id).is_some()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn values(&self) -> impl Iterator<Item = &ServerOpListEntry> {
        self.entries.values()
    }
    pub fn set_operator(&mut self, profile: &GameProfile, level: Option<PermissionLevel>) {
        if let Some(level) = level {
            let bypasses_player_limit = self
                .get(profile.id)
                .is_some_and(|entry| entry.bypasses_player_limit);
            let mut entry = ServerOpListEntry::new(profile, level);
            entry.bypasses_player_limit = bypasses_player_limit;
            self.entries.insert(profile.id.to_string(), entry);
        } else {
            self.entries.remove(&profile.id.to_string());
        }
    }
    pub fn permission_level(&self, id: Uuid) -> PermissionLevel {
        self.get(id)
            .map_or(PermissionLevel::All, ServerOpListEntry::level)
    }
    /// Port of MinecraftServer#getProfilePermissions. The integrated-server owner
    /// branch is intentionally unmodeled: Ferraloom only runs a dedicated server.
    pub fn get_profile_permissions(&self, id: Uuid) -> PermissionSet {
        PermissionSet::Level(self.permission_level(id))
    }
}
impl Level {
    /// Port of ServerPlayer#permissions through the canonical, phase-lent ops list.
    pub fn player_permissions(&self, entity: EntityRef) -> Result<PermissionSet, LevelError> {
        let player = self.entity::<ServerPlayer>(entity)?;
        Ok(self.shared()?.ops.get_profile_permissions(player.uuid))
    }
    /// Port of Player#canUseGameMasterBlocks. instabuild is the live ability,
    /// independent of permission level and of the player's current GameType.
    pub fn can_use_game_master_blocks(&self, entity: EntityRef) -> Result<bool, LevelError> {
        let player = self.entity::<ServerPlayer>(entity)?;
        Ok(player.instabuild()
            && self
                .player_permissions(entity)?
                .has(&Permission::COMMANDS_GAMEMASTER))
    }
}

#[cfg(test)]
mod tests;
