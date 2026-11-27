//! The worlds area: `World`, `Chunk` and `WorldBorder` objects and the world
//! events (`java/bridge/.../WorldNative.java`, `WorldEvents.java`).
//!
//! `World`'s many numeric reads and writes cross JNI as two natives,
//! `query` and `change`, with a code naming the value ([`Query`],
//! [`Change`]); the host sees them as enums.
use crate::{
    bridge::{self, Entry},
    natives::{self, Fail, guard, jbool, string, with_host},
};
use jni::{
    JNIEnv,
    objects::{JClass, JString, JValue},
    signature::{Primitive, ReturnType},
    sys::{JNI_FALSE, jboolean, jdoubleArray, jfloat, jint, jintArray, jlong, jstring},
};
use std::{ffi::c_void, sync::OnceLock};

/// A numeric reading of a world, by `WorldNative.query`'s code (same
/// values in `WorldNative.java`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    /// `getGameTime`.
    GameTime,
    /// `getFullTime` (the day time, whole).
    FullTime,
    /// `getTime` (the time within the day).
    Time,
    /// `hasStorm`, `isThundering` (0 or 1).
    Storm,
    Thundering,
    /// `getWeatherDuration`, `getThunderDuration`, `getClearWeatherDuration`.
    RainTime,
    ThunderTime,
    ClearTime,
    /// The spawn position's x, y and z.
    SpawnX,
    SpawnY,
    SpawnZ,
    /// `getSeaLevel`.
    SeaLevel,
    /// `getHighestBlockYAt(x, z, HeightMap)`: the heightmap is `HeightMap`'s ordinal.
    Highest {
        x: i32,
        z: i32,
        heightmap: i32,
    },
    /// `isChunkLoaded`, `isChunkGenerated` (0 or 1).
    ChunkLoaded {
        x: i32,
        z: i32,
    },
    ChunkGenerated {
        x: i32,
        z: i32,
    },
    /// `getDifficulty`'s ordinal.
    Difficulty,
    /// `getSeed`.
    Seed,
}

impl Query {
    fn decode(what: jint, [a, b, c]: [jint; 3]) -> Option<Self> {
        Some(match what {
            0 => Self::FullTime,
            1 => Self::Time,
            2 => Self::Storm,
            3 => Self::Thundering,
            4 => Self::RainTime,
            5 => Self::ThunderTime,
            6 => Self::ClearTime,
            7 => Self::SpawnX,
            8 => Self::SpawnY,
            9 => Self::SpawnZ,
            10 => Self::SeaLevel,
            11 => Self::Highest {
                x: a,
                z: b,
                heightmap: c,
            },
            12 => Self::ChunkLoaded { x: a, z: b },
            13 => Self::ChunkGenerated { x: a, z: b },
            14 => Self::Difficulty,
            15 => Self::Seed,
            16 => Self::GameTime,
            _ => return None,
        })
    }
}

/// A change to a world, by `WorldNative.change`'s code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    /// `setFullTime`, `setTime`.
    FullTime(i64),
    Time(i64),
    /// `setStorm`, `setThundering` (fire `WeatherChangeEvent`/`ThunderChangeEvent` first).
    Storm(bool),
    Thundering(bool),
    /// `setWeatherDuration`, `setThunderDuration`, `setClearWeatherDuration`.
    RainTime(i32),
    ThunderTime(i32),
    ClearTime(i32),
    /// `setSpawnLocation`.
    Spawn {
        pos: [i32; 3],
        angle: f32,
    },
    /// `setDifficulty` by ordinal.
    Difficulty(i32),
    /// `loadChunk(x, z, generate)`: the chunk is asked for (loaded by the
    /// next ticks, not during the call).
    LoadChunk {
        x: i32,
        z: i32,
        generate: bool,
    },
    /// `unloadChunkRequest`.
    UnloadChunk {
        x: i32,
        z: i32,
    },
    /// A plugin's chunk ticket (`addPluginChunkTicket`, `removePluginChunkTicket`):
    /// the chunk stays loaded while any plugin holds one.
    Ticket {
        x: i32,
        z: i32,
        add: bool,
    },
    /// `WorldBorder.setCenter`, `setSize(size, seconds)`, `setDamageBuffer`,
    /// `setDamageAmount`, `setWarningDistance`, `setWarningTime`.
    BorderCenter([f64; 2]),
    BorderSize {
        size: f64,
        ticks: i64,
    },
    BorderDamageBuffer(f64),
    BorderDamageAmount(f64),
    BorderWarningDistance(i32),
    BorderWarningTime(i32),
    /// `World.save`.
    Save,
}

