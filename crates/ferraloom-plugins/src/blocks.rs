//! The blocks area: `Block`, `BlockData`, `Material`'s block side, and the
//! block events (`java/bridge/.../BlockNative.java`, `BlockEvents.java`).
//!
//! Blocks cross JNI as global state ids (vanilla's `Block.BLOCK_STATE_REGISTRY`
//! ids, which Horizon's registry keeps). The bridge learns the whole block
//! table once ([`BlockHost::block_table`]) and does `BlockData`'s property
//! arithmetic itself, so reading a block is one native call returning an int.
use crate::{
    PlayerId,
    bridge::{self, Entry, obj},
    natives::{self, Fail, guard, jbool, with_host},
};
use jni::{
    JNIEnv,
    objects::{JClass, JValue},
    signature::{Primitive, ReturnType},
    sys::{JNI_FALSE, jboolean, jint, jstring},
};
use std::{ffi::c_void, sync::OnceLock};

/// What `Block` reads and changes of the game. Worlds are indices into
/// [`crate::Config::worlds`]; states are global state ids.
///
/// Every method has a default that answers as an unloaded world does
/// (nothing there, nothing changed): a restricted host (a level's
/// behaviour code mid-update, a test) implements what it can reach.
pub trait BlockHost {
    /// Every block, one line each, for the bridge's `Material` and
    /// `BlockData` (the format is [`table_line`]'s). Asked once, at start.
    fn block_table(&mut self) -> String {
        String::new()
    }
    /// The state at a position; none where no chunk is loaded.
    fn block_state(&mut self, _world: usize, _pos: [i32; 3]) -> Option<u32> {
        None
    }
    /// `CraftBlock.setTypeAndData`: the state with (`physics`, flags 3) or
    /// without (flags 2) neighbour updates. False when nothing changed or no
    /// chunk is loaded there.
    fn set_block_state(
        &mut self,
        _world: usize,
        _pos: [i32; 3],
        _state: u32,
        _physics: bool,
    ) -> bool {
        false
    }
    /// Sky light, block light, and `getMaxLocalRawBrightness` (the sky
    /// darkened by the time of day); none where no chunk is loaded.
    fn light(&mut self, _world: usize, _pos: [i32; 3]) -> Option<[u8; 3]> {
        None
    }
    /// The biome's key (`minecraft:plains`); none where no chunk is loaded.
    fn biome(&mut self, _world: usize, _pos: [i32; 3]) -> Option<String> {
        None
    }
    /// `Level.destroyBlock(pos, true)`: the block breaks with its drops and
    /// effects. False when there was nothing to break.
    fn break_naturally(&mut self, _world: usize, _pos: [i32; 3]) -> bool {
        false
    }
    /// A sign's lines on one side (`front` or back); none where there is no sign.
    fn sign_lines(&mut self, _world: usize, _pos: [i32; 3], _front: bool) -> Option<Vec<String>> {
        None
    }
    /// Writes a sign's lines on one side, as its players see them; false
    /// where there is no sign.
    fn set_sign_lines(
        &mut self,
        _world: usize,
        _pos: [i32; 3],
        _front: bool,
        _lines: &[String],
    ) -> bool {
        false
    }
}

/// One line of [`BlockHost::block_table`]: `name base count default flags
/// hardness resistance friction item prop=v1,v2 ...`, the properties in
/// vanilla's order (by name; the last varies fastest in the ids). `flags`:
/// 1 air, 2 solid (`blocksMotion`), 4 gravity (a `FallingBlock`), 8
/// occluding (`canOcclude`), 32 has a block entity, 64 burnable (fire's
/// ignite odds), 128 needs the right tool for drops; bits 8 to 11 the light
/// it gives off. Of the block's default state.
#[allow(clippy::too_many_arguments)]
pub fn table_line(
    out: &mut String,
    name: &str,
    base: u32,
    count: u32,
    default: u32,
    flags: u32,
    [hardness, resistance, friction]: [f32; 3],
    item: &str,
    properties: &[(&str, Vec<&str>)],
) {
    use std::fmt::Write;
    let _ = write!(
        out,
        "{name} {base} {count} {default} {flags} {hardness} {resistance} {friction} {item}"
    );
    for (key, values) in properties {
        let _ = write!(out, " {key}={}", values.join(","));
    }
    out.push('\n');
}

/// What a `BlockBreakEvent` left: whether the break goes on, whether the
/// block drops its items, and the experience it drops.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BreakOutcome {
    pub cancelled: bool,
    pub drop_items: bool,
    pub exp: i32,
}

