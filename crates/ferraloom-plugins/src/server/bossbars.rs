//! Plugins' boss bars (`Server.createBossBar`, `FerraloomBossBar.java`).
//!
//! The bridge keeps each bar's state and hands it over whole on every change,
//! from any thread (plugins update bars from async timers); the game shows the
//! bars to their players as it shows its own custom ones, diffed once a tick.
//! Without a plugin bar the game's check is one relaxed load.
use crate::{
    PlayerId,
    natives::{guard, player_id},
};
use jni::{
    JNIEnv,
    objects::{JClass, JLongArray, JString},
    sys::{jfloat, jint, jlong},
};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

/// A visible plugin boss bar with players.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginBossBar {
    pub id: u128,
    /// Legacy text (`§c` codes).
    pub title: String,
    pub progress: f32,
    /// `BossBarColor` and `BossBarOverlay` ordinals (Bukkit's `BarColor` and `BarStyle` have the same order).
    pub color: i32,
    pub overlay: i32,
    pub darken: bool,
    pub music: bool,
    pub fog: bool,
    pub players: Vec<PlayerId>,
}

static BARS: Mutex<Vec<PluginBossBar>> = Mutex::new(Vec::new());
/// Whether [`BARS`] holds any: the game's per-viewer check.
static ANY: AtomicBool = AtomicBool::new(false);

/// The plugin bars `viewer` sees, in creation order.
#[inline]
pub fn boss_bars_for(viewer: PlayerId, mut f: impl FnMut(&PluginBossBar)) {
    if !ANY.load(Ordering::Relaxed) {
        return;
    }
    for bar in BARS.lock().unwrap().iter() {
        if bar.players.contains(&viewer) {
            f(bar);
        }
    }
}

/// A bar's new state; one hidden or without players is dropped.
fn update(bar: PluginBossBar, shown: bool) {
    let mut bars = BARS.lock().unwrap();
    let at = bars.iter().position(|b| b.id == bar.id);
    match (at, shown && !bar.players.is_empty()) {
        (Some(i), true) => bars[i] = bar,
        (None, true) => bars.push(bar),
        (Some(i), false) => {
            bars.remove(i);
        }
        (None, false) => {}
    }
    ANY.store(!bars.is_empty(), Ordering::Relaxed);
}

/// `ServerNative.bossBar`'s binding.
pub(super) fn natives() -> Vec<(&'static str, String, *mut std::ffi::c_void)> {
    vec![(
        "bossBar",
        "(JJLjava/lang/String;FIII[J)V".into(),
        boss_bar as *mut _,
    )]
}

/// Flags: bit 0 darken the sky, 1 boss music, 2 fog, 3 visible.
#[allow(clippy::too_many_arguments)]
extern "system" fn boss_bar(
    mut env: JNIEnv,
    _: JClass,
    most: jlong,
    least: jlong,
    title: JString,
    progress: jfloat,
    color: jint,
    overlay: jint,
    flags: jint,
    players: JLongArray,
) {
    guard(&mut env, (), |env| {
        let title: String = env.get_string(&title)?.into();
        let len = env.get_array_length(&players)? as usize;
        let mut halves = vec![0; len];
        env.get_long_array_region(&players, 0, &mut halves)?;
        let bar = PluginBossBar {
            id: player_id(most, least),
            title,
            progress,
            color,
            overlay,
            darken: flags & 1 != 0,
            music: flags & 2 != 0,
            fog: flags & 4 != 0,
            players: halves
                .chunks_exact(2)
                .map(|h| player_id(h[0], h[1]))
                .collect(),
        };
        update(bar, flags & 8 != 0);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(id: u128, players: Vec<PlayerId>) -> PluginBossBar {
        PluginBossBar {
            id,
            title: "§cBoss".into(),
            progress: 0.5,
            color: 2,
            overlay: 0,
            darken: false,
            music: false,
            fog: false,
            players,
        }
    }

    #[test]
    fn bars_reach_their_players_until_hidden_or_emptied() {
        let seen = |viewer| {
            let mut ids = Vec::new();
            boss_bars_for(viewer, |b| ids.push(b.id));
            ids
        };
        update(bar(1, vec![7]), true);
        update(bar(2, vec![7, 8]), true);
        assert_eq!(seen(7), [1, 2]);
        assert_eq!(seen(8), [2]);
        update(bar(2, vec![8]), false);
        assert_eq!(seen(8), Vec::<u128>::new());
        update(bar(1, vec![]), true);
        assert!(!ANY.load(Ordering::Relaxed));
    }
}
