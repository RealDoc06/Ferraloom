//! The server area: the console, logging, vanilla commands and shutdown
//! (`java/bridge/.../ServerNative.java`; its entry points are `Bridge`'s),
//! the player lists ([`lists`]) and plugins' boss bars ([`bossbars`]).
mod bossbars;
mod lists;

use crate::{
    PlayerId, PluginHost,
    bridge::{self, Entry, obj},
    natives::{self, Deferred, Fail, guard, jbool, string, with_host},
};
pub use bossbars::{PluginBossBar, boss_bars_for};
use jni::{
    JNIEnv,
    objects::{JClass, JObjectArray, JString, JValue},
    signature::{Primitive, ReturnType},
    sys::{JNI_FALSE, jboolean, jlong},
};
pub use lists::{Ban, BanTarget, List, Profile, ServerLists, WhitelistFlag, set_lists};
use std::{
    ffi::c_void,
    sync::{Mutex, OnceLock},
};

const TAB_COMPLETE: usize = 0;
const HIDDEN_COMMANDS: usize = 1;
const OPS_CHANGED: usize = 2;

/// `ServerEvents`' entry points.
static EVENTS: OnceLock<Entry<3>> = OnceLock::new();

/// What `Server` and the console read and change of the game.
pub trait ServerHost {
    /// A line in the server's log, as a command's reply to the console.
    fn console_message(&mut self, message: &str);
    /// Runs a vanilla command line as the console; false when there is no such command.
    fn dispatch_vanilla(&mut self, line: &str) -> bool;
    /// Stops the server after this tick.
    fn shutdown(&mut self);
}

/// At start: `ServerNative`'s methods bound.
pub(crate) fn load(env: &mut JNIEnv) -> jni::errors::Result<()> {
    const S: &str = "Ljava/lang/String;";
    natives::register(
        env,
        "dev/ferraloom/bukkit/ServerNative",
        &[
            ("log", &format!("({S}{S}{S})V"), native_log as *mut c_void),
            (
                "consoleMessage",
                &format!("({S})V"),
                console_message as *mut c_void,
            ),
            (
                "dispatchVanilla",
                &format!("({S})Z"),
                dispatch_vanilla as *mut c_void,
            ),
            ("shutdown", "()V", shutdown as *mut c_void),
        ],
    )?;
    let more: Vec<_> = lists::natives()
        .into_iter()
        .chain(bossbars::natives())
        .chain([(
            "updateCommands",
            "(JJ)V".to_owned(),
            update_commands as *mut c_void,
        )])
        .collect();
    let more: Vec<(&str, &str, *mut c_void)> =
        more.iter().map(|(n, s, f)| (*n, s.as_str(), *f)).collect();
    natives::register(env, "dev/ferraloom/bukkit/ServerNative", &more)?;
    let entry = Entry::load(
        env,
        "dev/ferraloom/bukkit/ServerEvents",
        [
            ("tabComplete", format!("(JJ{S})[{S}")),
            ("hiddenCommands", format!("(JJ)[{S}")),
            ("opsChanged", "()V".to_owned()),
        ],
    )?;
    let _ = EVENTS.set(entry);
    Ok(())
}

/// The operator list changed: every online player's Bukkit permissions are
/// recalculated (CraftBukkit does it in sendPlayerPermissionLevel), before
/// their command trees are filtered again. One JNI call; nothing without plugins.
pub fn ops_changed() {
    let Some(events) = EVENTS.get() else {
        return;
    };
    bridge::call_unlent(|env| {
        // SAFETY: `opsChanged`'s signature in `load`.
        unsafe {
            events.invoke(
                env,
                OPS_CHANGED,
                ReturnType::Primitive(Primitive::Void),
                &[],
            )
        }?;
        Ok(())
    });
}

/// Players whose command tree is to be sent again, from any thread.
static COMMAND_UPDATES: Mutex<Vec<PlayerId>> = Mutex::new(Vec::new());

