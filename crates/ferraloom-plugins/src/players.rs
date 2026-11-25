//! The players area: `Player` objects, what they read and change of their
//! player, what they are sent, and the player events
//! (`java/bridge/.../PlayerEvents.java`, `PlayerNative.java`).
//!
//! Java asks for a player's state as one snapshot ([`PlayerState`], one
//! native call per getter) and changes it one [`Field`] at a time; what a
//! player is sent (titles, sounds, the tab list) is one native per kind.
//! Events that CraftBukkit fires in its own `CraftPlayer` methods
//! (`teleport` and `kickPlayer` from a plugin) fire in Java; the others are
//! called from the game's sites below, each behind [`crate::listens`].
use crate::{
    Location, PlayerId,
    bridge::{self, Entry, obj, string_or_none},
    natives::{self, Deferred, Fail, guard, jbool, player_id, string, with_host},
};
use jni::{
    JNIEnv,
    objects::{JByteArray, JClass, JDoubleArray, JObject, JObjectArray, JString, JValue},
    signature::{Primitive, ReturnType},
    sys::{JNI_FALSE, jboolean, jdouble, jdoubleArray, jfloat, jint, jlong, jobjectArray, jvalue},
};
use std::{
    ffi::c_void,
    sync::{
        Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

/// What `Player` reads and changes of the game. The methods with a default
/// body answer as if the player were not on, which is what the crate's
/// test hosts need; the game implements them all.
pub trait PlayerHost {
    /// A chat line (legacy `§` codes) to one online player.
    fn send_message(&mut self, player: PlayerId, message: &str);
    /// An online player's location; none when they are not on.
    fn location(&mut self, player: PlayerId) -> Option<Location>;
    /// Moves an online player; false when it could not. No event: a
    /// plugin's teleport fired `PlayerTeleportEvent` in Java already.
    fn teleport(&mut self, player: PlayerId, to: Location) -> bool;
    /// An online player's state now.
    fn state(&mut self, _player: PlayerId) -> Option<PlayerState> {
        None
    }
    /// Sets one field of an online player's state; false when it could not.
    fn set(&mut self, _player: PlayerId, _field: Field, _value: f64) -> bool {
        false
    }
    /// The player's velocity (`Entity.setDeltaMovement`, sent to them).
    fn set_velocity(&mut self, _player: PlayerId, _velocity: [f64; 3]) -> bool {
        false
    }
    /// Sends the player something (a title, a sound, the tab list...).
    fn show(&mut self, _player: PlayerId, _what: Show) -> bool {
        false
    }
    /// The player's active effects.
    fn effects(&mut self, _player: PlayerId) -> Vec<Effect> {
        Vec::new()
    }
    /// `LivingEntity.addEffect`: whether it changed anything.
    fn add_effect(&mut self, _player: PlayerId, _effect: &Effect) -> bool {
        false
    }
    /// `LivingEntity.removeEffect`: whether the player had it.
    fn remove_effect(&mut self, _player: PlayerId, _effect: &str) -> bool {
        false
    }
    /// Disconnects the player with a reason (legacy text). No event: a
    /// plugin's kick fired `PlayerKickEvent` in Java already.
    fn kick(&mut self, _player: PlayerId, _reason: &str) -> bool {
        false
    }
    /// A vanilla command line (no slash) run as the player, at their permission level.
    fn command_as(&mut self, _player: PlayerId, _line: &str) -> bool {
        false
    }
    /// What the player's connection says of them: address, locale, client view distance.
    fn client(&mut self, _player: PlayerId) -> Option<Client> {
        None
    }
}

/// A player's state as `Player`'s getters read it, at the moment of the call.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerState {
    pub health: f64,
    pub max_health: f64,
    pub absorption: f64,
    pub food: i32,
    pub saturation: f32,
    pub exhaustion: f32,
    pub level: i32,
    /// Progress to the next level, 0 to 1.
    pub exp: f32,
    pub total_exp: i32,
    /// `GameType`'s id: 0 survival, 1 creative, 2 adventure, 3 spectator.
    pub game_mode: i32,
    pub flying: bool,
    pub allow_flight: bool,
    /// Bukkit's speeds: the abilities' values times two (0.2 walking, 0.1 flying by default).
    pub walk_speed: f32,
    pub fly_speed: f32,
    pub sneaking: bool,
    pub sprinting: bool,
    pub on_ground: bool,
    pub swimming: bool,
    pub gliding: bool,
    pub sleeping: bool,
    pub dead: bool,
    pub in_vehicle: bool,
    pub velocity: [f64; 3],
    pub fire_ticks: i32,
    pub air: i32,
    pub max_air: i32,
    pub fall_distance: f32,
    pub frozen_ticks: i32,
    /// The latency the player list shows, in milliseconds.
    pub ping: i32,
    pub ticks_lived: i32,
    pub held_slot: i32,
    /// The operator permission level (0 none, 4 owner).
    pub op_level: i32,
    pub eye_height: f64,
    pub width: f64,
    pub height: f64,
    /// Where they respawn when they have a point of their own (a bed, an anchor).
    pub respawn: Option<Location>,
}

/// Each online player's state and location at the start of the tick, for
/// reads from other threads (a plugin's async task), which Spigot answers
/// from its entity fields. Kept only once such a read happened: from then on
/// the game stores them at each tick's start ([`snapshots_wanted`]).
static SNAPSHOTS: Mutex<Vec<(PlayerId, PlayerState, Location)>> = Mutex::new(Vec::new());
static SNAPSHOTS_WANTED: AtomicBool = AtomicBool::new(false);
static SNAPSHOTS_STORED: Condvar = Condvar::new();

/// Whether the game should store [`store_snapshots`] this tick: a plugin
/// read a player's state off the server thread. One relaxed load.
pub fn snapshots_wanted() -> bool {
    SNAPSHOTS_WANTED.load(Ordering::Relaxed)
}

/// The players' state and location for this tick's reads from other threads.
pub fn store_snapshots(snapshots: Vec<(PlayerId, PlayerState, Location)>) {
    *SNAPSHOTS.lock().unwrap_or_else(|e| e.into_inner()) = snapshots;
    SNAPSHOTS_STORED.notify_all();
}

/// A player's snapshot off the server thread. The first such read waits
/// for the next tick's start to store them (once, at most a tick or two).
fn snapshot(player: PlayerId) -> Option<(PlayerState, Location)> {
    let find = |all: &[(PlayerId, PlayerState, Location)]| {
        all.iter().find(|s| s.0 == player).map(|s| (s.1, s.2))
    };
    let mut all = SNAPSHOTS.lock().unwrap_or_else(|e| e.into_inner());
    if !SNAPSHOTS_WANTED.swap(true, Ordering::Relaxed) {
        all = SNAPSHOTS_STORED
            .wait_timeout(all, Duration::from_millis(200))
            .map_or_else(|e| e.into_inner().0, |(g, _)| g);
    }
    find(&all)
}

/// The length of [`PlayerState::to_array`]: `PlayerNative.state`'s answer.
const STATE_LEN: usize = 41;

impl PlayerState {
    /// The order `PlayerState.java`'s indices name.
    fn to_array(self) -> [f64; STATE_LEN] {
        let flag = |v: bool| f64::from(u8::from(v));
        let r = self.respawn;
        [
            self.health,
            self.max_health,
            self.absorption,
            f64::from(self.food),
            f64::from(self.saturation),
            f64::from(self.exhaustion),
            f64::from(self.level),
            f64::from(self.exp),
            f64::from(self.total_exp),
            f64::from(self.game_mode),
            flag(self.flying),
            flag(self.allow_flight),
            f64::from(self.walk_speed),
            f64::from(self.fly_speed),
            flag(self.sneaking),
            flag(self.sprinting),
            flag(self.on_ground),
            flag(self.swimming),
            flag(self.gliding),
            flag(self.sleeping),
            flag(self.dead),
            flag(self.in_vehicle),
            self.velocity[0],
            self.velocity[1],
            self.velocity[2],
            f64::from(self.fire_ticks),
            f64::from(self.air),
            f64::from(self.max_air),
            f64::from(self.fall_distance),
            f64::from(self.frozen_ticks),
            f64::from(self.ping),
            f64::from(self.ticks_lived),
            f64::from(self.held_slot),
            f64::from(self.op_level),
            self.eye_height,
            self.width,
            self.height,
            r.map_or(-1.0, |l| l.world as f64),
            r.map_or(0.0, |l| l.x),
            r.map_or(0.0, |l| l.y),
            r.map_or(0.0, |l| l.z),
        ]
    }
}

/// One settable part of a player's state: `PlayerNative.set`'s `field`,
/// the constants of `PlayerState.java`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Health,
    Food,
    Saturation,
    Exhaustion,
    Level,
    /// Progress to the next level.
    Exp,
    TotalExp,
    /// `Player.giveExperiencePoints`.
    GiveExp,
    /// `Player.giveExperienceLevels`.
    GiveLevels,
    GameMode,
    Flying,
    FireTicks,
    Air,
    FallDistance,
    Sneaking,
    Sprinting,
    HeldSlot,
    Absorption,
    /// `Player.setAllowFlight`: flight beyond the game mode's.
    AllowFlight,
}

