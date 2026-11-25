//! Which Bukkit events have listeners: one bit per event type, latched once
//! per tick.
//!
//! Every event site in the game starts with `if listens(Event::X)`: with no
//! listener that is one relaxed load and one predictable branch, nothing else
//! (no allocation, no JNI). The bits change only between ticks (in [`tick`]
//! and [`enable`]), never during one, so every decision a tick makes about an
//! event (the hub leaving a join message to plugins, a parallel section
//! taking its serial path) holds for the whole tick.
//!
//! [`tick`]: crate::tick
//! [`enable`]: crate::enable
use std::sync::atomic::{AtomicU64, Ordering};

/// The bridged events, as bits of [`listens`]: `Bridge.EVENTS`' indices in
/// `java/bridge/dev/ferraloom/bukkit/Bridge.java`. Each area keeps its own
/// range (players 0-15, blocks 16-31; docs/ferraloom/plugins.md, "Fan-out").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Event {
    // Players.
    PlayerJoin = 0,
    PlayerQuit = 1,
    AsyncPlayerChat = 2,
    PlayerMove = 3,
    PlayerTeleport = 4,
    PlayerInteract = 5,
    PlayerCommandPreprocess = 6,
    PlayerToggleSneak = 7,
    PlayerToggleSprint = 8,
    PlayerToggleFlight = 9,
    PlayerItemHeld = 10,
    PlayerGameModeChange = 11,
    PlayerChangedWorld = 12,
    PlayerRespawn = 13,
    PlayerKick = 14,
    /// `PlayerDeathEvent`, which listens through `EntityDeathEvent`'s handler list.
    PlayerDeath = 15,
    // Blocks.
    BlockBreak = 16,
}

impl Event {
    pub const ALL: [Event; 17] = [
        Event::PlayerJoin,
        Event::PlayerQuit,
        Event::AsyncPlayerChat,
        Event::PlayerMove,
        Event::PlayerTeleport,
        Event::PlayerInteract,
        Event::PlayerCommandPreprocess,
        Event::PlayerToggleSneak,
        Event::PlayerToggleSprint,
        Event::PlayerToggleFlight,
        Event::PlayerItemHeld,
        Event::PlayerGameModeChange,
        Event::PlayerChangedWorld,
        Event::PlayerRespawn,
        Event::PlayerKick,
        Event::PlayerDeath,
        Event::BlockBreak,
    ];

    const fn bit(self) -> u64 {
        1 << self as u8
    }
}

/// Whether a plugin listens to any of `events` this tick: with a constant
/// list, one load and one test, as [`listens`].
#[inline(always)]
pub fn listens_any(events: &[Event]) -> bool {
    let mut mask = 0;
    let mut i = 0;
    while i < events.len() {
        mask |= events[i].bit();
        i += 1;
    }
    FLAGS.load(Ordering::Relaxed) & mask != 0
}

/// The bits of the events with listeners; 0 without plugins.
static FLAGS: AtomicU64 = AtomicU64::new(0);

/// Whether a plugin listens to `event` this tick.
#[inline(always)]
pub fn listens(event: Event) -> bool {
    FLAGS.load(Ordering::Relaxed) & event.bit() != 0
}

/// Latches what the bridge reported, between ticks. Bits for events this
/// side does not know are dropped.
pub(crate) fn set(bits: u64) {
    let known = Event::ALL.iter().fold(0, |m, e| m | e.bit());
    FLAGS.store(bits & known, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_name_their_events_only() {
        set(0b10 | 1 << 16 | 1 << 40);
        assert!(!listens(Event::PlayerJoin));
        assert!(listens(Event::PlayerQuit));
        assert!(!listens(Event::AsyncPlayerChat));
        assert!(listens(Event::BlockBreak));
        assert_eq!(FLAGS.load(Ordering::Relaxed), 0b10 | 1 << 16);
        set(0);
        assert!(Event::ALL.iter().all(|&e| !listens(e)));
    }
}