/// The block events with no more than a few numbers, by `BlockEvents.simple`'s
/// `kind` (same values there). Each says what its `a`..`e` are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Simple {
    /// `BlockPhysicsEvent`: `a` the changed state (the block's own), `b c d` the source position.
    Physics = 0,
    /// `BlockFromToEvent`: the block flows to `b c d`; `a` the face (Bukkit's `BlockFace` ordinal).
    FromTo = 1,
    /// `BlockBurnEvent`: `b c d` the fire's position (`a` 1 when there is one).
    Burn = 2,
    /// `BlockIgniteEvent`: `a` the cause (`IgniteCause` ordinal), `b c d` the igniting block when `e` is 1.
    Ignite = 3,
    /// `BlockGrowEvent`: `a` the new state.
    Grow = 4,
    /// `BlockFadeEvent`: `a` the new state.
    Fade = 5,
    /// `BlockFormEvent`: `a` the new state.
    Form = 6,
    /// `BlockSpreadEvent`: `a` the new state, `b c d` the source.
    Spread = 7,
    /// `LeavesDecayEvent`.
    LeavesDecay = 8,
    /// `BlockRedstoneEvent`: `a` the old current, `b` the new; answers the current to use.
    Redstone = 9,
}

const S: &str = "Ljava/lang/String;";
const BLOCK_BREAK: usize = 0;
const BLOCK_PLACE: usize = 1;
const SIMPLE: usize = 2;
const PISTON: usize = 3;
const SIGN_CHANGE: usize = 4;
const EXPLODE: usize = 5;

static EVENTS: OnceLock<Entry<6>> = OnceLock::new();

/// At start: `BlockNative`'s methods bound, `BlockEvents`' looked up.
pub(crate) fn load(env: &mut JNIEnv) -> jni::errors::Result<()> {
    natives::register(
        env,
        "dev/ferraloom/bukkit/BlockNative",
        &[
            ("table", "()Ljava/lang/String;", table as *mut c_void),
            ("state", "(IIII)I", state as *mut c_void),
            ("setState", "(IIIIIZ)Z", set_state as *mut c_void),
            ("light", "(IIII)I", light as *mut c_void),
            ("biome", "(IIII)Ljava/lang/String;", biome as *mut c_void),
            ("breakNaturally", "(IIII)Z", break_naturally as *mut c_void),
            (
                "signLines",
                &format!("(IIIIZ)[{S}"),
                sign_lines as *mut c_void,
            ),
            (
                "setSignLines",
                &format!("(IIIIZ[{S})Z"),
                set_sign_lines as *mut c_void,
            ),
        ],
    )?;
    let entry = Entry::load(
        env,
        "dev/ferraloom/bukkit/BlockEvents",
        [
            ("blockBreak", "(JJIIIIII)J".to_owned()),
            ("blockPlace", "(JJIIIIIIIIIZ)Z".to_owned()),
            ("simple", "(IIIIIIIIII)I".to_owned()),
            ("piston", "(IIIIIZ[I)Z".to_owned()),
            ("signChange", format!("(JJIIIIZ[{S})[{S}")),
            ("explode", "(IIIIIF[I)[I".to_owned()),
        ],
    )?;
    let _ = EVENTS.set(entry);
    Ok(())
}

fn int(v: i32) -> jni::sys::jvalue {
    JValue::Int(v).as_jni()
}

fn world_arg(world: usize) -> jni::sys::jvalue {
    int(world as i32)
}

/// `BlockBreakEvent` for a player about to break `state` at `pos`, which
/// would drop `exp` experience. Without an answer from the bridge the break
/// goes on as vanilla's.
pub fn block_break(
    host: &mut dyn crate::PluginHost,
    id: PlayerId,
    world: usize,
    pos: [i32; 3],
    state: u32,
    exp: i32,
) -> BreakOutcome {
    let vanilla = BreakOutcome {
        cancelled: false,
        drop_items: true,
        exp,
    };
    let (most, least) = natives::halves(id);
    let Some(events) = EVENTS.get() else {
        return vanilla;
    };
    let packed = bridge::call(host, |env| {
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            world_arg(world),
            int(pos[0]),
            int(pos[1]),
            int(pos[2]),
            int(state as i32),
            int(exp),
        ];
        let long = ReturnType::Primitive(Primitive::Long);
        // SAFETY: `blockBreak`'s signature in `load`.
        unsafe { events.invoke(env, BLOCK_BREAK, long, &args) }?.j()
    });
    packed.map_or(vanilla, unpack_break)
}

