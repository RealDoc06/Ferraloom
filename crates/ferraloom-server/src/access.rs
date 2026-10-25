//! Port of StoredUserList, ServerOpList, UserWhiteList and ban-list admission.
use anyhow::{Context, Result};
use ferraloom_game::commands::permissions::{ServerOpList, ServerOpListEntry};
use ferraloom_protocol::login::GameProfile;
use ferraloom_util::collections::HashMap;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
};
use time::{OffsetDateTime, macros::format_description};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WhiteListEntry {
    pub uuid: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ban {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub expires: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}
impl Ban {
    pub fn permanent(
        uuid: Option<String>,
        name: Option<String>,
        ip: Option<String>,
        source: String,
        reason: Option<String>,
    ) -> Self {
        let created = OffsetDateTime::now_utc().format(format_description!("[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]")).expect("date formatter");
        Self {
            uuid,
            name,
            ip,
            created: Some(created),
            source: Some(source),
            expires: Some("forever".into()),
            reason,
        }
    }
    pub fn reason_message(&self) -> ferraloom_protocol::chat::Component {
        self.reason.as_ref().map_or_else(
            || {
                ferraloom_protocol::chat::Component::translatable(
                    "multiplayer.disconnect.banned.reason.default",
                    vec![],
                )
            },
            ferraloom_protocol::chat::Component::literal,
        )
    }
    fn active(&self) -> bool {
        self.expires.as_deref().filter(|s| *s != "forever").and_then(|s| OffsetDateTime::parse(s, format_description!("[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]" )).ok()).is_none_or(|expires| expires > OffsetDateTime::now_utc())
    }
}
#[derive(Serialize, Deserialize)]
struct CachedUser {
    name: String,
    uuid: String,
    #[serde(rename = "expiresOn")]
    expires_on: String,
}

pub struct AccessLists {
    root: PathBuf,
    pub whitelist: HashMap<String, WhiteListEntry>,
    pub banned_players: HashMap<String, Ban>,
    pub banned_ips: HashMap<String, Ban>,
    user_cache: Vec<CachedUser>,
}
impl AccessLists {
    pub fn load(root: &Path) -> Result<(Self, ServerOpList)> {
        let ops = ServerOpList::from_entries(read::<ServerOpListEntry>(root, "ops.json")?);
        Ok((
            Self {
                root: root.into(),
                whitelist: read_map(root, "whitelist.json", |entry: &WhiteListEntry| {
                    canonical_uuid(&entry.uuid)
                })?,
                banned_players: read_map(root, "banned-players.json", |entry: &Ban| {
                    entry.uuid.as_deref().and_then(canonical_uuid)
                })?,
                banned_ips: read_map(root, "banned-ips.json", |entry: &Ban| entry.ip.clone())?,
                user_cache: read(root, "usercache.json")?,
            },
            ops,
        ))
    }
    pub fn reload_whitelist(&mut self) -> Result<()> {
        self.whitelist = read_map(&self.root, "whitelist.json", |entry: &WhiteListEntry| {
            canonical_uuid(&entry.uuid)
        })?;
        Ok(())
    }
    pub fn save_whitelist(&self) -> Result<()> {
        self.write_entries("whitelist.json", self.whitelist.values())
    }
    pub fn save_bans(&self, ips: bool) -> Result<()> {
        if ips {
            self.write_entries("banned-ips.json", self.banned_ips.values())
        } else {
            self.write_entries("banned-players.json", self.banned_players.values())
        }
    }
    fn write_entries<'a, T: Serialize + 'a>(
        &self,
        name: &str,
        entries: impl IntoIterator<Item = &'a T>,
    ) -> Result<()> {
        fs::write(
            self.root.join(name),
            serde_json::to_vec_pretty(&entries.into_iter().collect::<Vec<_>>())?,
        )?;
        Ok(())
    }
    pub fn cached_profile(&self, name: &str) -> Option<GameProfile> {
        let cached = self
            .user_cache
            .iter()
            .find(|user| user.name.eq_ignore_ascii_case(name))?;
        Some(GameProfile {
            id: ferraloom_game::commands::arguments::selectors::parse_uuid(&cached.uuid)?,
            name: cached.name.clone(),
            properties: Vec::new(),
        })
    }
    pub fn save_operators(&self, ops: &ServerOpList) -> Result<()> {
        self.write_entries("ops.json", ops.values())
    }
    /// Reload directly into the same phase-lent owner used by game queries.
    pub fn reload_operators(&self, ops: &mut ServerOpList) -> Result<()> {
        ops.replace_entries(read::<ServerOpListEntry>(&self.root, "ops.json")?);
        Ok(())
    }
    pub fn can_join(
        &self,
        profile: &GameProfile,
        ip: IpAddr,
        whitelist: bool,
        full: bool,
        ops: &ServerOpList,
    ) -> Option<String> {
        let id = profile.id.to_string();
        if let Some(ban) = self
            .banned_players
            .values()
            .find(|b| b.active() && b.uuid.as_deref() == Some(&id))
        {
            return Some(format!(
                "You are banned from this server!\nReason: {}",
                ban.reason.as_deref().unwrap_or("Banned by an operator.")
            ));
        }
        let op = ops.get(profile.id);
        if whitelist
            && op.is_none()
            && !self
                .whitelist
                .values()
                .any(|e| e.uuid.eq_ignore_ascii_case(&id))
        {
            return Some("You are not white-listed on this server!".into());
        }
        if let Some(ban) = self
            .banned_ips
            .values()
            .find(|b| b.active() && b.ip.as_deref() == Some(ip.to_string().as_str()))
        {
            return Some(format!(
                "Your IP address is banned from this server!\nReason: {}",
                ban.reason.as_deref().unwrap_or("Banned by an operator.")
            ));
        }
        if full && !op.is_some_and(|o| o.bypasses_player_limit) {
            return Some("The server is full!".into());
        }
        None
    }
    pub fn remember(&mut self, profile: &GameProfile) -> Result<()> {
        self.user_cache.retain(|u| {
            u.uuid != profile.id.to_string() && !u.name.eq_ignore_ascii_case(&profile.name)
        });
        let expires = (OffsetDateTime::now_utc() + time::Duration::days(30)).format(format_description!("[year]-[month]-[day] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]"))?;
        self.user_cache.insert(
            0,
            CachedUser {
                name: profile.name.clone(),
                uuid: profile.id.to_string(),
                expires_on: expires,
            },
        );
        fs::write(
            self.root.join("usercache.json"),
            serde_json::to_vec_pretty(&self.user_cache)?,
        )?;
        Ok(())
    }
}
fn read<T: for<'de> Deserialize<'de>>(root: &Path, name: &str) -> Result<Vec<T>> {
    let path = root.join(name);
    if !path.exists() {
        fs::write(&path, "[]\n")?;
    }
    serde_json::from_slice(&fs::read(&path)?).with_context(|| format!("reading {}", path.display()))
}

/// StoredUserList#load skips entries whose user identity cannot be decoded.
fn read_map<T: for<'de> Deserialize<'de>>(
    root: &Path,
    name: &str,
    key: impl Fn(&T) -> Option<String>,
) -> Result<HashMap<String, T>> {
    let mut map = HashMap::new();
    for entry in read(root, name)? {
        if let Some(key) = key(&entry) {
            map.insert(key, entry);
        }
    }
    Ok(map)
}
fn canonical_uuid(value: &str) -> Option<String> {
    ferraloom_game::commands::arguments::selectors::parse_uuid(value).map(|id| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_user_lists_replace_duplicate_identities_and_skip_invalid_users() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("ops.json"),
            r#"[
            {"uuid":"00000000-0000-0000-0000-000000000001","name":"Old","level":2},
            {"uuid":"invalid","name":"Invalid","level":4},
            {"uuid":"00000000-0000-0000-0000-000000000001","name":"New","level":3}
        ]"#,
        )
        .unwrap();
        let (_lists, ops) = AccessLists::load(root.path()).unwrap();
        assert_eq!(ops.len(), 1);
        let op = ops.values().next().unwrap();
        assert_eq!((&*op.name, op.level().id()), ("New", 3));
    }
    #[test]
    fn file_reload_updates_the_same_ops_owner_and_missing_level_is_zero() {
        let root = tempfile::tempdir().unwrap();
        let profile = GameProfile {
            id: ferraloom_protocol::Uuid::new(1, 2),
            name: "metadata".into(),
            properties: vec![],
        };
        fs::write(
            root.path().join("ops.json"),
            format!(r#"[{{"uuid":"{}","name":"metadata"}}]"#, profile.id),
        )
        .unwrap();
        let (access, ops) = AccessLists::load(root.path()).unwrap();
        let mut shared = ferraloom_game::ServerShared {
            ops,
            ..Default::default()
        };
        assert!(shared.ops.contains(profile.id));
        assert_eq!(shared.ops.permission_level(profile.id).id(), 0);
        fs::write(
            root.path().join("ops.json"),
            format!(
                r#"[{{"uuid":"{}","name":"metadata","level":2}}]"#,
                profile.id
            ),
        )
        .unwrap();
        access.reload_operators(&mut shared.ops).unwrap();
        assert!(shared.ops.get_profile_permissions(profile.id).has_level(2));
        access.save_operators(&shared.ops).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(root.path().join("ops.json")).unwrap()).unwrap();
        assert_eq!(saved[0]["level"], 2);
    }
}