impl Change {
    fn decode(what: jint, [a, b, c, d]: [jlong; 4]) -> Option<Self> {
        let f = f64::from_bits;
        let i = |v: jlong| v as i32;
        Some(match what {
            0 => Self::FullTime(a),
            1 => Self::Time(a),
            2 => Self::Storm(a != 0),
            3 => Self::Thundering(a != 0),
            4 => Self::RainTime(i(a)),
            5 => Self::ThunderTime(i(a)),
            6 => Self::ClearTime(i(a)),
            7 => Self::Spawn {
                pos: [i(a), i(b), i(c)],
                angle: f32::from_bits(d as u32),
            },
            8 => Self::Difficulty(i(a)),
            9 => Self::LoadChunk {
                x: i(a),
                z: i(b),
                generate: c != 0,
            },
            10 => Self::UnloadChunk { x: i(a), z: i(b) },
            11 => Self::Ticket {
                x: i(a),
                z: i(b),
                add: c != 0,
            },
            12 => Self::BorderCenter([f(a as u64), f(b as u64)]),
            13 => Self::BorderSize {
                size: f(a as u64),
                ticks: b,
            },
            14 => Self::BorderDamageBuffer(f(a as u64)),
            15 => Self::BorderDamageAmount(f(a as u64)),
            16 => Self::BorderWarningDistance(i(a)),
            17 => Self::BorderWarningTime(i(a)),
            18 => Self::Save,
            _ => return None,
        })
    }
}

/// A world's border, as `WorldBorder` reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Border {
    pub center: [f64; 2],
    pub size: f64,
    pub damage_buffer: f64,
    pub damage_amount: f64,
    pub warning_distance: i32,
    pub warning_time: i32,
}

/// A sound for `World.playSound`.
pub struct Sound<'a> {
    pub at: [f64; 3],
    /// `minecraft:entity.pig.ambient`.
    pub name: &'a str,
    /// `SoundCategory`'s ordinal, which is vanilla's `SoundSource`'s.
    pub category: i32,
    pub volume: f32,
    pub pitch: f32,
    pub seed: i64,
}

