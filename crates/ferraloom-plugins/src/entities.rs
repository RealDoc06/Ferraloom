//! The entities area: `Entity` and `LivingEntity` objects for every entity
//! but players, their events (`java/bridge/.../EntityEvents.java`,
//! `EntityNative.java`, `Entities.java`).
//!
//! An entity's Java object is a handle, [`EntityRef`]: its id and a
//! generation the host checks, so a handle whose entity is gone answers as
//! removed, never as another entity. Everything else is asked of the host
//! when a plugin calls (docs/ferraloom/plugins.md, "Java objects and server
//! state").
//!
//! Some events fire inside mob code, where the game is not reachable as a
//! whole (a mob's tick in the parallel mob pass, a mob taking damage). Their
//! host is a restricted one: it reads the mob in hand and what the pass
//! holds, queues changes to other entities, and answers
//! [`Reach::Elsewhere`] for what it cannot see, which the bridge throws as
//! `IllegalStateException` rather than answer wrong.
use crate::{
    Location, PlayerId,
    bridge::{self, Entry, obj},
    natives::{self, Fail, guard, halves, jbool, player_id, string, with_host},
};
use jni::{
    JNIEnv,
    objects::{JClass, JIntArray, JObject, JString, JValue},
    signature::{Primitive, ReturnType},
    sys::{JNI_FALSE, jboolean, jdouble, jdoubleArray, jfloat, jint, jlong, jobject, jstring},
};
use std::{ffi::c_void, sync::OnceLock};

/// An entity as the bridge names it. Entity ids only grow during a run
/// (`Game::next_entity`), and the generation is a hash of the entity's type,
/// so a handle never resolves to another entity, even across the game's
/// separate lists (mobs, items, projectiles).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityRef {
    pub id: u32,
    pub generation: u32,
}

impl EntityRef {
    /// The handle of entity `id` of type `kind` (`zombie` or `minecraft:zombie`).
    pub fn new(id: u32, kind: &str) -> Self {
        Self {
            id,
            generation: generation(kind),
        }
    }

    /// Whether this handle names entity `id` of type `kind`.
    pub fn is(self, id: u32, kind: &str) -> bool {
        self.id == id && self.generation == generation(kind)
    }
}

/// FNV-1a of the type's key without its namespace.
fn generation(kind: &str) -> u32 {
    let key = kind.strip_prefix("minecraft:").unwrap_or(kind);
    key.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

/// Who an entity reference points at: a player (the players area's object)
/// or another entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actor {
    Player(PlayerId),
    Entity(EntityRef),
}

/// An actor with its type's key (`zombie`; `player` for players), which the
/// bridge needs to give its object the right Bukkit interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Named {
    pub actor: Actor,
    pub kind: &'static str,
}

impl Named {
    pub fn player(id: PlayerId) -> Self {
        Self {
            actor: Actor::Player(id),
            kind: "player",
        }
    }

    pub fn entity(id: u32, kind: &'static str) -> Self {
        let kind = kind.strip_prefix("minecraft:").unwrap_or(kind);
        Self {
            actor: Actor::Entity(EntityRef::new(id, kind)),
            kind,
        }
    }
}

/// What a host can say of an entity.
#[derive(Clone, Debug, PartialEq)]
pub enum Reach<T> {
    Here(T),
    /// No such entity (removed, or a stale handle).
    Gone,
    /// It may exist, but this host cannot see it (a restricted host).
    Elsewhere,
}

/// [`EntityState::flags`].
pub mod flags {
    pub const ON_GROUND: u32 = 1;
    pub const NAME_VISIBLE: u32 = 1 << 1;
    pub const SILENT: u32 = 1 << 2;
    pub const GLOWING: u32 = 1 << 3;
    pub const INVULNERABLE: u32 = 1 << 4;
    pub const GRAVITY: u32 = 1 << 5;
    /// A living entity: the health fields mean something.
    pub const LIVING: u32 = 1 << 6;
    pub const DEAD: u32 = 1 << 7;
    pub const AI: u32 = 1 << 8;
    pub const PERSISTENT: u32 = 1 << 9;
    pub const TAMED: u32 = 1 << 10;
    pub const BABY: u32 = 1 << 11;
    pub const INVISIBLE: u32 = 1 << 12;
    /// A charged creeper.
    pub const POWERED: u32 = 1 << 13;
}

