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
/// range (players 0-15, blocks 16-31, worlds 32-39, inventory 48-55; docs/ferraloom/plugins.md, "Fan-out").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Event {
    // Players.
    PlayerJoin = 0,
    PlayerQuit = 1,
    AsyncPlayerChat = 2,
    // Blocks.
    BlockBreak = 16,
    BlockPlace = 17,
    BlockPhysics = 18,
    BlockFromTo = 19,
    BlockBurn = 20,
    BlockIgnite = 21,
    BlockGrow = 22,
    BlockFade = 23,
    BlockForm = 24,
    BlockSpread = 25,
    LeavesDecay = 26,
    BlockPistonExtend = 27,
    BlockPistonRetract = 28,
    BlockRedstone = 29,
    SignChange = 30,
    BlockExplode = 31,
    // Worlds.
    ChunkLoad = 32,
    ChunkUnload = 33,
    WeatherChange = 34,
    ThunderChange = 35,
    WorldSave = 36,
    // Inventory.
    InventoryClick = 48,
    InventoryDrag = 49,
    InventoryOpen = 50,
    InventoryClose = 51,
    InventoryMoveItem = 52,
    PlayerItemConsume = 54,
    PlayerSwapHandItems = 55,
    // Entities: 40-47. One bit can stand for a group of Bukkit events (`Bridge.EVENTS`).
    EntityDamage = 40,
    EntityDeath = 41,
    /// `EntitySpawnEvent`'s list: `CreatureSpawnEvent` is one.
    EntitySpawn = 42,
    EntityTarget = 43,
    EntityExplode = 44,
    PlayerInteractEntity = 45,
    EntityChangeBlock = 46,
    EntityCombust = 47,
}

impl Event {
    pub const ALL: [Event; 39] = [
        Event::PlayerJoin,
        Event::PlayerQuit,
        Event::AsyncPlayerChat,
        Event::BlockBreak,
        Event::BlockPlace,
        Event::BlockPhysics,
        Event::BlockFromTo,
        Event::BlockBurn,
        Event::BlockIgnite,
        Event::BlockGrow,
        Event::BlockFade,
        Event::BlockForm,
        Event::BlockSpread,
        Event::LeavesDecay,
        Event::BlockPistonExtend,
        Event::BlockPistonRetract,
        Event::BlockRedstone,
        Event::SignChange,
        Event::BlockExplode,
        Event::ChunkLoad,
        Event::ChunkUnload,
        Event::WeatherChange,
        Event::ThunderChange,
        Event::WorldSave,
        // Inventory.
        Event::InventoryClick,
        Event::InventoryDrag,
        Event::InventoryOpen,
        Event::InventoryClose,
        Event::InventoryMoveItem,
        Event::PlayerItemConsume,
        Event::PlayerSwapHandItems,
        Event::EntityDamage,
        Event::EntityDeath,
        Event::EntitySpawn,
        Event::EntityTarget,
        Event::EntityExplode,
        Event::PlayerInteractEntity,
        Event::EntityChangeBlock,
        Event::EntityCombust,
    ];

    /// The word of [`FLAGS`] holding the event's bit.
    const fn word(self) -> usize {
        self as usize / 64
    }

    const fn bit(self) -> u64 {
        1 << (self as u16 % 64)
    }

    /// The bits of `events`, all in one word, for [`listens_any`].
    pub const fn mask(events: &[Event]) -> Mask {
        let word = events[0].word();
        let mut bits = 0;
        let mut i = 0;
        while i < events.len() {
            assert!(events[i].word() == word, "a mask's events share one word");
            bits |= events[i].bit();
            i += 1;
        }
        Mask { word, bits }
    }
}

/// Events of one word of [`FLAGS`], tested together ([`listens_any`]).
#[derive(Clone, Copy, Debug)]
pub struct Mask {
    word: usize,
    bits: u64,
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

/// Whether a plugin listens to any event of `mask` ([`Event::mask`]) this
/// tick: one load and one test, as [`listens`].
#[inline(always)]
pub fn listens_any(mask: Mask) -> bool {
    FLAGS[mask.word].load(Ordering::Relaxed) & mask.bits != 0
}

/// Latches the listener bits of `events` alone, for tests of event sites in
/// other crates (no JVM). Never called by the game.
#[doc(hidden)]
pub fn latch_for_tests(events: &[Event]) {
    let mut words = [0; WORDS];
    for e in events {
        words[e.word()] |= e.bit();
    }
    set(&words);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_name_their_events_only() {
        set(&[0b10 | 1 << 16 | 1 << 39, u64::MAX >> 1]);
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
        assert!(listens_any(Event::mask(&[
            Event::PlayerJoin,
            Event::PlayerQuit
        ])));
        assert!(!listens_any(Event::mask(&[
            Event::PlayerJoin,
            Event::EntityTarget
        ])));
        set(&[]);
        assert!(Event::ALL.iter().all(|&e| !listens(e)));
        assert!(FLAGS.iter().all(|f| f.load(Ordering::Relaxed) == 0));
    }

    #[test]
    fn every_event_has_a_word() {
        assert!(Event::ALL.iter().all(|e| e.word() < WORDS));
    }
}