impl Field {
    const ALL: [Field; 19] = [
        Field::Health,
        Field::Food,
        Field::Saturation,
        Field::Exhaustion,
        Field::Level,
        Field::Exp,
        Field::TotalExp,
        Field::GiveExp,
        Field::GiveLevels,
        Field::GameMode,
        Field::Flying,
        Field::FireTicks,
        Field::Air,
        Field::FallDistance,
        Field::Sneaking,
        Field::Sprinting,
        Field::HeldSlot,
        Field::Absorption,
        Field::AllowFlight,
    ];
}

/// What a player can be sent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Show<'a> {
    /// Legacy text over the hotbar.
    ActionBar(&'a str),
    /// `CraftPlayer.sendTitle`: the times, then the title and subtitle given.
    Title {
        title: Option<&'a str>,
        subtitle: Option<&'a str>,
        fade_in: i32,
        stay: i32,
        fade_out: i32,
    },
    /// `ClientboundClearTitlesPacket(true)`.
    ResetTitle,
    TabList {
        header: &'a str,
        footer: &'a str,
    },
    /// A sound event by name (`minecraft:block.note_block.pling`) at a place.
    Sound {
        sound: &'a str,
        /// `SoundSource`'s ordinal (Bukkit's `SoundCategory`).
        category: i32,
        at: [f64; 3],
        volume: f32,
        pitch: f32,
        seed: i64,
    },
    /// Stops a sound, a category, or (both none) everything.
    StopSound {
        sound: Option<&'a str>,
        category: Option<i32>,
    },
    /// A particle type without data, as `ServerLevel.sendParticles` sends it.
    Particle {
        particle: &'a str,
        at: [f64; 3],
        count: i32,
        offset: [f32; 3],
        speed: f32,
        force: bool,
    },
    /// A block state (`minecraft:stone`, `minecraft:oak_log[axis=x]`) the client is told is there.
    BlockChange {
        pos: [i32; 3],
        state: &'a str,
    },
    /// A custom payload on a channel (`namespace:path`).
    PluginMessage {
        channel: &'a str,
        data: &'a [u8],
    },
    /// The player list's name for the player, to everyone; none: their own.
    ListName(Option<&'a str>),
    /// The command tree again (`Player.updateCommands`).
    Commands,
}

/// An effect as `PotionEffect` has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    /// `minecraft:speed`.
    pub id: String,
    /// Ticks; -1 infinite.
    pub duration: i32,
    pub amplifier: i32,
    pub ambient: bool,
    pub particles: bool,
    pub icon: bool,
}

impl Effect {
    /// `id duration amplifier ambient particles icon`, as `FerraloomPlayer` parses it.
    fn line(&self) -> String {
        let b = |v: bool| u8::from(v);
        format!(
            "{} {} {} {} {} {}",
            self.id,
            self.duration,
            self.amplifier,
            b(self.ambient),
            b(self.particles),
            b(self.icon)
        )
    }
}

/// What a player's connection says of them.
#[derive(Clone, Debug, PartialEq)]
pub struct Client {
    pub address: std::net::SocketAddr,
    /// `en_us`.
    pub locale: String,
    pub view_distance: i32,
}