/// `BlockEvents.blockBreak`'s answer: bit 32 cancelled, bit 33 no drops, the
/// low 32 bits the experience.
fn unpack_break(packed: i64) -> BreakOutcome {
    BreakOutcome {
        cancelled: packed & 1 << 32 != 0,
        drop_items: packed & 1 << 33 == 0,
        exp: packed as i32,
    }
}

/// A block a player is about to place, for `BlockPlaceEvent`.
pub struct Placing {
    pub player: PlayerId,
    pub world: usize,
    pub pos: [i32; 3],
    /// The state to be placed, and the one it replaces.
    pub placed: u32,
    pub replaced: u32,
    /// The block clicked.
    pub against: [i32; 3],
    /// 0 the main hand, 1 the off hand.
    pub hand: i32,
    /// `canBuild`'s starting value (spawn protection allows it).
    pub can_build: bool,
}

/// `BlockPlaceEvent` before the block is placed: true when a plugin
/// cancelled it (or left `canBuild` false), and nothing may be placed.
pub fn block_place(host: &mut dyn crate::PluginHost, p: &Placing) -> bool {
    let (most, least) = natives::halves(p.player);
    let Some(events) = EVENTS.get() else {
        return false;
    };
    bridge::call(host, |env| {
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            world_arg(p.world),
            int(p.pos[0]),
            int(p.pos[1]),
            int(p.pos[2]),
            int(p.placed as i32),
            int(p.replaced as i32),
            int(p.against[0]),
            int(p.against[1]),
            int(p.against[2]),
            JValue::Bool(p.can_build.into()).as_jni(),
        ];
        let boolean = ReturnType::Primitive(Primitive::Boolean);
        // SAFETY: `blockPlace`'s signature in `load`.
        unsafe { events.invoke(env, BLOCK_PLACE, boolean, &args) }?.z()
    })
    .unwrap_or(false)
}

/// One of the [`Simple`] events at `pos`: its answer (1 cancelled, 0 not;
/// `Redstone`'s the current to use). `none` when the bridge could not answer.
pub fn simple(
    host: &mut dyn crate::PluginHost,
    kind: Simple,
    world: usize,
    pos: [i32; 3],
    [a, b, c, d, e]: [i32; 5],
    none: i32,
) -> i32 {
    let Some(events) = EVENTS.get() else {
        return none;
    };
    bridge::call(host, |env| {
        let args = [
            int(kind as i32),
            world_arg(world),
            int(pos[0]),
            int(pos[1]),
            int(pos[2]),
            int(a),
            int(b),
            int(c),
            int(d),
            int(e),
        ];
        let int = ReturnType::Primitive(Primitive::Int);
        // SAFETY: `simple`'s signature in `load`.
        unsafe { events.invoke(env, SIMPLE, int, &args) }?.i()
    })
    .unwrap_or(none)
}

/// [`simple`] for a cancellable event: true when cancelled.
pub fn cancelled(
    host: &mut dyn crate::PluginHost,
    kind: Simple,
    world: usize,
    pos: [i32; 3],
    args: [i32; 5],
) -> bool {
    simple(host, kind, world, pos, args, 0) != 0
}

/// `BlockPistonExtendEvent` (`extending`) or `BlockPistonRetractEvent` for
/// the piston at `pos` facing `direction` (a `BlockFace` ordinal), moving
/// `moved` (positions): true when cancelled.
pub fn piston(
    host: &mut dyn crate::PluginHost,
    world: usize,
    pos: [i32; 3],
    direction: i32,
    extending: bool,
    moved: &[[i32; 3]],
) -> bool {
    let Some(events) = EVENTS.get() else {
        return false;
    };
    bridge::call(host, |env| {
        let flat: Vec<i32> = moved.iter().flatten().copied().collect();
        let array = env.new_int_array(flat.len() as i32)?;
        env.set_int_array_region(&array, 0, &flat)?;
        let args = [
            world_arg(world),
            int(pos[0]),
            int(pos[1]),
            int(pos[2]),
            int(direction),
            JValue::Bool(extending.into()).as_jni(),
            obj(&array),
        ];
        let boolean = ReturnType::Primitive(Primitive::Boolean);
        // SAFETY: `piston`'s signature in `load`.
        unsafe { events.invoke(env, PISTON, boolean, &args) }?.z()
    })
    .unwrap_or(false)
}