/// An entity as one read gives it. A restricted host may know less; what it
/// does not know is its type's default.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityState {
    pub at: Location,
    pub velocity: [f64; 3],
    /// [`flags`].
    pub flags: u32,
    pub fire_ticks: i32,
    pub ticks_lived: i32,
    pub fall_distance: f64,
    pub health: f64,
    pub max_health: f64,
    pub no_damage_ticks: i32,
    pub custom_name: Option<String>,
    pub target: Option<Named>,
    /// `LivingEntity.getKiller`: the player who last hurt it, while that counts.
    pub killer: Option<PlayerId>,
    /// `Tameable.getOwner`.
    pub owner: Option<PlayerId>,
    pub passengers: Vec<Named>,
}

impl EntityState {
    /// An entity at `at` the host knows nothing else of: what it does know
    /// it sets after.
    pub fn at(at: Location) -> Self {
        Self {
            at,
            velocity: [0.0; 3],
            flags: flags::GRAVITY,
            fire_ticks: 0,
            ticks_lived: 0,
            fall_distance: 0.0,
            health: 0.0,
            max_health: 0.0,
            no_damage_ticks: 0,
            custom_name: None,
            target: None,
            killer: None,
            owner: None,
            passengers: Vec::new(),
        }
    }
}

/// A change a plugin makes to an entity.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Velocity([f64; 3]),
    Teleport(Location),
    /// `Entity.remove`: gone without dying.
    Remove,
    CustomName(Option<String>),
    NameVisible(bool),
    FireTicks(i32),
    Health(f64),
    Target(Option<Actor>),
    Ai(bool),
    Persistent(bool),
    Silent(bool),
    Glowing(bool),
    Gravity(bool),
    Invulnerable(bool),
    Tamed(bool),
    Baby(bool),
    Powered(bool),
}

/// `CreatureSpawnEvent.SpawnReason`'s names the game spawns with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnReason {
    Natural,
    ChunkGen,
    Spawner,
    SpawnerEgg,
    Breeding,
    Command,
    Custom,
    Patrol,
    Raid,
    Reinforcements,
    Village,
    Default,
}

impl SpawnReason {
    fn name(self) -> &'static str {
        match self {
            Self::Natural => "NATURAL",
            Self::ChunkGen => "CHUNK_GEN",
            Self::Spawner => "SPAWNER",
            Self::SpawnerEgg => "SPAWNER_EGG",
            Self::Breeding => "BREEDING",
            Self::Command => "COMMAND",
            Self::Custom => "CUSTOM",
            Self::Patrol => "PATROL",
            Self::Raid => "RAID",
            Self::Reinforcements => "REINFORCEMENTS",
            Self::Village => "VILLAGE_DEFENSE",
            Self::Default => "DEFAULT",
        }
    }
}

/// What `Entity` and `LivingEntity` read and change of the game.
///
/// Every method has a default that answers [`Reach::Elsewhere`] (thrown as
/// `IllegalStateException`): a host that cannot reach entities implements
/// none of them.
pub trait EntityHost {
    /// One read of an entity.
    fn entity(&mut self, _e: EntityRef) -> Reach<EntityState> {
        Reach::Elsewhere
    }
    /// A change: `Here(true)` once it is made or queued, `Here(false)` for
    /// one this entity cannot take.
    fn change_entity(&mut self, _e: EntityRef, _change: Change) -> Reach<bool> {
        Reach::Elsewhere
    }
    /// The entities of a world (players included), within a box
    /// (`[min x, y, z, max x, y, z]`) when one is given. `Gone` (an empty
    /// list) for a world that is not the level in place; `Elsewhere` where
    /// no world is reachable (a restricted host).
    fn entities(&mut self, _world: usize, _within: Option<[f64; 6]>) -> Reach<Vec<Named>> {
        Reach::Elsewhere
    }
    /// `World.spawnEntity`: `Here(None)` when that type cannot spawn there,
    /// `Elsewhere` when the world is not reachable from here.
    fn spawn(&mut self, _kind: &str, _at: Location) -> Reach<Option<Named>> {
        Reach::Elsewhere
    }
}