/// A player who just joined, for `PlayerJoinEvent`.
pub struct Joined<'a> {
    pub id: PlayerId,
    pub name: &'a str,
    pub entity_id: i32,
    pub op: bool,
    /// The join message vanilla would send, as legacy text (`§eSteve joined the game`).
    pub message: &'a str,
    /// Their player file existed before this join (`OfflinePlayer.hasPlayedBefore`).
    pub played_before: bool,
    /// `getFirstPlayed` and `getLastPlayed`, in milliseconds since the epoch (0: unknown).
    pub first_played: i64,
    pub last_played: i64,
}

/// What a player-moving event's listeners decided.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Moved {
    /// The move goes on as it was.
    Unchanged,
    /// Cancelled: the player goes back to `from`.
    Cancelled,
    /// A listener chose another destination.
    To(Location),
}

/// `PlayerInteractEvent.Action`'s ordinals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    LeftClickBlock = 0,
    RightClickBlock = 1,
    LeftClickAir = 2,
    RightClickAir = 3,
    Physical = 4,
}

/// A `PlayerInteractEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interact<'a> {
    pub player: PlayerId,
    pub action: Action,
    /// The held item's id (`minecraft:air` for none): `getMaterial`.
    pub item: &'a str,
    /// The block clicked: world, position, `BlockFace`'s ordinal.
    pub block: Option<(usize, [i32; 3], i32)>,
    /// The off hand's turn.
    pub off_hand: bool,
    /// Where on the block, relative to its corner (`getClickedPosition`).
    pub point: Option<[f64; 3]>,
}

/// What `PlayerInteractEvent`'s listeners left to happen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Interaction {
    /// `useInteractedBlock() == DENY`.
    pub deny_block: bool,
    /// `useItemInHand() == DENY`.
    pub deny_item: bool,
}

/// `PlayerTeleportEvent.TeleportCause`'s ordinals the game's sites use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    EnderPearl = 0,
    Command = 1,
    Plugin = 2,
    NetherPortal = 3,
    EndPortal = 4,
    Spectate = 5,
    EndGateway = 6,
    ChorusFruit = 7,
    Dismount = 8,
    ExitBed = 9,
    Unknown = 10,
}

/// A player's toggle, for `PlayerToggleSneakEvent`, `SprintEvent` and `FlightEvent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    Sneak = 0,
    Sprint = 1,
    Flight = 2,
}

/// The vanilla verdict on a login, for `PlayerLoginEvent.Result`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginResult {
    Allowed = 0,
    KickFull = 1,
    KickBanned = 2,
    KickWhitelist = 3,
    KickOther = 4,
}

const S: &str = "Ljava/lang/String;";
const LOC: &str = "IDDDFF";

// `PlayerEvents`' entry points, by index.
const JOIN: usize = 0;
const QUIT: usize = 1;
const CHAT: usize = 2;
const COMMAND: usize = 3;
const PREPROCESS: usize = 4;
const PRE_LOGIN: usize = 5;
const LOGIN: usize = 6;
const KICK: usize = 7;
const MOVE: usize = 8;
const TELEPORT: usize = 9;
const INTERACT: usize = 10;
const RESPAWN: usize = 11;
const GAME_MODE: usize = 12;
const TOGGLE: usize = 13;
const CHANGED_WORLD: usize = 14;
const ITEM_HELD: usize = 15;
const PLUGIN_MESSAGE: usize = 16;
const DEATH: usize = 17;
const ENTRIES: usize = 18;

static EVENTS: OnceLock<Entry<ENTRIES>> = OnceLock::new();

/// At start: `PlayerNative`'s methods bound, `PlayerEvents`' looked up.
pub(crate) fn load(env: &mut JNIEnv) -> jni::errors::Result<()> {
    // `$` stands for `String` in these signatures.
    let sig = |s: &str| s.replace('$', S);
    let methods: Vec<(&str, String, *mut c_void)> = vec![
        ("sendMessage", sig("(JJ$)V"), send_message as *mut c_void),
        ("location", "(JJ)[D".into(), location as *mut c_void),
        ("teleport", "(JJIDDDFF)Z".into(), teleport as *mut c_void),
        ("state", "(JJ)[D".into(), state as *mut c_void),
        ("set", "(JJID)Z".into(), set as *mut c_void),
        (
            "setVelocity",
            "(JJDDD)Z".into(),
            set_velocity as *mut c_void,
        ),
        ("actionBar", sig("(JJ$)Z"), action_bar as *mut c_void),
        ("title", sig("(JJ$$III)Z"), title as *mut c_void),
        ("resetTitle", "(JJ)Z".into(), reset_title as *mut c_void),
        ("tabList", sig("(JJ$$)Z"), tab_list as *mut c_void),
        ("sound", sig("(JJ$IDDDFFJ)Z"), sound as *mut c_void),
        ("stopSound", sig("(JJ$I)Z"), stop_sound as *mut c_void),
        ("particle", sig("(JJ$DDDIFFFFZ)Z"), particle as *mut c_void),
        ("blockChange", sig("(JJIII$)Z"), block_change as *mut c_void),
        (
            "pluginMessage",
            sig("(JJ$[B)Z"),
            send_plugin_message as *mut c_void,
        ),
        ("listName", sig("(JJ$)Z"), list_name as *mut c_void),
        (
            "updateCommands",
            "(JJ)Z".into(),
            update_commands as *mut c_void,
        ),
        ("effects", format!("(JJ)[{S}"), effects as *mut c_void),
        ("addEffect", sig("(JJ$IIZZZ)Z"), add_effect as *mut c_void),
        ("removeEffect", sig("(JJ$)Z"), remove_effect as *mut c_void),
        ("kick", sig("(JJ$)Z"), kick as *mut c_void),
        ("command", sig("(JJ$)Z"), command_as as *mut c_void),
        ("client", format!("(JJ)[{S}"), client as *mut c_void),
    ];
    let methods: Vec<(&str, &str, *mut c_void)> = methods
        .iter()
        .map(|(n, s, f)| (*n, s.as_str(), *f))
        .collect();
    natives::register(env, "dev/ferraloom/bukkit/PlayerNative", &methods)?;
    let entry = Entry::load(
        env,
        "dev/ferraloom/bukkit/PlayerEvents",
        [
            ("join", sig("(JJ$IZ$ZJJ)$")),
            ("quit", sig(&format!("(JJ${LOC})$"))),
            ("chat", sig("(JJ$)V")),
            ("command", sig("(JJ$)Z")),
            ("preprocess", sig("(JJ$)$")),
            ("preLogin", sig("(JJ$$Z)$")),
            ("login", sig("(JJ$$$ZI$)$")),
            ("kick", sig("(JJ$$)[$")),
            ("move", format!("(JJ{LOC}{LOC})[D")),
            ("teleport", format!("(JJ{LOC}{LOC}I)[D")),
            ("interact", sig("(JJI$IIIIIZDDDZ)I")),
            ("respawn", format!("(JJ{LOC}ZZ)[D")),
            ("gameModeChange", "(JJI)Z".into()),
            ("toggle", "(JJIZ)Z".into()),
            ("changedWorld", "(JJI)V".into()),
            ("itemHeld", "(JJII)Z".into()),
            ("pluginMessage", sig("(JJ$[B)V")),
            ("death", sig("(JJ$IIZZ)[$")),
        ],
    )?;
    let _ = EVENTS.set(entry);
    Ok(())
}

