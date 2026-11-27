//! The server's player lists for `Server` and `OfflinePlayer`: operators,
//! the whitelist, bans and the user cache (`ServerNative.java`'s list natives).
//!
//! Bukkit lets plugins read and change these off the main thread (an async
//! task asking `getOfflinePlayer(name)` is the common case), so they are not
//! the host's: the game registers one [`ServerLists`] before [`crate::start`]
//! and implements it over the same locked lists its login checks use.
use crate::{
    PlayerId,
    natives::{guard, jbool, player_id},
};
use jni::{
    JNIEnv,
    objects::{JClass, JObject, JString},
    sys::{JNI_FALSE, jboolean, jint, jlong, jobjectArray, jstring},
};
use std::sync::OnceLock;

/// A player as the lists name them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: PlayerId,
    pub name: String,
}

/// The lists of players: `ops.json`, `whitelist.json`, `banned-players.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    Ops,
    Whitelist,
    Banned,
}

/// `white-list` and `enforce-whitelist` in server.properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhitelistFlag {
    On,
    Enforced,
}

/// What a ban is of: a player (`banned-players.json`) or an address (`banned-ips.json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BanTarget {
    Player(Profile),
    Ip(String),
}

/// A ban as BanList shows it; times in epoch milliseconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ban {
    pub target: BanTarget,
    pub created: i64,
    pub source: String,
    pub expires: Option<i64>,
    pub reason: Option<String>,
}

/// The server's lists, answered on any thread. Changes are saved at once;
/// what players see of them (an operator's permission level and command
/// tree, the kicks an enforced whitelist makes) follows at the next tick's start.
pub trait ServerLists: Send + Sync {
    /// The user cache's player with this name (any case), else the profile
    /// service's, which blocks: the bridge asks only in online mode, as
    /// CraftServer.getOfflinePlayer(String) does.
    fn profile_by_name(&self, name: &str) -> Option<Profile>;
    /// The user cache's player with this UUID; never a lookup.
    fn profile_by_id(&self, id: PlayerId) -> Option<Profile>;
    fn listed(&self, list: List) -> Vec<Profile>;
    fn is_listed(&self, list: List, id: PlayerId) -> bool;
    /// Adds to or removes from the operators (at the server's
    /// `op-permission-level`) or the whitelist; `Banned` is [`Self::add_ban`]'s.
    fn set_listed(&self, list: List, who: &Profile, on: bool);
    fn whitelist_flag(&self, flag: WhitelistFlag) -> bool;
    /// Changes the flag and stores it in server.properties, as `/whitelist on` does.
    fn set_whitelist_flag(&self, flag: WhitelistFlag, on: bool);
    /// whitelist.json read again.
    fn reload_whitelist(&self);
    /// The bans of players (`ip` false) or of addresses, expired ones dropped.
    fn bans(&self, ip: bool) -> Vec<Ban>;
    /// A ban, replacing the target's; created now unless given (a saved
    /// `BanEntry` keeps its date), a missing source is vanilla's `(Unknown)`.
    fn add_ban(
        &self,
        target: BanTarget,
        created: Option<i64>,
        source: Option<String>,
        expires: Option<i64>,
        reason: Option<String>,
    );
    fn pardon(&self, target: &BanTarget);
}

static LISTS: OnceLock<Box<dyn ServerLists>> = OnceLock::new();

/// The game's lists, before the plugins start; the first one set stays.
pub fn set_lists(lists: Box<dyn ServerLists>) {
    let _ = LISTS.set(lists);
}

fn lists() -> &'static dyn ServerLists {
    LISTS
        .get()
        .expect("the game registers its player lists before the plugins start")
        .as_ref()
}

fn list(which: jint) -> List {
    match which {
        0 => List::Ops,
        1 => List::Whitelist,
        _ => List::Banned,
    }
}

fn flag(which: jint) -> WhitelistFlag {
    if which == 0 {
        WhitelistFlag::On
    } else {
        WhitelistFlag::Enforced
    }
}

/// `java.util.UUID.toString` of an id.
fn uuid(id: PlayerId) -> String {
    let h = format!("{id:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

/// A `String[]` of these, nulls kept.
fn strings(
    env: &mut JNIEnv,
    items: &[Option<String>],
) -> Result<jobjectArray, crate::natives::Fail> {
    let array = env.new_object_array(items.len() as jint, "java/lang/String", JObject::null())?;
    for (i, item) in items.iter().enumerate() {
        if let Some(s) = item {
            let s = env.new_string(s)?;
            env.set_object_array_element(&array, i as jint, &s)?;
            // A long ban or whitelist file piles up no local references.
            env.delete_local_ref(s)?;
        }
    }
    Ok(array.into_raw())
}

fn optional(env: &mut JNIEnv, s: &JString) -> Result<Option<String>, crate::natives::Fail> {
    if s.is_null() {
        return Ok(None);
    }
    Ok(Some(env.get_string(s)?.into()))
}

/// `ServerNative`'s list natives, as `(name, signature, function)`.
pub(super) fn natives() -> Vec<(&'static str, String, *mut std::ffi::c_void)> {
    const S: &str = "Ljava/lang/String;";
    vec![
        (
            "profileByName",
            format!("({S})[{S}"),
            profile_by_name as *mut _,
        ),
        ("profileById", format!("(JJ){S}"), profile_by_id as *mut _),
        ("listed", format!("(I)[{S}"), listed as *mut _),
        ("isListed", "(IJJ)Z".into(), is_listed as *mut _),
        ("setListed", format!("(IJJ{S}Z)V"), set_listed as *mut _),
        ("whitelistFlag", "(I)Z".into(), whitelist_flag as *mut _),
        (
            "setWhitelistFlag",
            "(IZ)V".into(),
            set_whitelist_flag as *mut _,
        ),
        ("reloadWhitelist", "()V".into(), reload_whitelist as *mut _),
        ("bans", format!("(Z)[{S}"), bans as *mut _),
        ("addBan", format!("({S}JJJ{S}J{S})V"), add_ban as *mut _),
        ("pardon", format!("({S}JJ)V"), pardon as *mut _),
    ]
}

extern "system" fn profile_by_name(mut env: JNIEnv, _: JClass, name: JString) -> jobjectArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let name: String = env.get_string(&name)?.into();
        match lists().profile_by_name(&name) {
            Some(p) => strings(env, &[Some(uuid(p.id)), Some(p.name)]),
            None => Ok(std::ptr::null_mut()),
        }
    })
}