// `EntityEvents`' methods, by index.
const DAMAGE: usize = 0;
const DEATH: usize = 1;
const SPAWN: usize = 2;
const TARGET: usize = 3;
const EXPLODE: usize = 4;
const INTERACT: usize = 5;
/// `Entities.of`, which makes an actor's object.
const OF: usize = 6;
const CHANGE_BLOCK: usize = 7;
const COMBUST: usize = 8;

static EVENTS: OnceLock<Entry<9>> = OnceLock::new();

/// At start: `EntityNative`'s methods bound, `EntityEvents`' looked up.
pub(crate) fn load(env: &mut JNIEnv) -> jni::errors::Result<()> {
    const S: &str = "Ljava/lang/String;";
    const O: &str = "Ljava/lang/Object;";
    natives::register(
        env,
        "dev/ferraloom/bukkit/EntityNative",
        &[
            ("state", "(II)[D", state as *mut c_void),
            (
                "customName",
                &format!("(II){S}"),
                custom_name as *mut c_void,
            ),
            ("related", &format!("(III){O}"), related as *mut c_void),
            ("change", &format!("(III[D{S})Z"), change as *mut c_void),
            (
                "setTarget",
                &format!("(IIJJ{S})Z"),
                set_target as *mut c_void,
            ),
            ("list", &format!("(I[D)[{O}"), list as *mut c_void),
            ("spawn", &format!("({S}IDDDFF){O}"), spawn as *mut c_void),
        ],
    )?;
    // Each actor crosses as three values: `(a, b, kind)`, see `actor`.
    let a = format!("JJ{S}");
    let entry = Entry::load(
        env,
        "dev/ferraloom/bukkit/EntityEvents",
        [
            ("damage", format!("({a}{a}{S}{S}DDDD)D")),
            ("death", format!("({a}{a}{S}I)I")),
            ("spawn", format!("({a}{S})Z")),
            ("target", format!("({a}{a}{S})[J")),
            ("explode", format!("({a}IDDDF[I)[I")),
            ("interact", format!("(JJ{a}Z)Z")),
            ("of", format!("({a})Lorg/bukkit/entity/Entity;")),
            ("changeBlock", format!("({a}IIII{S})Z")),
            ("combust", format!("({a}F)F")),
        ],
    )?;
    let _ = EVENTS.set(entry);
    Ok(())
}

/// An actor as three values: a player's UUID halves and `player`, an
/// entity's id and generation and its type, or nothing (`0, 0, null`).
fn actor<'local>(
    env: &mut JNIEnv<'local>,
    who: Option<Named>,
) -> jni::errors::Result<(jlong, jlong, JObject<'local>)> {
    let Some(who) = who else {
        return Ok((0, 0, JObject::null()));
    };
    let (a, b) = match who.actor {
        Actor::Player(id) => halves(id),
        Actor::Entity(e) => (jlong::from(e.id), jlong::from(e.generation)),
    };
    Ok((a, b, env.new_string(who.kind)?.into()))
}

/// The three JNI arguments of an actor.
fn args3(a: &(jlong, jlong, JObject)) -> [jni::sys::jvalue; 3] {
    [
        JValue::Long(a.0).as_jni(),
        JValue::Long(a.1).as_jni(),
        obj(&a.2),
    ]
}

/// An actor's Java object (`Entities.of`), inside a native.
fn object<'local>(env: &mut JNIEnv<'local>, who: Named) -> Result<JObject<'local>, Fail> {
    let events = EVENTS.get().ok_or(Fail::Jni)?;
    let who = actor(env, Some(who))?;
    // SAFETY: `of`'s signature in `load`.
    Ok(unsafe { events.invoke(env, OF, ReturnType::Object, &args3(&who)) }?.l()?)
}

