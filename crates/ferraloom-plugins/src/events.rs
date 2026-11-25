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
/// range (players 0-15, blocks 16-31, inventory 48-55; docs/ferraloom/plugins.md, "Fan-out").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Event {
    // Players.
    PlayerJoin = 0,
    PlayerQuit = 1,
    AsyncPlayerChat = 2,
    // Blocks.
    BlockBreak = 16,
    // Inventory.
    InventoryClick = 48,
    InventoryDrag = 49,
    InventoryOpen = 50,
    InventoryClose = 51,
    InventoryMoveItem = 52,
    PlayerItemConsume = 54,
    PlayerSwapHandItems = 55,
}

impl Event {
    pub const ALL: [Event; 11] = [
        Event::PlayerJoin,
        Event::PlayerQuit,
        Event::AsyncPlayerChat,
        Event::BlockBreak,
        // Inventory.
        Event::InventoryClick,
        Event::InventoryDrag,
        Event::InventoryOpen,
        Event::InventoryClose,
        Event::InventoryMoveItem,
        Event::PlayerItemConsume,
        Event::PlayerSwapHandItems,
    ];

    /// The word of [`FLAGS`] holding the event's bit.
    const fn word(self) -> usize {
        self as usize / 64
    }

    const fn bit(self) -> u64 {
        1 << (self as u16 % 64)
    }
}

/// How many words of bits there are: event `e` is bit `e % 64` of word
/// `e / 64`. Word 0 holds the first ranges (its bit 63 is Java's "the command
/// labels changed", never latched here); each area then has a word of its
/// own: inventory 64-127, players 128-191, blocks 192-255, entities 256-319,
/// server 320-383, worlds 384-447.
pub(crate) const WORDS: usize = 7;

/// The bits of the events with listeners; all 0 without plugins.
static FLAGS: [AtomicU64; WORDS] = [const { AtomicU64::new(0) }; WORDS];

/// Whether a plugin listens to `event` this tick. `event` is a constant at
/// every site, so its word and bit are too: one relaxed load and one test.
#[inline(always)]
pub fn listens(event: Event) -> bool {
    FLAGS[event.word()].load(Ordering::Relaxed) & event.bit() != 0
}

/// Latches what the bridge reported, between ticks, word by word (missing
/// words are 0). Bits for events this side does not know are dropped.
pub(crate) fn set(words: &[u64]) {
    for (w, flags) in FLAGS.iter().enumerate() {
        let known = Event::ALL
            .iter()
            .filter(|e| e.word() == w)
            .fold(0, |m, e| m | e.bit());
        flags.store(
            words.get(w).copied().unwrap_or(0) & known,
            Ordering::Relaxed,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_name_their_events_only() {
        set(&[0b10 | 1 << 16 | 1 << 40, u64::MAX >> 1]);
        assert!(!listens(Event::PlayerJoin));
        assert!(listens(Event::PlayerQuit));
        assert!(!listens(Event::AsyncPlayerChat));
        assert!(listens(Event::BlockBreak));
        assert_eq!(FLAGS[0].load(Ordering::Relaxed), 0b10 | 1 << 16);
        // Only known events' bits are kept, in every word.
        let known = |w: usize| {
            Event::ALL
                .iter()
                .filter(|e| e.word() == w)
                .fold(0, |m, e| m | e.bit())
        };
        assert_eq!(FLAGS[1].load(Ordering::Relaxed), known(1));
        set(&[]);
        assert!(Event::ALL.iter().all(|&e| !listens(e)));
        assert!(FLAGS.iter().all(|f| f.load(Ordering::Relaxed) == 0));
    }

    #[test]
    fn every_event_has_a_word() {
        assert!(Event::ALL.iter().all(|e| e.word() < WORDS));
    }
}
