//! The worlds and blocks area: `World` and `Block` objects, block events
//! (`java/bridge/.../BlockEvents.java`, `WorldNative.java`).
use crate::{
    PlayerId,
    bridge::{self, Entry, obj},
    natives::{self, Fail, guard, with_host},
};
use jni::{
    JNIEnv,
    objects::{JClass, JValue},
    signature::{Primitive, ReturnType},
    sys::{jboolean, jint, jlong, jstring},
};
use std::{ffi::c_void, sync::OnceLock};

/// What `World` and `Block` read and change of the game. Worlds are
/// indices into [`crate::Config::worlds`].
pub trait WorldHost {
    /// The world's game time (`full`) or time of day.
    fn world_time(&mut self, world: usize, full: bool) -> i64;
    /// The block state at a position (`minecraft:oak_log[axis=y]`); none where no chunk is loaded.
    fn block_at(&mut self, world: usize, pos: [i32; 3]) -> Option<String>;
}

const BLOCK_BREAK: usize = 0;

static EVENTS: OnceLock<Entry<1>> = OnceLock::new();

/// At start: `WorldNative`'s methods bound, `BlockEvents`' looked up.
pub(crate) fn load(env: &mut JNIEnv) -> jni::errors::Result<()> {
    natives::register(
        env,
        "dev/ferraloom/bukkit/WorldNative",
        &[
            ("worldTime", "(IZ)J", world_time as *mut c_void),
            (
                "blockAt",
                "(IIII)Ljava/lang/String;",
                block_at as *mut c_void,
            ),
        ],
    )?;
    let entry = Entry::load(
        env,
        "dev/ferraloom/bukkit/BlockEvents",
        [("blockBreak", "(JJIIIILjava/lang/String;)Z".to_owned())],
    )?;
    let _ = EVENTS.set(entry);
    Ok(())
}

/// `BlockBreakEvent` for a player about to break `state` at `pos`: true
/// when a plugin cancelled it, and nothing may break.
pub fn block_break(
    host: &mut dyn crate::PluginHost,
    id: PlayerId,
    world: usize,
    pos: [i32; 3],
    state: &str,
) -> bool {
    let (most, least) = natives::halves(id);
    let Some(events) = EVENTS.get() else {
        return false;
    };
    bridge::call(host, |env| {
        let state = env.new_string(state)?;
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            JValue::Int(world as i32).as_jni(),
            JValue::Int(pos[0]).as_jni(),
            JValue::Int(pos[1]).as_jni(),
            JValue::Int(pos[2]).as_jni(),
            obj(&state),
        ];
        let boolean = ReturnType::Primitive(Primitive::Boolean);
        // SAFETY: `blockBreak`'s signature in `load`.
        unsafe { events.invoke(env, BLOCK_BREAK, boolean, &args) }?.z()
    })
    .unwrap_or(false)
}

extern "system" fn world_time(mut env: JNIEnv, _: JClass, world: jint, full: jboolean) -> jlong {
    guard(&mut env, 0, |_| {
        let world = usize::try_from(world).unwrap_or(usize::MAX);
        with_host(|h| h.world_time(world, full != 0)).ok_or(Fail::NoHost("World.getTime"))
    })
}

extern "system" fn block_at(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    x: jint,
    y: jint,
    z: jint,
) -> jstring {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let world = usize::try_from(world).unwrap_or(usize::MAX);
        let state =
            with_host(|h| h.block_at(world, [x, y, z])).ok_or(Fail::NoHost("Block.getType"))?;
        match state {
            Some(s) => Ok(env.new_string(s)?.into_raw()),
            None => Ok(std::ptr::null_mut()),
        }
    })
}