/// A damage about to be dealt, for [`entity_damage`].
pub struct Damage<'a> {
    pub victim: Named,
    /// The entity that caused it (EntityDamageByEntityEvent), if known.
    pub damager: Option<Named>,
    /// `EntityDamageEvent.DamageCause`'s name (`ENTITY_ATTACK`).
    pub cause: &'a str,
    /// The damage type's key (`minecraft:mob_attack`).
    pub damage_type: &'a str,
    /// The damage before armor (the BASE modifier).
    pub base: f64,
    /// The fractions armor and enchantments take, as the ARMOR and MAGIC
    /// modifiers apply them to what is left before them.
    pub armor: f64,
    pub magic: f64,
    /// The damage the game would deal.
    pub dealt: f64,
}

/// `EntityDamageEvent` (or `EntityDamageByEntityEvent`), before the damage
/// is dealt: the damage to deal instead of `dealt`, or none when cancelled.
pub fn entity_damage(host: &mut dyn crate::PluginHost, d: &Damage) -> Option<f64> {
    let Some(events) = EVENTS.get() else {
        return Some(d.dealt);
    };
    bridge::call(host, |env| {
        let victim = actor(env, Some(d.victim))?;
        let damager = actor(env, d.damager)?;
        let (cause, kind) = (env.new_string(d.cause)?, env.new_string(d.damage_type)?);
        let mut args = Vec::with_capacity(12);
        args.extend(args3(&victim));
        args.extend(args3(&damager));
        args.extend([obj(&cause), obj(&kind)]);
        args.extend([d.base, d.armor, d.magic, d.dealt].map(|v| JValue::Double(v).as_jni()));
        let double = ReturnType::Primitive(Primitive::Double);
        // SAFETY: `damage`'s signature in `load`.
        unsafe { events.invoke(env, DAMAGE, double, &args) }?.d()
    })
    .map_or(Some(d.dealt), |dealt| (!dealt.is_nan()).then_some(dealt))
}

/// `EntityDeathEvent`, as the entity dies: the experience to drop.
pub fn entity_death(
    host: &mut dyn crate::PluginHost,
    victim: Named,
    killer: Option<Named>,
    damage_type: &str,
    experience: i32,
) -> i32 {
    let Some(events) = EVENTS.get() else {
        return experience;
    };
    bridge::call(host, |env| {
        let victim = actor(env, Some(victim))?;
        let killer = actor(env, killer)?;
        let kind = env.new_string(damage_type)?;
        let mut args = Vec::with_capacity(8);
        args.extend(args3(&victim));
        args.extend(args3(&killer));
        args.extend([obj(&kind), JValue::Int(experience).as_jni()]);
        let int = ReturnType::Primitive(Primitive::Int);
        // SAFETY: `death`'s signature in `load`.
        unsafe { events.invoke(env, DEATH, int, &args) }?.i()
    })
    .unwrap_or(experience)
}

/// `CreatureSpawnEvent` (an `EntitySpawnEvent`) for an entity about to be
/// added; the host can already read it. True when cancelled: it must not be added.
pub fn creature_spawn(host: &mut dyn crate::PluginHost, who: Named, reason: SpawnReason) -> bool {
    let Some(events) = EVENTS.get() else {
        return false;
    };
    bridge::call(host, |env| {
        let who = actor(env, Some(who))?;
        let reason = env.new_string(reason.name())?;
        let mut args = Vec::with_capacity(4);
        args.extend(args3(&who));
        args.push(obj(&reason));
        let boolean = ReturnType::Primitive(Primitive::Boolean);
        // SAFETY: `spawn`'s signature in `load`.
        unsafe { events.invoke(env, SPAWN, boolean, &args) }?.z()
    })
    .unwrap_or(false)
}

/// What an `EntityTargetEvent` left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Targeted {
    /// As the game chose.
    Kept,
    /// Cancelled: the old target stays.
    Cancelled,
    /// A plugin chose another target (or none).
    Changed(Option<Actor>),
}