/// A player's UUID halves as arguments.
fn id_args(id: PlayerId) -> [jvalue; 2] {
    let (most, least) = natives::halves(id);
    [JValue::Long(most).as_jni(), JValue::Long(least).as_jni()]
}

/// A location's arguments, `LOC` in the signatures.
fn location_args(l: &Location) -> [jvalue; 6] {
    [
        JValue::Int(l.world as i32).as_jni(),
        JValue::Double(l.x).as_jni(),
        JValue::Double(l.y).as_jni(),
        JValue::Double(l.z).as_jni(),
        JValue::Float(l.yaw).as_jni(),
        JValue::Float(l.pitch).as_jni(),
    ]
}

/// `{world, x, y, z, yaw, pitch}` as Java answers a location.
fn location_of(v: &[f64]) -> Option<Location> {
    let [world, x, y, z, yaw, pitch] = v[..] else {
        return None;
    };
    Some(Location {
        world: usize::try_from(world as i64).ok()?,
        x,
        y,
        z,
        yaw: yaw as f32,
        pitch: pitch as f32,
    })
}

/// A returned `double[]`: none for null.
fn doubles(
    env: &mut JNIEnv,
    value: jni::objects::JValueOwned,
) -> jni::errors::Result<Option<Vec<f64>>> {
    let o = value.l()?;
    if o.is_null() {
        return Ok(None);
    }
    let array = JDoubleArray::from(o);
    let mut out = vec![0.0; env.get_array_length(&array)? as usize];
    env.get_double_array_region(&array, 0, &mut out)?;
    Ok(Some(out))
}

/// A `Moved` from the convention of `move` and `teleport`: null unchanged,
/// empty cancelled, else the new destination.
fn moved(answer: Option<Vec<f64>>) -> Moved {
    match answer {
        None => Moved::Unchanged,
        Some(v) if v.is_empty() => Moved::Cancelled,
        Some(v) => location_of(&v).map_or(Moved::Unchanged, Moved::To),
    }
}

fn boolean() -> ReturnType {
    ReturnType::Primitive(Primitive::Boolean)
}

fn void() -> ReturnType {
    ReturnType::Primitive(Primitive::Void)
}