/// What `World`, `Chunk` and `WorldBorder` read and change of the game.
///
/// Every method but [`WorldHost::world_time`] has a default that answers
/// "cannot" (none, false, nothing): a restricted host implements what it
/// can reach.
pub trait WorldHost {
    /// `World.getFullTime` (`full`: the day time) or `getTime` (the day time
    /// within its day, 0 to 23999).
    fn world_time(&mut self, world: usize, full: bool) -> i64;
    /// A numeric reading; none when the world cannot answer it.
    fn query(&mut self, _world: usize, _query: Query) -> Option<i64> {
        None
    }
    /// A change; false when it was refused.
    fn change(&mut self, _world: usize, _change: Change) -> bool {
        false
    }
    /// A game rule's value (`true`, `3`), or its `default`; none for no such rule.
    fn game_rule(&mut self, _name: &str, _default: bool) -> Option<String> {
        None
    }
    /// Sets a game rule from its text; false for no such rule or a bad value.
    fn set_game_rule(&mut self, _name: &str, _value: &str) -> bool {
        false
    }
    /// Every game rule's name.
    fn game_rules(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// The loaded chunks' positions.
    fn loaded_chunks(&mut self, _world: usize) -> Vec<[i32; 2]> {
        Vec::new()
    }
    /// The world's border.
    fn border(&mut self, _world: usize) -> Border {
        Border::default()
    }
    /// `World.playSound` at a position, to the players who hear it.
    fn play_sound(&mut self, _world: usize, _sound: &Sound) {}
    /// `World.playEffect`: a level event (`LevelEvent` id) at a position.
    fn level_event(&mut self, _world: usize, _pos: [i32; 3], _id: i32, _data: i32) {}
    /// `World.createExplosion`: false when nothing exploded.
    fn explode(
        &mut self,
        _world: usize,
        _at: [f64; 3],
        _power: f32,
        _fire: bool,
        _breaks: bool,
    ) -> bool {
        false
    }
    /// `World.strikeLightning` (`effect`: `strikeLightningEffect`, no damage or fire).
    fn strike_lightning(&mut self, _world: usize, _at: [f64; 3], _effect: bool) -> bool {
        false
    }
}

const CHUNK: usize = 0;
const WEATHER: usize = 1;
const SAVE: usize = 2;

static EVENTS: OnceLock<Entry<3>> = OnceLock::new();

/// At start: `WorldNative`'s methods bound, `WorldEvents`' looked up.
pub(crate) fn load(env: &mut JNIEnv) -> jni::errors::Result<()> {
    const S: &str = "Ljava/lang/String;";
    natives::register(
        env,
        "dev/ferraloom/bukkit/WorldNative",
        &[
            ("worldTime", "(IZ)J", world_time as *mut c_void),
            ("query", "(IIIII)J", query as *mut c_void),
            ("change", "(IIJJJJ)Z", change as *mut c_void),
            ("gameRule", &format!("({S}Z){S}"), game_rule as *mut c_void),
            (
                "setGameRule",
                &format!("({S}{S})Z"),
                set_game_rule as *mut c_void,
            ),
            ("gameRules", &format!("()[{S}"), game_rules as *mut c_void),
            ("loadedChunks", "(I)[I", loaded_chunks as *mut c_void),
            ("border", "(I)[D", border as *mut c_void),
            (
                "playSound",
                &format!("(IDDD{S}IFFJ)V"),
                play_sound as *mut c_void,
            ),
            ("levelEvent", "(IIIIII)V", level_event as *mut c_void),
            ("explode", "(IDDDFZZ)Z", explode as *mut c_void),
            (
                "strikeLightning",
                "(IDDDZ)Z",
                strike_lightning as *mut c_void,
            ),
        ],
    )?;
    let entry = Entry::load(
        env,
        "dev/ferraloom/bukkit/WorldEvents",
        [
            ("chunk", "(IIIZZ)V".to_owned()),
            ("weather", "(IZZ)Z".to_owned()),
            ("save", "(I)V".to_owned()),
        ],
    )?;
    let _ = EVENTS.set(entry);
    Ok(())
}

/// `ChunkLoadEvent` (`loaded`, with `new_chunk` for a chunk just
/// generated) or `ChunkUnloadEvent`.
pub fn chunk_event(
    host: &mut dyn crate::PluginHost,
    world: usize,
    [x, z]: [i32; 2],
    loaded: bool,
    new_chunk: bool,
) {
    let Some(events) = EVENTS.get() else { return };
    bridge::call(host, |env| {
        let args = [
            JValue::Int(world as i32).as_jni(),
            JValue::Int(x).as_jni(),
            JValue::Int(z).as_jni(),
            JValue::Bool(loaded.into()).as_jni(),
            JValue::Bool(new_chunk.into()).as_jni(),
        ];
        let void = ReturnType::Primitive(Primitive::Void);
        // SAFETY: `chunk`'s signature in `load`.
        unsafe { events.invoke(env, CHUNK, void, &args) }.map(drop)
    });
}

/// `WeatherChangeEvent` (`thunder` false) or `ThunderChangeEvent` for the
/// weather turning to `to`: true when cancelled.
pub fn weather_event(
    host: &mut dyn crate::PluginHost,
    world: usize,
    thunder: bool,
    to: bool,
) -> bool {
    let Some(events) = EVENTS.get() else {
        return false;
    };
    bridge::call(host, |env| {
        let args = [
            JValue::Int(world as i32).as_jni(),
            JValue::Bool(thunder.into()).as_jni(),
            JValue::Bool(to.into()).as_jni(),
        ];
        let boolean = ReturnType::Primitive(Primitive::Boolean);
        // SAFETY: `weather`'s signature in `load`.
        unsafe { events.invoke(env, WEATHER, boolean, &args) }?.z()
    })
    .unwrap_or(false)
}

/// `WorldSaveEvent`, before the world saves.
pub fn save_event(host: &mut dyn crate::PluginHost, world: usize) {
    let Some(events) = EVENTS.get() else { return };
    bridge::call(host, |env| {
        let args = [JValue::Int(world as i32).as_jni()];
        let void = ReturnType::Primitive(Primitive::Void);
        // SAFETY: `save`'s signature in `load`.
        unsafe { events.invoke(env, SAVE, void, &args) }.map(drop)
    });
}

fn index(world: jint) -> usize {
    usize::try_from(world).unwrap_or(usize::MAX)
}

/// `WorldNative.query`'s answer for "none".
const NONE: jlong = jlong::MIN;

extern "system" fn world_time(mut env: JNIEnv, _: JClass, world: jint, full: jboolean) -> jlong {
    guard(&mut env, 0, |_| {
        with_host(|h| h.world_time(index(world), full != 0)).ok_or(Fail::NoHost("World.getTime"))
    })
}

extern "system" fn query(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    what: jint,
    a: jint,
    b: jint,
    c: jint,
) -> jlong {
    guard(&mut env, NONE, |_| {
        let Some(q) = Query::decode(what, [a, b, c]) else {
            return Ok(NONE);
        };
        let v = with_host(|h| h.query(index(world), q)).ok_or(Fail::NoHost("World"))?;
        Ok(v.unwrap_or(NONE))
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn change(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    what: jint,
    a: jlong,
    b: jlong,
    c: jlong,
    d: jlong,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        let Some(change) = Change::decode(what, [a, b, c, d]) else {
            return Ok(JNI_FALSE);
        };
        let done = with_host(|h| h.change(index(world), change)).ok_or(Fail::NoHost("World"))?;
        Ok(jbool(done))
    })
}

extern "system" fn game_rule(
    mut env: JNIEnv,
    _: JClass,
    name: JString,
    default: jboolean,
) -> jstring {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let name = string(env, &name)?;
        let value = with_host(|h| h.game_rule(&name, default != 0))
            .ok_or(Fail::NoHost("World.getGameRuleValue"))?;
        match value {
            Some(v) => Ok(env.new_string(v)?.into_raw()),
            None => Ok(std::ptr::null_mut()),
        }
    })
}