/// `EntityTargetEvent` (`EntityTargetLivingEntityEvent` for a target) for a
/// mob whose target changes to `target`; `reason` is `TargetReason`'s name.
pub fn entity_target(
    host: &mut dyn crate::PluginHost,
    mob: Named,
    target: Option<Named>,
    reason: &str,
) -> Targeted {
    let Some(events) = EVENTS.get() else {
        return Targeted::Kept;
    };
    bridge::call(host, |env| {
        let mob = actor(env, Some(mob))?;
        let target = actor(env, target)?;
        let reason = env.new_string(reason)?;
        let mut args = Vec::with_capacity(7);
        args.extend(args3(&mob));
        args.extend(args3(&target));
        args.push(obj(&reason));
        // SAFETY: `target`'s signature in `load`.
        let out = unsafe { events.invoke(env, TARGET, ReturnType::Array, &args) }?.l()?;
        let mut v = [0; 3];
        env.get_long_array_region(jni::objects::JLongArray::from(out), 0, &mut v)?;
        Ok(match v[0] {
            1 => Targeted::Cancelled,
            2 => Targeted::Changed(None),
            3 => Targeted::Changed(Some(Actor::Player(player_id(v[1], v[2])))),
            4 => Targeted::Changed(Some(Actor::Entity(EntityRef {
                id: v[1] as u32,
                generation: v[2] as u32,
            }))),
            _ => Targeted::Kept,
        })
    })
    .unwrap_or(Targeted::Kept)
}

/// `EntityExplodeEvent` for an explosion an entity caused (`by`, none for
/// a block's), before blocks break: the blocks that may break, none when
/// cancelled (no block breaks).
pub fn entity_explode(
    host: &mut dyn crate::PluginHost,
    by: Option<Named>,
    at: Location,
    yield_: f32,
    blocks: &[[i32; 3]],
) -> Option<Vec<[i32; 3]>> {
    let Some(events) = EVENTS.get() else {
        return Some(blocks.to_vec());
    };
    bridge::call(host, |env| {
        let by = actor(env, by)?;
        let flat: Vec<i32> = blocks.iter().flatten().copied().collect();
        let array = env.new_int_array(flat.len() as jint)?;
        env.set_int_array_region(&array, 0, &flat)?;
        let mut args = Vec::with_capacity(9);
        args.extend(args3(&by));
        args.push(JValue::Int(at.world as jint).as_jni());
        args.extend([at.x, at.y, at.z].map(|v| JValue::Double(v).as_jni()));
        args.extend([JValue::Float(yield_).as_jni(), obj(&array)]);
        // SAFETY: `explode`'s signature in `load`.
        let out = unsafe { events.invoke(env, EXPLODE, ReturnType::Array, &args) }?.l()?;
        if out.is_null() {
            return Ok(None);
        }
        let out = JIntArray::from(out);
        let mut kept = vec![0; env.get_array_length(&out)? as usize];
        env.get_int_array_region(&out, 0, &mut kept)?;
        Ok(Some(
            kept.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
        ))
    })
    .unwrap_or_else(|| Some(blocks.to_vec()))
}

/// `PlayerInteractEntityEvent` for a player using `entity`: true when
/// cancelled, and the interaction does nothing.
pub fn interact_entity(
    host: &mut dyn crate::PluginHost,
    player: PlayerId,
    entity: Named,
    off_hand: bool,
) -> bool {
    let Some(events) = EVENTS.get() else {
        return false;
    };
    let (most, least) = halves(player);
    bridge::call(host, |env| {
        let entity = actor(env, Some(entity))?;
        let mut args = vec![JValue::Long(most).as_jni(), JValue::Long(least).as_jni()];
        args.extend(args3(&entity));
        args.push(JValue::Bool(off_hand.into()).as_jni());
        let boolean = ReturnType::Primitive(Primitive::Boolean);
        // SAFETY: `interact`'s signature in `load`.
        unsafe { events.invoke(env, INTERACT, boolean, &args) }?.z()
    })
    .unwrap_or(false)
}