/// `PlayerJoinEvent` (and the player's Bukkit object, the one its login made),
/// after the player is in the world: the join message to send instead of
/// vanilla's, `None` for none. Called for every join while plugins run,
/// whether or not one listens.
pub fn player_join(host: &mut dyn crate::PluginHost, p: &Joined) -> Option<String> {
    let events = EVENTS.get()?;
    bridge::call(host, |env| {
        let name = env.new_string(p.name)?;
        let message = env.new_string(p.message)?;
        let [most, least] = id_args(p.id);
        let args = [
            most,
            least,
            obj(&name),
            JValue::Int(p.entity_id).as_jni(),
            JValue::Bool(p.op.into()).as_jni(),
            obj(&message),
            JValue::Bool(p.played_before.into()).as_jni(),
            JValue::Long(p.first_played).as_jni(),
            JValue::Long(p.last_played).as_jni(),
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
    let events = EVENTS.get()?;
    bridge::call(host, |env| {
        let text = env.new_string(message)?;
        let [most, least] = id_args(id);
        let mut args = vec![most, least, obj(&text)];
        args.extend(location_args(&at));
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
    let Some(events) = EVENTS.get() else { return };
    bridge::call_unlent(|env| {
        let text = env.new_string(message)?;
        let [most, least] = id_args(id);
        // SAFETY: `chat`'s signature in `load`.
        unsafe { events.invoke(env, CHAT, void(), &[most, least, obj(&text)]) }?;
        Ok(())
    });
}

/// A command line (no slash) a player sent: true when a plugin command took it.
pub fn player_command(host: &mut dyn crate::PluginHost, id: PlayerId, line: &str) -> bool {
    let Some(events) = EVENTS.get() else {
        return false;
    };
    bridge::call(host, |env| {
        let line = env.new_string(line)?;
        let [most, least] = id_args(id);
        // SAFETY: `command`'s signature in `load`.
        unsafe { events.invoke(env, COMMAND, boolean(), &[most, least, obj(&line)]) }?.z()
    })
    .unwrap_or(false)
}

/// `PlayerCommandPreprocessEvent` for a command line (no slash): the line
/// to run, `None` when a listener cancelled it.
pub fn command_preprocess(
    host: &mut dyn crate::PluginHost,
    id: PlayerId,
    line: &str,
) -> Option<String> {
    let Some(events) = EVENTS.get() else {
        return Some(line.to_owned());
    };
    bridge::call(host, |env| {
        let text = env.new_string(line)?;
        let [most, least] = id_args(id);
        // SAFETY: `preprocess`'s signature in `load`.
        let out = unsafe {
            events.invoke(
                env,
                PREPROCESS,
                ReturnType::Object,
                &[most, least, obj(&text)],
            )
        }?;
        string_or_none(env, out)
    })
    .unwrap_or_else(|| Some(line.to_owned()))
}

/// A login waiting for `PlayerLoginEvent` on the server thread.
struct Login {
    id: PlayerId,
    name: String,
    address: std::net::SocketAddr,
    /// The address the client connected to (`getHostname`).
    hostname: String,
    op: bool,
    vanilla: LoginResult,
    message: String,
    answer: mpsc::SyncSender<Option<String>>,
}

static LOGINS: Mutex<Vec<Login>> = Mutex::new(Vec::new());
/// Some login waits: lets [`serve_logins`] skip the lock.
static LOGINS_WAITING: AtomicBool = AtomicBool::new(false);

/// How long a login waits for the server thread before it goes on with
/// vanilla's verdict (the server stopping, say).
const LOGIN_WAIT: Duration = Duration::from_secs(10);

/// A login on its connection's thread, after authentication:
/// `AsyncPlayerPreLoginEvent` here, then `PlayerLoginEvent` on the server
/// thread (this one waits for it) with vanilla's verdict, which a listener
/// may change. Answers the kick message (legacy text), `None` to let them in.
/// Without plugins, vanilla's verdict as it was.
pub fn login(
    id: PlayerId,
    name: &str,
    address: std::net::SocketAddr,
    hostname: &str,
    op: bool,
    vanilla: LoginResult,
    message: &str,
) -> Option<String> {
    let refused = (vanilla != LoginResult::Allowed).then(|| message.to_owned());
    let Some(events) = EVENTS.get() else {
        return refused;
    };
    let pre = bridge::call_attached(|env| {
        let name = env.new_string(name)?;
        let ip = env.new_string(address.ip().to_string())?;
        let [most, least] = id_args(id);
        let args = [most, least, obj(&name), obj(&ip), JValue::Bool(0).as_jni()];
        // SAFETY: `preLogin`'s signature in `load`.
        let out = unsafe { events.invoke(env, PRE_LOGIN, ReturnType::Object, &args) }?;
        string_or_none(env, out)
    });
    if let Some(Some(kick)) = pre {
        return Some(kick);
    }
    let (answer, wait) = mpsc::sync_channel(1);
    LOGINS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(Login {
            id,
            name: name.to_owned(),
            address,
            hostname: hostname.to_owned(),
            op,
            vanilla,
            message: message.to_owned(),
            answer,
        });
    LOGINS_WAITING.store(true, Ordering::Release);
    wait.recv_timeout(LOGIN_WAIT).unwrap_or(refused)
}

/// The logins waiting for `PlayerLoginEvent`, on the server thread (even
/// while the server is paused: vanilla handles logins then too).
pub fn serve_logins(host: &mut dyn crate::PluginHost) {
    if !LOGINS_WAITING.swap(false, Ordering::Acquire) {
        return;
    }
    let waiting = std::mem::take(&mut *LOGINS.lock().unwrap_or_else(|e| e.into_inner()));
    let Some(events) = EVENTS.get() else { return };
    for l in waiting {
        let refused = (l.vanilla != LoginResult::Allowed).then(|| l.message.clone());
        let verdict = bridge::call(host, |env| {
            let name = env.new_string(&l.name)?;
            let ip = env.new_string(l.address.ip().to_string())?;
            let hostname = env.new_string(&l.hostname)?;
            let message = env.new_string(&l.message)?;
            let [most, least] = id_args(l.id);
            let args = [
                most,
                least,
                obj(&name),
                obj(&ip),
                obj(&hostname),
                JValue::Bool(l.op.into()).as_jni(),
                JValue::Int(l.vanilla as i32).as_jni(),
                obj(&message),
            ];
            // SAFETY: `login`'s signature in `load`.
            let out = unsafe { events.invoke(env, LOGIN, ReturnType::Object, &args) }?;
            string_or_none(env, out)
        });
        let _ = l.answer.send(verdict.unwrap_or(refused));
    }
}

/// `PlayerKickEvent` for a kick the server decided (a command, idling, a
/// ban): the reason to kick with, `None` when a listener cancelled it.
pub fn player_kick(host: &mut dyn crate::PluginHost, id: PlayerId, reason: &str) -> Option<String> {
    let Some(events) = EVENTS.get() else {
        return Some(reason.to_owned());
    };
    bridge::call(host, |env| {
        let text = env.new_string(reason)?;
        let leave = env.new_string("")?;
        let [most, least] = id_args(id);
        let args = [most, least, obj(&text), obj(&leave)];
        // SAFETY: `kick`'s signature in `load`.
        let out = unsafe { events.invoke(env, KICK, ReturnType::Array, &args) }?.l()?;
        if out.is_null() {
            return Ok(None);
        }
        let out = JObjectArray::from(out);
        let reason = JString::from(env.get_object_array_element(&out, 0)?);
        Ok(Some(env.get_string(&reason)?.into()))
    })
    .unwrap_or_else(|| Some(reason.to_owned()))
}

/// `PlayerMoveEvent` from `from` to `to`, before the move is applied.
pub fn player_move(
    host: &mut dyn crate::PluginHost,
    id: PlayerId,
    from: Location,
    to: Location,
) -> Moved {
    let Some(events) = EVENTS.get() else {
        return Moved::Unchanged;
    };
    // On the stack: this is called for every moving player each tick.
    let args = move_args(id, &from, &to, None);
    let answer = bridge::call(host, |env| {
        // SAFETY: `move`'s signature in `load`.
        let out = unsafe { events.invoke(env, MOVE, ReturnType::Array, &args[..14]) }?;
        doubles(env, out)
    });
    moved(answer.flatten())
}

/// `move` and `teleport`'s arguments: the player, both locations, the cause (none for `move`).
fn move_args(id: PlayerId, from: &Location, to: &Location, cause: Option<Cause>) -> [jvalue; 15] {
    let [most, least] = id_args(id);
    let [fw, fx, fy, fz, fyaw, fpitch] = location_args(from);
    let [tw, tx, ty, tz, tyaw, tpitch] = location_args(to);
    let cause = JValue::Int(cause.map_or(0, |c| c as i32)).as_jni();
    [
        most, least, fw, fx, fy, fz, fyaw, fpitch, tw, tx, ty, tz, tyaw, tpitch, cause,
    ]
}

/// `PlayerTeleportEvent` for a teleport the server makes (a command, a
/// pearl, a portal), before the player moves.
pub fn player_teleport(
    host: &mut dyn crate::PluginHost,
    id: PlayerId,
    from: Location,
    to: Location,
    cause: Cause,
) -> Moved {
    let Some(events) = EVENTS.get() else {
        return Moved::Unchanged;
    };
    let args = move_args(id, &from, &to, Some(cause));
    let answer = bridge::call(host, |env| {
        // SAFETY: `teleport`'s signature in `load`.
        let out = unsafe { events.invoke(env, TELEPORT, ReturnType::Array, &args) }?;
        doubles(env, out)
    });
    moved(answer.flatten())
}

/// `PlayerInteractEvent`: what its listeners left to happen.
pub fn player_interact(host: &mut dyn crate::PluginHost, i: &Interact) -> Interaction {
    let Some(events) = EVENTS.get() else {
        return Interaction::default();
    };
    let bits = bridge::call(host, |env| {
        let item = env.new_string(i.item)?;
        let (world, pos, face) = i.block.unwrap_or((usize::MAX, [0; 3], 0));
        let point = i.point.unwrap_or_default();
        let mut args = id_args(i.player).to_vec();
        args.extend([
            JValue::Int(i.action as i32).as_jni(),
            obj(&item),
            JValue::Int(if i.block.is_some() { world as i32 } else { -1 }).as_jni(),
            JValue::Int(pos[0]).as_jni(),
            JValue::Int(pos[1]).as_jni(),
            JValue::Int(pos[2]).as_jni(),
            JValue::Int(face).as_jni(),
            JValue::Bool(i.off_hand.into()).as_jni(),
            JValue::Double(point[0]).as_jni(),
            JValue::Double(point[1]).as_jni(),
            JValue::Double(point[2]).as_jni(),
            JValue::Bool(i.point.is_some().into()).as_jni(),
        ]);
        let int = ReturnType::Primitive(Primitive::Int);
        // SAFETY: `interact`'s signature in `load`.
        unsafe { events.invoke(env, INTERACT, int, &args) }?.i()
    })
    .unwrap_or(0);
    Interaction {
        deny_block: bits & 1 != 0,
        deny_item: bits & 2 != 0,
    }
}

/// `PlayerRespawnEvent`: where the player respawns, as its listeners leave it.
pub fn player_respawn(
    host: &mut dyn crate::PluginHost,
    id: PlayerId,
    at: Location,
    bed: bool,
    anchor: bool,
) -> Location {
    let Some(events) = EVENTS.get() else {
        return at;
    };
    let answer = bridge::call(host, |env| {
        let mut args = id_args(id).to_vec();
        args.extend(location_args(&at));
        args.push(JValue::Bool(bed.into()).as_jni());
        args.push(JValue::Bool(anchor.into()).as_jni());
        // SAFETY: `respawn`'s signature in `load`.
        let out = unsafe { events.invoke(env, RESPAWN, ReturnType::Array, &args) }?;
        doubles(env, out)
    });
    answer.flatten().and_then(|v| location_of(&v)).unwrap_or(at)
}

/// `PlayerGameModeChangeEvent` before the change: true when cancelled.
pub fn player_game_mode_change(host: &mut dyn crate::PluginHost, id: PlayerId, mode: i32) -> bool {
    let Some(events) = EVENTS.get() else {
        return false;
    };
    bridge::call(host, |env| {
        let [most, least] = id_args(id);
        let args = [most, least, JValue::Int(mode).as_jni()];
        // SAFETY: `gameModeChange`'s signature in `load`.
        unsafe { events.invoke(env, GAME_MODE, boolean(), &args) }?.z()
    })
    .unwrap_or(false)
}

/// `PlayerToggleSneakEvent`, `SprintEvent` or `FlightEvent` for a player
/// turning `on` (or off): true when cancelled.
pub fn player_toggle(
    host: &mut dyn crate::PluginHost,
    id: PlayerId,
    toggle: Toggle,
    on: bool,
) -> bool {
    let Some(events) = EVENTS.get() else {
        return false;
    };
    bridge::call(host, |env| {
        let [most, least] = id_args(id);
        let args = [
            most,
            least,
            JValue::Int(toggle as i32).as_jni(),
            JValue::Bool(on.into()).as_jni(),
        ];
        // SAFETY: `toggle`'s signature in `load`.
        unsafe { events.invoke(env, TOGGLE, boolean(), &args) }?.z()
    })
    .unwrap_or(false)
}

/// `PlayerChangedWorldEvent`, after the player arrived; `from` is the world they left.
pub fn player_changed_world(host: &mut dyn crate::PluginHost, id: PlayerId, from: usize) {
    let Some(events) = EVENTS.get() else { return };
    bridge::call(host, |env| {
        let [most, least] = id_args(id);
        let args = [most, least, JValue::Int(from as i32).as_jni()];
        // SAFETY: `changedWorld`'s signature in `load`.
        unsafe { events.invoke(env, CHANGED_WORLD, void(), &args) }?;
        Ok(())
    });
}

/// `PlayerItemHeldEvent` for a hotbar slot change: true when cancelled.
pub fn player_item_held(
    host: &mut dyn crate::PluginHost,
    id: PlayerId,
    previous: i32,
    new: i32,
) -> bool {
    let Some(events) = EVENTS.get() else {
        return false;
    };
    bridge::call(host, |env| {
        let [most, least] = id_args(id);
        let args = [
            most,
            least,
            JValue::Int(previous).as_jni(),
            JValue::Int(new).as_jni(),
        ];
        // SAFETY: `itemHeld`'s signature in `load`.
        unsafe { events.invoke(env, ITEM_HELD, boolean(), &args) }?.z()
    })
    .unwrap_or(false)
}

/// A custom payload a player's client sent, for the plugins registered on
/// its channel (and `minecraft:register`/`unregister`, the channels it listens on).
pub fn plugin_message(host: &mut dyn crate::PluginHost, id: PlayerId, channel: &str, data: &[u8]) {
    let Some(events) = EVENTS.get() else { return };
    bridge::call(host, |env| {
        let channel = env.new_string(channel)?;
        let data = env.byte_array_from_slice(data)?;
        let [most, least] = id_args(id);
        let args = [most, least, obj(&channel), obj(&data)];
        // SAFETY: `pluginMessage`'s signature in `load`.
        unsafe { events.invoke(env, PLUGIN_MESSAGE, void(), &args) }?;
        Ok(())
    });
}

/// A player's death as `PlayerDeathEvent` leaves it.
#[derive(Clone, Debug, PartialEq)]
pub struct Death {
    /// The death message (legacy text); none for none.
    pub message: Option<String>,
    /// The experience the body drops.
    pub dropped_exp: i32,
    pub keep_inventory: bool,
    pub keep_level: bool,
}

/// `PlayerDeathEvent` as the player dies, before anything is dropped or said.
pub fn player_death(host: &mut dyn crate::PluginHost, id: PlayerId, death: Death) -> Death {
    let Some(events) = EVENTS.get() else {
        return death;
    };
    let answer = bridge::call(host, |env| {
        let message = match &death.message {
            Some(m) => JObject::from(env.new_string(m)?),
            None => JObject::null(),
        };
        let [most, least] = id_args(id);
        let args = [
            most,
            least,
            obj(&message),
            JValue::Int(death.dropped_exp).as_jni(),
            JValue::Int(0).as_jni(),
            JValue::Bool(death.keep_inventory.into()).as_jni(),
            JValue::Bool(death.keep_level.into()).as_jni(),
        ];
        // SAFETY: `death`'s signature in `load`.
        let out = unsafe { events.invoke(env, DEATH, ReturnType::Array, &args) }?.l()?;
        if out.is_null() {
            return Ok(None);
        }
        let out = JObjectArray::from(out);
        let mut fields = Vec::new();
        for i in 0..env.get_array_length(&out)? {
            let s = env.get_object_array_element(&out, i)?;
            fields.push(if s.is_null() {
                None
            } else {
                Some(String::from(env.get_string(&JString::from(s))?))
            });
        }
        Ok(Some(fields))
    });
    // {message, droppedExp, keepInventory, keepLevel}.
    let Some(Some(fields)) = answer else {
        return death;
    };
    let field = |i: usize| fields.get(i).cloned().flatten();
    let flag = |i: usize, default: bool| field(i).map_or(default, |v| v == "true");
    Death {
        message: field(0),
        dropped_exp: field(1)
            .and_then(|v| v.parse().ok())
            .unwrap_or(death.dropped_exp),
        keep_inventory: flag(2, death.keep_inventory),
        keep_level: flag(3, death.keep_level),
    }
}

// Natives.

/// The host for one native, or `IllegalStateException` naming the API method.
fn on_host<R>(
    what: &'static str,
    f: impl FnOnce(&mut dyn crate::PluginHost) -> R,
) -> Result<R, Fail> {
    with_host(f).ok_or(Fail::NoHost(what))
}

/// A Java string that may be null.
fn optional(env: &mut JNIEnv, s: &JString) -> Result<Option<String>, Fail> {
    if s.is_null() {
        return Ok(None);
    }
    string(env, s).map(Some)
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

/// A Java `double[]` of `values`.
fn double_array(env: &mut JNIEnv, values: &[f64]) -> Result<jdoubleArray, Fail> {
    let array = env.new_double_array(values.len() as jint)?;
    env.set_double_array_region(&array, 0, values)?;
    Ok(array.into_raw())
}

extern "system" fn location(mut env: JNIEnv, _: JClass, most: jlong, least: jlong) -> jdoubleArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let player = player_id(most, least);
        let at = with_host(|h| h.location(player)).or_else(|| Some(snapshot(player).map(|s| s.1)));
        let Some(Some(l)) = at else {
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
        double_array(env, &values)
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn teleport(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    world: jint,
    x: jdouble,
    y: jdouble,
    z: jdouble,
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
        let moved = on_host("Player.teleport", |h| {
            h.teleport(player_id(most, least), to)
        })?;
        Ok(jbool(moved))
    })
}

extern "system" fn state(mut env: JNIEnv, _: JClass, most: jlong, least: jlong) -> jdoubleArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let player = player_id(most, least);
        let live = with_host(|h| h.state(player));
        match live.unwrap_or_else(|| snapshot(player).map(|s| s.0)) {
            Some(s) => double_array(env, &s.to_array()),
            None => Ok(std::ptr::null_mut()),
        }
    })
}

extern "system" fn set(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    field: jint,
    value: jdouble,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        let Some(&field) = usize::try_from(field).ok().and_then(|i| Field::ALL.get(i)) else {
            return Ok(JNI_FALSE);
        };
        let done = on_host("Player state", |h| {
            h.set(player_id(most, least), field, value)
        })?;
        Ok(jbool(done))
    })
}

