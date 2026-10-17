//! Port of StoredUserList, ServerOpList, UserWhiteList and ban-list admission.
use anyhow::{Context, Result};
use ferraloom_protocol::{Uuid, login::GameProfile};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
};
use time::{OffsetDateTime, macros::format_description};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Operator {
    pub uuid: String,
    pub name: String,
    #[serde(default = "default_level")]
    pub level: i32,
    #[serde(default, rename = "bypassesPlayerLimit")]
    pub bypasses_player_limit: bool,
}
fn default_level() -> i32 {
    4
}
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
    pub ip: Option<IpAddr>,
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
    pub ops: Vec<Operator>,
    pub whitelist: Vec<WhiteListEntry>,
    pub banned_players: Vec<Ban>,
    pub banned_ips: Vec<Ban>,
    user_cache: Vec<CachedUser>,
}
impl AccessLists {
    pub fn load(root: &Path) -> Result<Self> {
        Ok(Self {
            root: root.into(),
            ops: read(root, "ops.json")?,
            whitelist: read(root, "whitelist.json")?,
            banned_players: read(root, "banned-players.json")?,
            banned_ips: read(root, "banned-ips.json")?,
            user_cache: read(root, "usercache.json")?,
        })
    }
    pub fn permission(&self, id: Uuid) -> i32 {
        self.ops
            .iter()
            .find(|o| o.uuid.eq_ignore_ascii_case(&id.to_string()))
            .map_or(0, |o| o.level)
    }
    pub fn can_join(
        &self,
        profile: &GameProfile,
        ip: IpAddr,
        whitelist: bool,
        full: bool,
    ) -> Option<String> {
        let id = profile.id.to_string();
        if let Some(ban) = self
            .banned_players
            .iter()
            .find(|b| b.active() && b.uuid.as_deref() == Some(&id))
        {
            return Some(format!(
                "You are banned from this server!\nReason: {}",
                ban.reason.as_deref().unwrap_or("Banned by an operator.")
            ));
        }
        let op = self.ops.iter().find(|o| o.uuid.eq_ignore_ascii_case(&id));
        if whitelist
            && op.is_none()
            && !self
                .whitelist
                .iter()
                .any(|e| e.uuid.eq_ignore_ascii_case(&id))
        {
            return Some("You are not white-listed on this server!".into());
        }
        if let Some(ban) = self
            .banned_ips
            .iter()
            .find(|b| b.active() && b.ip == Some(ip))
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