extern "system" fn set_game_rule(
    mut env: JNIEnv,
    _: JClass,
    name: JString,
    value: JString,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let (name, value) = (string(env, &name)?, string(env, &value)?);
        let set = with_host(|h| h.set_game_rule(&name, &value))
            .ok_or(Fail::NoHost("World.setGameRule"))?;
        Ok(jbool(set))
    })
}

extern "system" fn game_rules(mut env: JNIEnv, _: JClass) -> jni::sys::jobjectArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let names = with_host(|h| h.game_rules()).ok_or(Fail::NoHost("World.getGameRules"))?;
        let array = env.new_object_array(
            names.len() as i32,
            "java/lang/String",
            jni::objects::JObject::null(),
        )?;
        for (i, n) in names.iter().enumerate() {
            let s = env.new_string(n)?;
            env.set_object_array_element(&array, i as i32, s)?;
        }
        Ok(array.into_raw())
    })
}

extern "system" fn loaded_chunks(mut env: JNIEnv, _: JClass, world: jint) -> jintArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let chunks = with_host(|h| h.loaded_chunks(index(world)))
            .ok_or(Fail::NoHost("World.getLoadedChunks"))?;
        let flat: Vec<i32> = chunks.iter().flatten().copied().collect();
        let array = env.new_int_array(flat.len() as i32)?;
        env.set_int_array_region(&array, 0, &flat)?;
        Ok(array.into_raw())
    })
}

extern "system" fn border(mut env: JNIEnv, _: JClass, world: jint) -> jdoubleArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let b = with_host(|h| h.border(index(world))).ok_or(Fail::NoHost("WorldBorder"))?;
        let values = [
            b.center[0],
            b.center[1],
            b.size,
            b.damage_buffer,
            b.damage_amount,
            f64::from(b.warning_distance),
            f64::from(b.warning_time),
        ];
        let array = env.new_double_array(values.len() as i32)?;
        env.set_double_array_region(&array, 0, &values)?;
        Ok(array.into_raw())
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn play_sound(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    x: f64,
    y: f64,
    z: f64,
    name: JString,
    category: jint,
    volume: jfloat,
    pitch: jfloat,
    seed: jlong,
) {
    guard(&mut env, (), |env| {
        let name = string(env, &name)?;
        let sound = Sound {
            at: [x, y, z],
            name: &name,
            category,
            volume,
            pitch,
            seed,
        };
        with_host(|h| h.play_sound(index(world), &sound)).ok_or(Fail::NoHost("World.playSound"))
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn level_event(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    x: jint,
    y: jint,
    z: jint,
    id: jint,
    data: jint,
) {
    guard(&mut env, (), |_| {
        with_host(|h| h.level_event(index(world), [x, y, z], id, data))
            .ok_or(Fail::NoHost("World.playEffect"))
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn explode(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    x: f64,
    y: f64,
    z: f64,
    power: jfloat,
    fire: jboolean,
    breaks: jboolean,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        let done = with_host(|h| h.explode(index(world), [x, y, z], power, fire != 0, breaks != 0))
            .ok_or(Fail::NoHost("World.createExplosion"))?;
        Ok(jbool(done))
    })
}

extern "system" fn strike_lightning(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    x: f64,
    y: f64,
    z: f64,
    effect: jboolean,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        let done = with_host(|h| h.strike_lightning(index(world), [x, y, z], effect != 0))
            .ok_or(Fail::NoHost("World.strikeLightning"))?;
        Ok(jbool(done))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_decode_to_their_values() {
        assert_eq!(
            Query::decode(11, [3, -4, 1]),
            Some(Query::Highest {
                x: 3,
                z: -4,
                heightmap: 1
            })
        );
        assert_eq!(Query::decode(99, [0; 3]), None);
        let size = Change::decode(13, [64.5_f64.to_bits() as jlong, 20, 0, 0]);
        assert_eq!(
            size,
            Some(Change::BorderSize {
                size: 64.5,
                ticks: 20
            })
        );
        let spawn = Change::decode(7, [1, -2, 3, i64::from(90.0_f32.to_bits())]);
        assert_eq!(
            spawn,
            Some(Change::Spawn {
                pos: [1, -2, 3],
                angle: 90.0
            })
        );
        assert_eq!(Change::decode(-1, [0; 4]), None);
    }
}