extern "system" fn set_velocity(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    x: jdouble,
    y: jdouble,
    z: jdouble,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        let done = on_host("Player.setVelocity", |h| {
            h.set_velocity(player_id(most, least), [x, y, z])
        })?;
        Ok(jbool(done))
    })
}

/// A `show` native's body: the host shows `what` to the player.
fn show(most: jlong, least: jlong, api: &'static str, what: Show) -> Result<jboolean, Fail> {
    Ok(jbool(on_host(api, |h| {
        h.show(player_id(most, least), what)
    })?))
}

extern "system" fn action_bar(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    text: JString,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let text = string(env, &text)?;
        show(most, least, "Player.sendActionBar", Show::ActionBar(&text))
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn title(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    title: JString,
    subtitle: JString,
    fade_in: jint,
    stay: jint,
    fade_out: jint,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let (title, subtitle) = (optional(env, &title)?, optional(env, &subtitle)?);
        let what = Show::Title {
            title: title.as_deref(),
            subtitle: subtitle.as_deref(),
            fade_in,
            stay,
            fade_out,
        };
        show(most, least, "Player.sendTitle", what)
    })
}

extern "system" fn reset_title(mut env: JNIEnv, _: JClass, most: jlong, least: jlong) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        show(most, least, "Player.resetTitle", Show::ResetTitle)
    })
}