extern "system" fn profile_by_id(mut env: JNIEnv, _: JClass, most: jlong, least: jlong) -> jstring {
    guard(&mut env, std::ptr::null_mut(), |env| {
        match lists().profile_by_id(player_id(most, least)) {
            Some(p) => Ok(env.new_string(p.name)?.into_raw()),
            None => Ok(std::ptr::null_mut()),
        }
    })
}

/// `[uuid, name, uuid, name, ...]`.
extern "system" fn listed(mut env: JNIEnv, _: JClass, which: jint) -> jobjectArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let flat: Vec<Option<String>> = lists()
            .listed(list(which))
            .into_iter()
            .flat_map(|p| [Some(uuid(p.id)), Some(p.name)])
            .collect();
        strings(env, &flat)
    })
}

extern "system" fn is_listed(
    mut env: JNIEnv,
    _: JClass,
    which: jint,
    most: jlong,
    least: jlong,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        Ok(jbool(
            lists().is_listed(list(which), player_id(most, least)),
        ))
    })
}

extern "system" fn set_listed(
    mut env: JNIEnv,
    _: JClass,
    which: jint,
    most: jlong,
    least: jlong,
    name: JString,
    on: jboolean,
) {
    guard(&mut env, (), |env| {
        let who = Profile {
            id: player_id(most, least),
            name: optional(env, &name)?.unwrap_or_default(),
        };
        lists().set_listed(list(which), &who, on != JNI_FALSE);
        Ok(())
    })
}

extern "system" fn whitelist_flag(mut env: JNIEnv, _: JClass, which: jint) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        Ok(jbool(lists().whitelist_flag(flag(which))))
    })
}

extern "system" fn set_whitelist_flag(mut env: JNIEnv, _: JClass, which: jint, on: jboolean) {
    guard(&mut env, (), |_| {
        lists().set_whitelist_flag(flag(which), on != JNI_FALSE);
        Ok(())
    })
}

extern "system" fn reload_whitelist(mut env: JNIEnv, _: JClass) {
    guard(&mut env, (), |_| {
        lists().reload_whitelist();
        Ok(())
    })
}

/// Six strings per ban: target (name or address), UUID (null for an
/// address), created and expiry (milliseconds; expiry null for never), source, reason (may be null).
extern "system" fn bans(mut env: JNIEnv, _: JClass, ip: jboolean) -> jobjectArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let mut flat = Vec::new();
        for ban in lists().bans(ip != JNI_FALSE) {
            let (target, id) = match ban.target {
                BanTarget::Player(p) => (p.name, Some(uuid(p.id))),
                BanTarget::Ip(ip) => (ip, None),
            };
            flat.extend([
                Some(target),
                id,
                Some(ban.created.to_string()),
                ban.expires.map(|e| e.to_string()),
                Some(ban.source),
                ban.reason,
            ]);
        }
        strings(env, &flat)
    })
}

/// A nonzero UUID names a player (`name` their name), else `name` is an address.
fn target(
    env: &mut JNIEnv,
    name: &JString,
    most: jlong,
    least: jlong,
    player: bool,
) -> Result<BanTarget, crate::natives::Fail> {
    let name = optional(env, name)?.unwrap_or_default();
    Ok(if player {
        BanTarget::Player(Profile {
            id: player_id(most, least),
            name,
        })
    } else {
        BanTarget::Ip(name)
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn add_ban(
    mut env: JNIEnv,
    _: JClass,
    name: JString,
    most: jlong,
    least: jlong,
    created: jlong,
    source: JString,
    expires: jlong,
    reason: JString,
) {
    guard(&mut env, (), |env| {
        let target = target(env, &name, most, least, most != 0 || least != 0)?;
        let source = optional(env, &source)?;
        let reason = optional(env, &reason)?;
        let created = (created >= 0).then_some(created);
        lists().add_ban(
            target,
            created,
            source,
            (expires >= 0).then_some(expires),
            reason,
        );
        Ok(())
    })
}

extern "system" fn pardon(mut env: JNIEnv, _: JClass, name: JString, most: jlong, least: jlong) {
    guard(&mut env, (), |env| {
        let target = target(env, &name, most, least, most != 0 || least != 0)?;
        lists().pardon(&target);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_print_as_java_does() {
        let id = 0x069a79f4_44e9_4726_a5be_fca90e38aaf5_u128;
        assert_eq!(uuid(id), "069a79f4-44e9-4726-a5be-fca90e38aaf5");
    }
}