/// `EntityChangeBlockEvent` for `entity` about to change the block at `pos`
/// in `world` to state `to` (`minecraft:air`): true when cancelled.
pub fn entity_change_block(
    host: &mut dyn crate::PluginHost,
    entity: Named,
    world: usize,
    pos: [i32; 3],
    to: &str,
) -> bool {
    let Some(events) = EVENTS.get() else {
        return false;
    };
    bridge::call(host, |env| {
        let entity = actor(env, Some(entity))?;
        let to = env.new_string(to)?;
        let mut args = Vec::with_capacity(8);
        args.extend(args3(&entity));
        args.push(JValue::Int(world as jint).as_jni());
        args.extend(pos.map(|v| JValue::Int(v).as_jni()));
        args.push(obj(&to));
        let boolean = ReturnType::Primitive(Primitive::Boolean);
        // SAFETY: `changeBlock`'s signature in `load`.
        unsafe { events.invoke(env, CHANGE_BLOCK, boolean, &args) }?.z()
    })
    .unwrap_or(false)
}

/// `EntityCombustEvent` for `entity` catching fire for `seconds`: the
/// seconds it burns, none when cancelled.
pub fn entity_combust(
    host: &mut dyn crate::PluginHost,
    entity: Named,
    seconds: f32,
) -> Option<f32> {
    let Some(events) = EVENTS.get() else {
        return Some(seconds);
    };
    bridge::call(host, |env| {
        let entity = actor(env, Some(entity))?;
        let mut args = Vec::with_capacity(4);
        args.extend(args3(&entity));
        args.push(JValue::Float(seconds).as_jni());
        let float = ReturnType::Primitive(Primitive::Float);
        // SAFETY: `combust`'s signature in `load`.
        unsafe { events.invoke(env, COMBUST, float, &args) }?.f()
    })
    .map_or(Some(seconds), |s| (!s.is_nan()).then_some(s))
}

/// `EntityNative.state`'s layout, mirrored in `EntityNative.java`.
pub(crate) const STATE_LEN: usize = 16;

fn state_values(s: &EntityState) -> [f64; STATE_LEN] {
    [
        s.at.world as f64,
        s.at.x,
        s.at.y,
        s.at.z,
        f64::from(s.at.yaw),
        f64::from(s.at.pitch),
        s.velocity[0],
        s.velocity[1],
        s.velocity[2],
        f64::from(s.flags),
        f64::from(s.fire_ticks),
        f64::from(s.ticks_lived),
        s.fall_distance,
        s.health,
        s.max_health,
        f64::from(s.no_damage_ticks),
    ]
}

/// The lent host's read of `e`; `Elsewhere` throws, naming `what`.
fn read<R>(
    env: &mut JNIEnv,
    e: EntityRef,
    what: &'static str,
    f: impl FnOnce(EntityState) -> R,
) -> Result<Option<R>, Fail> {
    match with_host(|h| h.entity(e)).ok_or(Fail::NoHost(what))? {
        Reach::Here(s) => Ok(Some(f(s))),
        Reach::Gone => Ok(None),
        Reach::Elsewhere => Err(unreachable_here(env, what)),
    }
}

/// `IllegalStateException` for what a restricted host cannot see.
fn unreachable_here(env: &mut JNIEnv, what: &str) -> Fail {
    let _ = env.throw_new(
        "java/lang/IllegalStateException",
        format!(
            "{what} is not reachable inside this event on Ferraloom yet \
             (only the event's own entities are; schedule a sync task)"
        ),
    );
    Fail::Jni
}

fn handle(id: jint, generation: jint) -> EntityRef {
    EntityRef {
        id: id as u32,
        generation: generation as u32,
    }
}

extern "system" fn state(mut env: JNIEnv, _: JClass, id: jint, generation: jint) -> jdoubleArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let Some(values) = read(env, handle(id, generation), "Entity.getLocation", |s| {
            state_values(&s)
        })?
        else {
            return Ok(std::ptr::null_mut());
        };
        let array = env.new_double_array(STATE_LEN as jint)?;
        env.set_double_array_region(&array, 0, &values)?;
        Ok(array.into_raw())
    })
}