extern "system" fn tab_list(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    header: JString,
    footer: JString,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let (header, footer) = (string(env, &header)?, string(env, &footer)?);
        let what = Show::TabList {
            header: &header,
            footer: &footer,
        };
        show(most, least, "Player.setPlayerListHeaderFooter", what)
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn sound(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    sound: JString,
    category: jint,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    volume: jfloat,
    pitch: jfloat,
    seed: jlong,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let sound = string(env, &sound)?;
        let what = Show::Sound {
            sound: &sound,
            category,
            at: [x, y, z],
            volume,
            pitch,
            seed,
        };
        show(most, least, "Player.playSound", what)
    })
}

extern "system" fn stop_sound(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    sound: JString,
    category: jint,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let sound = optional(env, &sound)?;
        let what = Show::StopSound {
            sound: sound.as_deref(),
            category: (category >= 0).then_some(category),
        };
        show(most, least, "Player.stopSound", what)
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn particle(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    particle: JString,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    count: jint,
    dx: jfloat,
    dy: jfloat,
    dz: jfloat,
    speed: jfloat,
    force: jboolean,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let particle = string(env, &particle)?;
        let what = Show::Particle {
            particle: &particle,
            at: [x, y, z],
            count,
            offset: [dx, dy, dz],
            speed,
            force: force != 0,
        };
        show(most, least, "Player.spawnParticle", what)
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn block_change(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    x: jint,
    y: jint,
    z: jint,
    state: JString,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let state = string(env, &state)?;
        let what = Show::BlockChange {
            pos: [x, y, z],
            state: &state,
        };
        show(most, least, "Player.sendBlockChange", what)
    })
}