/// `SignChangeEvent` for the lines a player wrote on the sign at `pos`
/// (`front` side): the lines to keep (legacy text), none when cancelled.
pub fn sign_change(
    host: &mut dyn crate::PluginHost,
    player: PlayerId,
    world: usize,
    pos: [i32; 3],
    front: bool,
    lines: &[String],
) -> Option<Vec<String>> {
    let (most, least) = natives::halves(player);
    let Some(events) = EVENTS.get() else {
        return Some(lines.to_vec());
    };
    bridge::call(host, |env| {
        let array = env.new_object_array(
            lines.len() as i32,
            "java/lang/String",
            jni::objects::JObject::null(),
        )?;
        for (i, line) in lines.iter().enumerate() {
            let s = env.new_string(line)?;
            env.set_object_array_element(&array, i as i32, s)?;
        }
        let args = [
            JValue::Long(most).as_jni(),
            JValue::Long(least).as_jni(),
            world_arg(world),
            int(pos[0]),
            int(pos[1]),
            int(pos[2]),
            JValue::Bool(front.into()).as_jni(),
            obj(&array),
        ];
        // SAFETY: `signChange`'s signature in `load`.
        let out = unsafe { events.invoke(env, SIGN_CHANGE, ReturnType::Array, &args) }?.l()?;
        if out.is_null() {
            return Ok(None);
        }
        let out = jni::objects::JObjectArray::from(out);
        let mut kept = Vec::new();
        for i in 0..env.get_array_length(&out)? {
            let line = env.get_object_array_element(&out, i)?;
            kept.push(if line.is_null() {
                String::new()
            } else {
                env.get_string(&jni::objects::JString::from(line))?.into()
            });
        }
        Ok(Some(kept))
    })
    .unwrap_or_else(|| Some(lines.to_vec()))
}

/// `BlockExplodeEvent` for an explosion at `pos` whose exploding block was
/// `state`, about to destroy `blocks` with `yield_` drop chance: the blocks
/// to destroy (a subset, in order) and the yield, none when cancelled.
pub fn block_explode(
    host: &mut dyn crate::PluginHost,
    world: usize,
    pos: [i32; 3],
    state: u32,
    blocks: &[[i32; 3]],
    yield_: f32,
) -> Option<(Vec<[i32; 3]>, f32)> {
    let unchanged = || Some((blocks.to_vec(), yield_));
    let Some(events) = EVENTS.get() else {
        return unchanged();
    };
    bridge::call(host, |env| {
        let flat: Vec<i32> = blocks.iter().flatten().copied().collect();
        let array = env.new_int_array(flat.len() as i32)?;
        env.set_int_array_region(&array, 0, &flat)?;
        let args = [
            world_arg(world),
            int(pos[0]),
            int(pos[1]),
            int(pos[2]),
            int(state as i32),
            JValue::Float(yield_).as_jni(),
            obj(&array),
        ];
        // SAFETY: `explode`'s signature in `load`.
        let out = unsafe { events.invoke(env, EXPLODE, ReturnType::Array, &args) }?.l()?;
        if out.is_null() {
            return Ok(None);
        }
        let out = jni::objects::JIntArray::from(out);
        let mut flat = vec![0; env.get_array_length(&out)? as usize];
        env.get_int_array_region(&out, 0, &mut flat)?;
        Ok(Some(unpack_explosion(&flat)))
    })
    .unwrap_or_else(unchanged)
}

/// `BlockEvents.explode`'s answer: the yield's bits, then x y z per block.
fn unpack_explosion(flat: &[i32]) -> (Vec<[i32; 3]>, f32) {
    let Some((&bits, rest)) = flat.split_first() else {
        return (Vec::new(), 0.0);
    };
    let blocks = rest.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect();
    (blocks, f32::from_bits(bits as u32))
}

fn index(world: jint) -> usize {
    usize::try_from(world).unwrap_or(usize::MAX)
}

/// The table, read from the host once (at start, inside a lent call) and
/// kept: it is the same for the server's life.
static TABLE: OnceLock<String> = OnceLock::new();

extern "system" fn table(mut env: JNIEnv, _: JClass) -> jstring {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let table = match TABLE.get() {
            Some(t) => t,
            None => {
                let t = with_host(|h| h.block_table()).ok_or(Fail::NoHost("Material"))?;
                TABLE.get_or_init(|| t)
            }
        };
        Ok(env.new_string(table)?.into_raw())
    })
}