extern "system" fn custom_name(mut env: JNIEnv, _: JClass, id: jint, generation: jint) -> jstring {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let name = read(env, handle(id, generation), "Entity.getCustomName", |s| {
            s.custom_name
        })?;
        match name.flatten() {
            Some(n) => Ok(env.new_string(n)?.into_raw()),
            None => Ok(std::ptr::null_mut()),
        }
    })
}

/// `EntityNative.related`'s `which`.
const TARGET_OF: jint = 0;
const KILLER_OF: jint = 1;
const OWNER_OF: jint = 2;
const PASSENGERS_OF: jint = 3;

/// An entity's target, killer or owner (an Entity or null), or its passengers (an Entity[]).
extern "system" fn related(
    mut env: JNIEnv,
    _: JClass,
    id: jint,
    generation: jint,
    which: jint,
) -> jobject {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let what = match which {
            TARGET_OF => "Mob.getTarget",
            KILLER_OF => "LivingEntity.getKiller",
            OWNER_OF => "Tameable.getOwner",
            _ => "Entity.getPassengers",
        };
        let Some(s) = read(env, handle(id, generation), what, |s| s)? else {
            return Ok(std::ptr::null_mut());
        };
        let one = match which {
            TARGET_OF => s.target,
            KILLER_OF => s.killer.map(Named::player),
            OWNER_OF => s.owner.map(Named::player),
            PASSENGERS_OF => {
                let array = env.new_object_array(
                    s.passengers.len() as jint,
                    "org/bukkit/entity/Entity",
                    JObject::null(),
                )?;
                for (i, p) in s.passengers.iter().enumerate() {
                    let o = object(env, *p)?;
                    env.set_object_array_element(&array, i as jint, o)?;
                }
                return Ok(array.into_raw());
            }
            _ => None,
        };
        match one {
            Some(who) => Ok(object(env, who)?.into_raw()),
            None => Ok(std::ptr::null_mut()),
        }
    })
}

/// `EntityNative.change`'s `what`, mirrored in `EntityNative.java`.
fn decode(what: jint, v: &[f64], text: Option<String>) -> Option<Change> {
    let at = |i: usize| v.get(i).copied().unwrap_or(0.0);
    let on = at(0) != 0.0;
    Some(match what {
        0 => Change::Velocity([at(0), at(1), at(2)]),
        1 => Change::Teleport(Location {
            world: usize::try_from(at(0) as i64).ok()?,
            x: at(1),
            y: at(2),
            z: at(3),
            yaw: at(4) as f32,
            pitch: at(5) as f32,
        }),
        2 => Change::Remove,
        3 => Change::CustomName(text),
        4 => Change::NameVisible(on),
        5 => Change::FireTicks(at(0) as i32),
        6 => Change::Health(at(0)),
        7 => Change::Ai(on),
        8 => Change::Persistent(on),
        9 => Change::Silent(on),
        10 => Change::Glowing(on),
        11 => Change::Gravity(on),
        12 => Change::Invulnerable(on),
        13 => Change::Tamed(on),
        14 => Change::Baby(on),
        15 => Change::Powered(on),
        _ => return None,
    })
}

fn apply(env: &mut JNIEnv, e: EntityRef, change: Change) -> Result<jboolean, Fail> {
    match with_host(|h| h.change_entity(e, change)).ok_or(Fail::NoHost("Entity's setters"))? {
        Reach::Here(done) => Ok(jbool(done)),
        Reach::Gone => Ok(JNI_FALSE),
        Reach::Elsewhere => Err(unreachable_here(env, "Changing another entity")),
    }
}

extern "system" fn change(
    mut env: JNIEnv,
    _: JClass,
    id: jint,
    generation: jint,
    what: jint,
    values: jni::objects::JDoubleArray,
    text: JString,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let mut v = [0.0; 6];
        if !values.is_null() {
            let n = (env.get_array_length(&values)? as usize).min(v.len());
            env.get_double_array_region(&values, 0, &mut v[..n])?;
        }
        let text = if text.is_null() {
            None
        } else {
            Some(string(env, &text)?)
        };
        let Some(c) = decode(what, &v, text) else {
            return Ok(JNI_FALSE);
        };
        apply(env, handle(id, generation), c)
    })
}