extern "system" fn send_plugin_message(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    channel: JString,
    data: JByteArray,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let channel = string(env, &channel)?;
        let data = env.convert_byte_array(&data)?;
        let what = Show::PluginMessage {
            channel: &channel,
            data: &data,
        };
        show(most, least, "Player.sendPluginMessage", what)
    })
}

extern "system" fn list_name(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    name: JString,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let name = optional(env, &name)?;
        show(
            most,
            least,
            "Player.setPlayerListName",
            Show::ListName(name.as_deref()),
        )
    })
}

extern "system" fn update_commands(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        show(most, least, "Player.updateCommands", Show::Commands)
    })
}

/// A Java `String[]` of `lines`.
fn string_array(env: &mut JNIEnv, lines: &[String]) -> Result<jobjectArray, Fail> {
    let array = env.new_object_array(lines.len() as jint, "java/lang/String", JObject::null())?;
    for (i, line) in lines.iter().enumerate() {
        let s = env.new_string(line)?;
        env.set_object_array_element(&array, i as jint, s)?;
    }
    Ok(array.into_raw())
}

extern "system" fn effects(mut env: JNIEnv, _: JClass, most: jlong, least: jlong) -> jobjectArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let lines: Vec<String> = on_host("Player.getActivePotionEffects", |h| {
            h.effects(player_id(most, least))
        })?
        .iter()
        .map(Effect::line)
        .collect();
        string_array(env, &lines)
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn add_effect(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    id: JString,
    duration: jint,
    amplifier: jint,
    ambient: jboolean,
    particles: jboolean,
    icon: jboolean,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let effect = Effect {
            id: string(env, &id)?,
            duration,
            amplifier,
            ambient: ambient != 0,
            particles: particles != 0,
            icon: icon != 0,
        };
        let done = on_host("Player.addPotionEffect", |h| {
            h.add_effect(player_id(most, least), &effect)
        })?;
        Ok(jbool(done))
    })
}

extern "system" fn remove_effect(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    id: JString,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let id = string(env, &id)?;
        let done = on_host("Player.removePotionEffect", |h| {
            h.remove_effect(player_id(most, least), &id)
        })?;
        Ok(jbool(done))
    })
}

extern "system" fn kick(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    reason: JString,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let reason = string(env, &reason)?;
        Ok(jbool(on_host("Player.kickPlayer", |h| {
            h.kick(player_id(most, least), &reason)
        })?))
    })
}

extern "system" fn command_as(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    line: JString,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let line = string(env, &line)?;
        Ok(jbool(on_host("Player.performCommand", |h| {
            h.command_as(player_id(most, least), &line)
        })?))
    })
}

extern "system" fn client(mut env: JNIEnv, _: JClass, most: jlong, least: jlong) -> jobjectArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let Some(c) = on_host("Player.getAddress", |h| h.client(player_id(most, least)))? else {
            return Ok(std::ptr::null_mut());
        };
        let lines = [
            c.address.ip().to_string(),
            c.address.port().to_string(),
            c.locale,
            c.view_distance.to_string(),
        ];
        string_array(env, &lines)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_array_matches_its_length_and_order() {
        let s = PlayerState {
            health: 7.5,
            game_mode: 2,
            flying: true,
            velocity: [1.0, 2.0, 3.0],
            respawn: Some(Location {
                world: 1,
                x: 4.0,
                y: 5.0,
                z: 6.0,
                yaw: 0.0,
                pitch: 0.0,
            }),
            ..Default::default()
        };
        let a = s.to_array();
        assert_eq!(a.len(), STATE_LEN);
        // PlayerState.java: HEALTH 0, GAME_MODE 9, FLYING 10, VELOCITY 22, RESPAWN 37.
        assert_eq!((a[0], a[9], a[10]), (7.5, 2.0, 1.0));
        assert_eq!(a[22..25], [1.0, 2.0, 3.0]);
        assert_eq!(a[37..41], [1.0, 4.0, 5.0, 6.0]);
        let none = PlayerState::default().to_array();
        assert_eq!(none[37], -1.0);
    }

    #[test]
    fn moved_reads_the_answer_convention() {
        assert_eq!(moved(None), Moved::Unchanged);
        assert_eq!(moved(Some(Vec::new())), Moved::Cancelled);
        let to = moved(Some(vec![2.0, 1.0, 2.0, 3.0, 90.0, 0.0]));
        assert_eq!(
            to,
            Moved::To(Location {
                world: 2,
                x: 1.0,
                y: 2.0,
                z: 3.0,
                yaw: 90.0,
                pitch: 0.0
            })
        );
        // A malformed answer leaves the move as it was.
        assert_eq!(moved(Some(vec![1.0])), Moved::Unchanged);
    }

    #[test]
    fn effect_lines_parse_back_in_java_order() {
        let e = Effect {
            id: "minecraft:speed".into(),
            duration: 200,
            amplifier: 1,
            ambient: false,
            particles: true,
            icon: true,
        };
        assert_eq!(e.line(), "minecraft:speed 200 1 0 1 1");
    }
}