extern "system" fn update_commands(mut env: JNIEnv, _: JClass, most: jlong, least: jlong) {
    guard(&mut env, (), |_| {
        COMMAND_UPDATES
            .lock()
            .unwrap()
            .push(natives::player_id(most, least));
        Ok(())
    })
}

/// The players to send their command tree again (they joined, or a plugin
/// called `Player.updateCommands`) since the last call; at a tick's start.
pub fn take_command_updates() -> Vec<PlayerId> {
    std::mem::take(&mut *COMMAND_UPDATES.lock().unwrap())
}

/// A `String[]` as strings (null elements skipped).
fn strings(env: &mut JNIEnv, array: JObjectArray) -> jni::errors::Result<Vec<String>> {
    let mut out = Vec::new();
    if array.is_null() {
        return Ok(out);
    }
    for i in 0..env.get_array_length(&array)? {
        let s = JString::from(env.get_object_array_element(&array, i)?);
        if !s.is_null() {
            out.push(env.get_string(&s)?.into());
        }
    }
    Ok(out)
}

/// CraftServer.tabComplete for a player's plugin command line (no slash):
/// the completions of its last word, after `TabCompleteEvent`. Runs the
/// plugin's `TabCompleter`; one JNI call per request.
pub fn tab_complete(host: &mut dyn PluginHost, player: PlayerId, line: &str) -> Vec<String> {
    let Some(events) = EVENTS.get() else {
        return Vec::new();
    };
    let (most, least) = natives::halves(player);
    bridge::call(host, |env| {
        let line = env.new_string(line)?;
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            obj(&line),
        ];
        // SAFETY: `tabComplete`'s signature in `load`.
        let out = unsafe { events.invoke(env, TAB_COMPLETE, ReturnType::Array, &args) }?.l()?;
        strings(env, JObjectArray::from(out))
    })
    .unwrap_or_default()
}

/// The plugin command labels `player` may not use (`Command.testPermissionSilent`),
/// for their command tree, as CraftBukkit's command nodes require. No host:
/// permission checks read the plugins' permission state only. Empty without plugins.
pub fn hidden_commands(player: PlayerId) -> Vec<String> {
    let Some(events) = EVENTS.get() else {
        return Vec::new();
    };
    let (most, least) = natives::halves(player);
    bridge::call_unlent(|env| {
        let args = [JValue::Long(most).as_jni(), JValue::Long(least).as_jni()];
        // SAFETY: `hiddenCommands`'s signature in `load`.
        let out = unsafe { events.invoke(env, HIDDEN_COMMANDS, ReturnType::Array, &args) }?.l()?;
        strings(env, JObjectArray::from(out))
    })
    .unwrap_or_default()
}

extern "system" fn native_log(
    mut env: JNIEnv,
    _: JClass,
    level: JString,
    thread: JString,
    message: JString,
) {
    guard(&mut env, (), |env| {
        let (level, thread) = (string(env, &level)?, string(env, &thread)?);
        natives::log(&level, &thread, &string(env, &message)?);
        Ok(())
    })
}

extern "system" fn console_message(mut env: JNIEnv, _: JClass, message: JString) {
    guard(&mut env, (), |env| {
        let text = string(env, &message)?;
        if with_host(|h| h.console_message(&text)).is_none() {
            natives::defer(Deferred::Console(text));
        }
        Ok(())
    })
}

extern "system" fn dispatch_vanilla(mut env: JNIEnv, _: JClass, line: JString) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let line = string(env, &line)?;
        let ran = with_host(|h| h.dispatch_vanilla(&line))
            .ok_or(Fail::NoHost("Server.dispatchCommand"))?;
        Ok(jbool(ran))
    })
}

extern "system" fn shutdown(mut env: JNIEnv, _: JClass) {
    guard(&mut env, (), |_| {
        if with_host(|h| h.shutdown()).is_none() {
            natives::defer(Deferred::Shutdown);
        }
        Ok(())
    })
}