extern "system" fn state(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    x: jint,
    y: jint,
    z: jint,
) -> jint {
    guard(&mut env, -1, |_| {
        let state = with_host(|h| h.block_state(index(world), [x, y, z]))
            .ok_or(Fail::NoHost("Block.getType"))?;
        Ok(state.map_or(-1, |s| s as jint))
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn set_state(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    x: jint,
    y: jint,
    z: jint,
    state: jint,
    physics: jboolean,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        let Ok(state) = u32::try_from(state) else {
            return Ok(JNI_FALSE);
        };
        let set = with_host(|h| h.set_block_state(index(world), [x, y, z], state, physics != 0))
            .ok_or(Fail::NoHost("Block.setType"))?;
        Ok(jbool(set))
    })
}

extern "system" fn light(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    x: jint,
    y: jint,
    z: jint,
) -> jint {
    guard(&mut env, -1, |_| {
        let light = with_host(|h| h.light(index(world), [x, y, z]))
            .ok_or(Fail::NoHost("Block.getLightLevel"))?;
        Ok(light.map_or(-1, |[sky, block, max]| {
            jint::from(sky) << 8 | jint::from(block) << 4 | jint::from(max)
        }))
    })
}

extern "system" fn biome(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    x: jint,
    y: jint,
    z: jint,
) -> jstring {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let biome = with_host(|h| h.biome(index(world), [x, y, z]))
            .ok_or(Fail::NoHost("Block.getBiome"))?;
        match biome {
            Some(b) => Ok(env.new_string(b)?.into_raw()),
            None => Ok(std::ptr::null_mut()),
        }
    })
}

extern "system" fn break_naturally(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    x: jint,
    y: jint,
    z: jint,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |_| {
        let broke = with_host(|h| h.break_naturally(index(world), [x, y, z]))
            .ok_or(Fail::NoHost("Block.breakNaturally"))?;
        Ok(jbool(broke))
    })
}

extern "system" fn sign_lines(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    x: jint,
    y: jint,
    z: jint,
    front: jboolean,
) -> jni::sys::jobjectArray {
    guard(&mut env, std::ptr::null_mut(), |env| {
        let lines = with_host(|h| h.sign_lines(index(world), [x, y, z], front != 0))
            .ok_or(Fail::NoHost("Sign.getLine"))?;
        let Some(lines) = lines else {
            return Ok(std::ptr::null_mut());
        };
        let null = jni::objects::JObject::null();
        let array = env.new_object_array(lines.len() as i32, "java/lang/String", null)?;
        for (i, line) in lines.iter().enumerate() {
            let s = env.new_string(line)?;
            env.set_object_array_element(&array, i as i32, s)?;
        }
        Ok(array.into_raw())
    })
}

#[allow(clippy::too_many_arguments)]
extern "system" fn set_sign_lines(
    mut env: JNIEnv,
    _: JClass,
    world: jint,
    x: jint,
    y: jint,
    z: jint,
    front: jboolean,
    lines: jni::objects::JObjectArray,
) -> jboolean {
    guard(&mut env, JNI_FALSE, |env| {
        let mut text = Vec::new();
        for i in 0..env.get_array_length(&lines)? {
            let line = env.get_object_array_element(&lines, i)?;
            text.push(if line.is_null() {
                String::new()
            } else {
                env.get_string(&jni::objects::JString::from(line))?.into()
            });
        }
        let set = with_host(|h| h.set_sign_lines(index(world), [x, y, z], front != 0, &text))
            .ok_or(Fail::NoHost("Sign.update"))?;
        Ok(jbool(set))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_line_names_its_properties_in_order() {
        let mut out = String::new();
        let props = [("axis", vec!["x", "y", "z"])];
        table_line(
            &mut out,
            "minecraft:oak_log",
            136,
            3,
            137,
            2,
            [2.0, 2.0, 0.6],
            "minecraft:oak_log",
            &props,
        );
        assert_eq!(
            out,
            "minecraft:oak_log 136 3 137 2 2 2 0.6 minecraft:oak_log axis=x,y,z\n"
        );
    }

    #[test]
    fn break_answers_unpack() {
        let kept = unpack_break(1 << 32 | 7);
        assert_eq!(
            kept,
            BreakOutcome {
                cancelled: true,
                drop_items: true,
                exp: 7
            }
        );
        let bare = unpack_break(1 << 33);
        assert!(!bare.cancelled && !bare.drop_items && bare.exp == 0);
    }

    #[test]
    fn explosion_answers_unpack() {
        let flat = [0.5_f32.to_bits() as i32, 1, 2, 3, -4, 5, -6];
        assert_eq!(unpack_explosion(&flat), (vec![[1, 2, 3], [-4, 5, -6]], 0.5));
        assert_eq!(unpack_explosion(&[]), (Vec::new(), 0.0));
    }
}
