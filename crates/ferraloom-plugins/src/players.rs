//! The players area: `Player` objects, their join and quit, chat and
//! commands (`java/bridge/.../PlayerEvents.java`, `PlayerNative.java`).
use crate::{
    Location, PlayerId,
    bridge::{self, Entry, obj, string_or_none},
    natives::{self, Deferred, Fail, guard, jbool, player_id, string, with_host},
};
use jni::{
    JNIEnv,
    objects::{JClass, JString, JValue},
    signature::{Primitive, ReturnType},
    sys::{JNI_FALSE, jboolean, jdoubleArray, jfloat, jint, jlong},
};
use std::{ffi::c_void, sync::OnceLock};

/// What `Player` reads and changes of the game.
pub trait PlayerHost {
    /// A chat line (legacy `§` codes) to one online player.
    fn send_message(&mut self, player: PlayerId, message: &str);
    /// An online player's location; none when they are not on.
    fn location(&mut self, player: PlayerId) -> Option<Location>;
    /// Moves an online player; false when it could not.
    fn teleport(&mut self, player: PlayerId, to: Location) -> bool;
}

/// A player who just joined, for `PlayerJoinEvent`.
pub struct Joined<'a> {
    pub id: PlayerId,
    pub name: &'a str,
    pub entity_id: i32,
    pub op: bool,
    /// The join message vanilla would send, as legacy text (`§eSteve joined the game`).
    pub message: &'a str,
}

const S: &str = "Ljava/lang/String;";
const JOIN: usize = 0;
const QUIT: usize = 1;
const CHAT: usize = 2;
const COMMAND: usize = 3;

static EVENTS: OnceLock<Entry<4>> = OnceLock::new();

/// At start: `PlayerNative`'s methods bound, `PlayerEvents`' looked up.
pub(crate) fn load(env: &mut JNIEnv) -> jni::errors::Result<()> {
    natives::register(
        env,
        "dev/ferraloom/bukkit/PlayerNative",
        &[
            (
                "sendMessage",
                "(JJLjava/lang/String;)V",
                send_message as *mut c_void,
            ),
            ("location", "(JJ)[D", location as *mut c_void),
            ("teleport", "(JJIDDDFF)Z", teleport as *mut c_void),
        ],
    )?;
    let entry = Entry::load(
        env,
        "dev/ferraloom/bukkit/PlayerEvents",
        [
            ("join", format!("(JJ{S}IZ{S}){S}")),
            ("quit", format!("(JJ{S}IDDDFF){S}")),
            ("chat", format!("(JJ{S})V")),
            ("command", format!("(JJ{S})Z")),
        ],
    )?;
    let _ = EVENTS.set(entry);
    Ok(())
}

/// `PlayerJoinEvent` (and the player's Bukkit object), after the player is
/// in the world: the join message to send instead of vanilla's, `None` for
/// none. Called for every join while plugins run, whether or not one listens.
pub fn player_join(host: &mut dyn crate::PluginHost, p: &Joined) -> Option<String> {
    let (most, least) = natives::halves(p.id);
    let events = EVENTS.get()?;
    bridge::call(host, |env| {
        let name = env.new_string(p.name)?;
        let message = env.new_string(p.message)?;
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            obj(&name),
            JValue::Int(p.entity_id).as_jni(),
            JValue::Bool(p.op.into()).as_jni(),
            obj(&message),
        ];
        // SAFETY: `join`'s signature in `load`.
        let out = unsafe { events.invoke(env, JOIN, ReturnType::Object, &args) }?;
        string_or_none(env, out)
    })
    .unwrap_or_else(|| Some(p.message.to_owned()))
}

/// `PlayerQuitEvent`, the player still online in it, then their object goes
/// offline for good: the quit message to send instead of vanilla's, `None`
/// for none. `at` is where they were.
pub fn player_quit(
    host: &mut dyn crate::PluginHost,
    id: PlayerId,
    message: &str,
    at: Location,
) -> Option<String> {
    let (most, least) = natives::halves(id);
    let events = EVENTS.get()?;
    bridge::call(host, |env| {
        let text = env.new_string(message)?;
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            obj(&text),
            JValue::Int(at.world as i32).as_jni(),
            JValue::Double(at.x).as_jni(),
            JValue::Double(at.y).as_jni(),
            JValue::Double(at.z).as_jni(),
            JValue::Float(at.yaw).as_jni(),
            JValue::Float(at.pitch).as_jni(),
        ];
        // SAFETY: `quit`'s signature in `load`.
        let out = unsafe { events.invoke(env, QUIT, ReturnType::Object, &args) }?;
        string_or_none(env, out)
    })
    .unwrap_or_else(|| Some(message.to_owned()))
}

/// A player's chat line, handed to `AsyncPlayerChatEvent` on the chat
/// thread. Its recipients get the line as the event leaves it, from the
/// start of a later tick on. No host: nothing runs on this thread.
pub fn chat(id: PlayerId, message: &str) {
    let (most, least) = natives::halves(id);
    let Some(events) = EVENTS.get() else { return };
    bridge::call_unlent(|env| {
        let text = env.new_string(message)?;
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            obj(&text),
        ];
        let void = ReturnType::Primitive(Primitive::Void);
        // SAFETY: `chat`'s signature in `load`.
        unsafe { events.invoke(env, CHAT, void, &args) }?;
        Ok(())
    });
}

/// A command line (no slash) a player sent: true when a plugin command took it.
pub fn player_command(host: &mut dyn crate::PluginHost, id: PlayerId, line: &str) -> bool {
    let (most, least) = natives::halves(id);
    let Some(events) = EVENTS.get() else {
        return false;
    };
    bridge::call(host, |env| {
        let line = env.new_string(line)?;
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            obj(&line),
        ];
        let boolean = ReturnType::Primitive(Primitive::Boolean);
        // SAFETY: `command`'s signature in `load`.
        unsafe { events.invoke(env, COMMAND, boolean, &args) }?.z()
    })
    .unwrap_or(false)
}

extern "system" fn send_message(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    message: JString,
) {
    guard(&mut env, (), |env| {
        let (player, text) = (player_id(most, least), string(env, &message)?);
        if with_host(|h| h.send_message(player, &text)).is_none() {
            natives::defer(Deferred::Message { player, text });
        }
        Ok(())
    })
}

extern "system" fn location(mut env: JNIEnv, _: JClass, most: jlong, least: jlong) -> jdoubleArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let at = with_host(|h| h.location(player_id(most, least)))
            .ok_or(Fail::NoHost("Player.getLocation"))?;
        let Some(l) = at else {
            return Ok(std::ptr::null_mut());
        };
        let values = [
            l.world as f64,
            l.x,
            l.y,
            l.z,
            f64::from(l.yaw),
            f64::from(l.pitch),
        ];
        let array = env.new_double_array(values.len() as jint)?;
        env.set_double_array_region(&array, 0, &values)?;
        Ok(array.into_raw())
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn teleport(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    world: jint,
    x: f64,
    y: f64,
    z: f64,
    yaw: jfloat,
    pitch: jfloat,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        let Ok(world) = usize::try_from(world) else {
            return Ok(JNI_FALSE);
        };
        let to = Location {
            world,
            x,
            y,
            z,
            yaw,
            pitch,
        };
        let moved = with_host(|h| h.teleport(player_id(most, least), to))
            .ok_or(Fail::NoHost("Player.teleport"))?;
        Ok(jbool(moved))
    })
}