/// `Mob.setTarget`: `kind` null for none, `player` for a player (`a`, `b`
/// their UUID's halves), else an entity's id and generation.
extern "system" fn set_target(
    mut env: JNIEnv,
    _: JClass,
    id: jint,
    generation: jint,
    a: jlong,
    b: jlong,
    kind: JString,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let target = if kind.is_null() {
            None
        } else if string(env, &kind)? == "player" {
            Some(Actor::Player(player_id(a, b)))
        } else {
            Some(Actor::Entity(EntityRef {
                id: a as u32,
                generation: b as u32,
            }))
        };
        apply(env, handle(id, generation), Change::Target(target))
    })
}

/// A world's entities (`World.getEntities`), within a box when one is given
/// (six doubles); an empty array for a world not reachable from here.
extern "system" fn list(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    within: jni::objects::JDoubleArray,
) -> jobject {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let within = if within.is_null() {
            None
        } else {
            let mut b = [0.0; 6];
            env.get_double_array_region(&within, 0, &mut b)?;
            Some(b)
        };
        let world = usize::try_from(world).unwrap_or(usize::MAX);
        let found = match with_host(|h| h.entities(world, within))
            .ok_or(Fail::NoHost("World.getEntities"))?
        {
            Reach::Here(found) => found,
            Reach::Gone => Vec::new(),
            Reach::Elsewhere => return Err(unreachable_here(env, "World.getEntities")),
        };
        let array =
            env.new_object_array(found.len() as jint, "java/lang/Object", JObject::null())?;
        for (i, who) in found.into_iter().enumerate() {
            let o = object(env, who)?;
            env.set_object_array_element(&array, i as jint, &o)?;
            env.delete_local_ref(o)?;
        }
        Ok(array.into_raw())
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn spawn(
    mut env: JNIEnv,
    _: JClass,
    kind: JString,
    world: jint,
    x: jdouble,
    y: jdouble,
    z: jdouble,
    yaw: jfloat,
    pitch: jfloat,
) -> jobject {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let kind = string(env, &kind)?;
        let at = Location {
            world: usize::try_from(world).unwrap_or(usize::MAX),
            x,
            y,
            z,
            yaw,
            pitch,
        };
        match with_host(|h| h.spawn(&kind, at)).ok_or(Fail::NoHost("World.spawnEntity"))? {
            Reach::Here(Some(who)) => Ok(object(env, who)?.into_raw()),
            Reach::Here(None) | Reach::Gone => Ok(std::ptr::null_mut()),
            Reach::Elsewhere => {
                let _ = env.throw_new(
                    "java/lang/IllegalStateException",
                    "Ferraloom spawns entities only in the world being ticked now \
                     (the overworld between ticks) yet",
                );
                Err(Fail::Jni)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_names_its_entity_and_type_only() {
        let zombie = EntityRef::new(7, "minecraft:zombie");
        assert_eq!(zombie, EntityRef::new(7, "zombie"));
        assert!(zombie.is(7, "zombie"));
        assert!(!zombie.is(7, "drowned"));
        assert!(!zombie.is(8, "zombie"));
        assert_eq!(Named::entity(7, "minecraft:zombie").kind, "zombie");
    }

    #[test]
    fn changes_decode_as_entity_native_encodes_them() {
        let v = [1.0, 2.0, 3.0, 4.0, 90.0, 10.0];
        assert_eq!(decode(0, &v, None), Some(Change::Velocity([1.0, 2.0, 3.0])));
        assert_eq!(
            decode(1, &v, None),
            Some(Change::Teleport(Location {
                world: 1,
                x: 2.0,
                y: 3.0,
                z: 4.0,
                yaw: 90.0,
                pitch: 10.0
            }))
        );
        assert_eq!(
            decode(3, &v, Some("Bob".into())),
            Some(Change::CustomName(Some("Bob".into())))
        );
        assert_eq!(decode(7, &[0.0], None), Some(Change::Ai(false)));
        assert_eq!(decode(99, &v, None), None);
        let mut teleport = v;
        teleport[0] = -1.0;
        assert_eq!(decode(1, &teleport, None), None);
    }
}
