//! Per-state bits for the blocks that per-tick contact checks look for by
//! name (fire, cactus, flowers, powder snow...). With them a section's
//! palette answers whether such a block can be there at all with a table
//! lookup per entry, before any cell is read ([`crate::world::World::may_hold`]).
use horizon_world::registry::{BlockRegistry, BlockStateId};
use std::sync::{Mutex, OnceLock};

pub const FIRE: u16 = 1 << 0;
pub const SOUL_FIRE: u16 = 1 << 1;
pub const CACTUS: u16 = 1 << 2;
pub const SWEET_BERRY_BUSH: u16 = 1 << 3;
pub const WITHER_ROSE: u16 = 1 << 4;
pub const OPEN_EYEBLOSSOM: u16 = 1 << 5;
pub const POWDER_SNOW: u16 = 1 << 6;
pub const BUBBLE_COLUMN: u16 = 1 << 7;
pub const TURTLE_EGG: u16 = 1 << 8;

const NAMES: [(&str, u16); 9] = [
    ("minecraft:fire", FIRE),
    ("minecraft:soul_fire", SOUL_FIRE),
    ("minecraft:cactus", CACTUS),
    ("minecraft:sweet_berry_bush", SWEET_BERRY_BUSH),
    ("minecraft:wither_rose", WITHER_ROSE),
    ("minecraft:open_eyeblossom", OPEN_EYEBLOSSOM),
    ("minecraft:powder_snow", POWDER_SNOW),
    ("minecraft:bubble_column", BUBBLE_COLUMN),
    ("minecraft:turtle_egg", TURTLE_EGG),
];

/// The kind bits of every state of one registry.
pub struct BlockKinds {
    registry: usize,
    bits: Vec<u16>,
}
impl BlockKinds {
    fn build(registry: &BlockRegistry) -> Self {
        let bits = (0..registry.len() as u32)
            .map(|id| {
                registry.state(BlockStateId(id)).map_or(0, |state| {
                    NAMES
                        .iter()
                        .find(|(name, _)| state.name == *name)
                        .map_or(0, |&(_, bit)| bit)
                })
            })
            .collect();
        Self {
            registry: registry as *const BlockRegistry as usize,
            bits,
        }
    }
    /// The table of `registry`, built on first use. A process has one
    /// registry in play; others (tests) get theirs kept beside it.
    pub fn of(registry: &BlockRegistry) -> &'static Self {
        static FIRST: OnceLock<BlockKinds> = OnceLock::new();
        static OTHERS: Mutex<Vec<&'static BlockKinds>> = Mutex::new(Vec::new());
        let key = registry as *const BlockRegistry as usize;
        let first = FIRST.get_or_init(|| Self::build(registry));
        if first.registry == key && first.bits.len() == registry.len() {
            return first;
        }
        let mut others = OTHERS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(kinds) = others
            .iter()
            .find(|k| k.registry == key && k.bits.len() == registry.len())
        {
            return kinds;
        }
        let kinds: &'static Self = Box::leak(Box::new(Self::build(registry)));
        others.push(kinds);
        kinds
    }
    /// Whether `state` is one of the kinds in `mask`. States outside the
    /// registry are none of them.
    #[inline]
    pub fn is(&self, state: BlockStateId, mask: u16) -> bool {
        self.bits
            .get(state.0 as usize)
            .is_some_and(|bits| bits & mask != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each named block's states carry its bit, and only those.
    #[test]
    fn kinds_mark_every_state_of_their_blocks() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        if !root.join("cached-collision.nbt").exists() {
            return;
        }
        let assets = crate::assets::Assets::load(&root).unwrap();
        let registry = assets.registry;
        let kinds = BlockKinds::of(registry);
        for id in 0..registry.len() as u32 {
            let state = BlockStateId(id);
            let name = &registry.state(state).unwrap().name;
            for (block, bit) in NAMES {
                assert_eq!(kinds.is(state, bit), name == block, "{name} {block}");
            }
        }
        assert!(std::ptr::eq(kinds, BlockKinds::of(registry)));
    }
}
