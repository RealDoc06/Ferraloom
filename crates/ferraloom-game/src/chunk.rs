//! Resident client chunk storage. Sections are reference-counted so worker
//! threads can mesh and light immutable snapshots while the game thread edits
//! (copy-on-write through `Arc::make_mut`).
use horizon_core::paletted::{PalettedContainer, Strategy};
use horizon_world::registry::BlockStateId;
use std::sync::Arc;

pub const MIN_Y: i32 = -64;
pub const HEIGHT: i32 = 384;
pub const SECTIONS: usize = (HEIGHT / 16) as usize;

/// Biomes are 4x4x4 containers over the biome table (67 entries: a 7-bit
/// global palette).
pub const BIOME_STRATEGY: Strategy = Strategy::biomes(7);

/// Bits of the block states' global palette: 16 for vanilla's 35,723 states
/// (what vanilla servers and clients send), more when content mods add states
/// past 65,536 (docs/modpacks.md). Set once, before the first chunk.
static BLOCK_BITS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(16);

/// Block states are `PalettedContainer`s over the registry: vanilla's 16-bit
/// global palette, wider with enough content mods.
pub fn block_strategy() -> Strategy {
    Strategy::blocks(BLOCK_BITS.load(std::sync::atomic::Ordering::Relaxed))
}

/// Frees `columns` (`count` of them) on a short-lived thread of its own
/// when there are many: a teleport or a dimension change drops a few
/// thousand columns at once, and freeing their sections took the frame
/// 15-20 ms. A few are freed here.
pub fn free_columns<T: Send + 'static>(columns: T, count: usize) {
    if count < 64 {
        return;
    }
    // Should the thread not start, the closure drops `columns` here.
    let _ = std::thread::Builder::new()
        .name("Chunk freeing".into())
        .spawn(move || drop(columns));
}

/// Sizes the block states' global palette for a registry of `states`.
pub fn set_block_states(states: usize) {
    let bits = horizon_core::paletted::ceil_log2(states).max(16);
    BLOCK_BITS.store(bits, std::sync::atomic::Ordering::Relaxed);
}
/// Packed light per block: sky light in the high nibble, block light in the low one.
#[derive(Clone)]
pub enum Light {
    Uniform(u8),
    Full(Box<[u8; 4096]>),
}
impl Light {
    #[inline]
    pub fn get(&self, i: usize) -> u8 {
        match self {
            Self::Uniform(v) => *v,
            Self::Full(a) => a[i],
        }
    }
}
#[inline]
pub fn index(x: i32, y: i32, z: i32) -> usize {
    ((y & 15) << 8 | (z & 15) << 4 | (x & 15)) as usize
}
#[derive(Clone)]
pub struct Section {
    pub blocks: PalettedContainer,
    /// Biome table index per 4x4x4 cell, `(y << 2 | z) << 2 | x`.
    pub biomes: PalettedContainer,
    pub light: Light,
}
impl Default for Section {
    fn default() -> Self {
        Self {
            blocks: PalettedContainer::new(block_strategy(), 0),
            biomes: PalettedContainer::new(BIOME_STRATEGY, 0),
            light: Light::Uniform(0xF0),
        }
    }
}
impl Section {
    #[inline]
    pub fn get(&self, i: usize) -> u32 {
        self.blocks.get(i)
    }
    pub fn set(&mut self, i: usize, v: u32) -> u32 {
        self.blocks.set(i, v)
    }
    /// All 4,096 block states in index order.
    pub fn unpack(&self, out: &mut [u32; 4096]) {
        self.blocks.unpack(out);
    }
    /// The block states of a whole section at once.
    pub fn from_blocks(values: &[u32; 4096]) -> PalettedContainer {
        PalettedContainer::from_values(block_strategy(), values)
    }
    #[inline]
    pub fn light(&self, i: usize) -> u8 {
        self.light.get(i)
    }
    pub fn is_uniform(&self, v: u32) -> bool {
        self.blocks.is_uniform(v)
    }
    /// Heap bytes held by the block, biome and light storage (benchmark accounting).
    pub fn heap_bytes(&self) -> usize {
        self.blocks.heap_bytes()
            + self.biomes.heap_bytes()
            + match &self.light {
                Light::Uniform(_) => 0,
                Light::Full(_) => 4096,
            }
    }
    /// Collapse storage that became uniform, and drop palette entries no
    /// block uses any more.
    pub fn compact(&mut self) {
        for container in [&mut self.blocks, &mut self.biomes] {
            if container.bits() != 0
                && let Some(v) = container.uniform()
            {
                *container = PalettedContainer::new(container.strategy(), v);
            }
        }
        if let Light::Full(a) = &self.light
            && a.iter().all(|&b| b == a[0])
        {
            self.light = Light::Uniform(a[0]);
        }
    }
}
/// Borrowed block access with a selected section, without retaining an Arc.
#[derive(Clone, Copy)]
pub struct ChunkBlocks<'a> {
    sections: &'a [Arc<Section>],
    section_y: i32,
    section: Option<&'a Section>,
}
impl<'a> ChunkBlocks<'a> {
    pub fn new(sections: &'a [Arc<Section>]) -> Self {
        Self {
            sections,
            section_y: i32::MIN,
            section: None,
        }
    }
    #[inline]
    pub fn select_section(mut self, section_y: i32) -> Self {
        if self.section_y == section_y {
            return self;
        }
        self.section_y = section_y;
        self.section = section_y
            .checked_sub(MIN_Y >> 4)
            .and_then(|s| usize::try_from(s).ok())
            .and_then(|s| self.sections.get(s))
            .map(Arc::as_ref);
        self
    }
    #[inline]
    pub fn block(&self, x: i32, y: i32, z: i32) -> BlockStateId {
        if y >> 4 != self.section_y {
            let s = (y - MIN_Y) >> 4;
            return BlockStateId(if !(0..SECTIONS as i32).contains(&s) {
                0
            } else {
                self.sections[s as usize].get(index(x, y, z))
            });
        }
        BlockStateId(
            self.section
                .map_or(0, |section| section.get(index(x, y, z))),
        )
    }
    #[inline]
    pub fn section(&self) -> Option<&'a Section> {
        self.section
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_snapshots_keep_blocks_biomes_and_light_after_live_edits() {
        let mut live: Vec<_> = (0..SECTIONS)
            .map(|_| Arc::new(Section::default()))
            .collect();
        let snapshot = live.clone();
        let s = Arc::make_mut(&mut live[0]);
        s.set(index(-1, MIN_Y, -1), 42);
        s.biomes.set(0, 7);
        s.light = Light::Uniform(0x12);

        let before = ChunkBlocks::new(&snapshot).select_section(MIN_Y >> 4);
        let after = ChunkBlocks::new(&live).select_section(MIN_Y >> 4);
        assert_eq!(before.block(-1, MIN_Y, -1), BlockStateId(0));
        assert_eq!(after.block(-1, MIN_Y, -1), BlockStateId(42));
        assert_eq!(snapshot[0].biomes.get(0), 0);
        assert_eq!(live[0].biomes.get(0), 7);
        assert_eq!(snapshot[0].light(0), 0xf0);
        assert_eq!(live[0].light(0), 0x12);
        assert!(Arc::ptr_eq(&live[1], &snapshot[1]));
        // Both unselected and selected reads retain the original height rules.
        let unselected = ChunkBlocks::new(&live);
        for y in [
            MIN_Y - 1,
            MIN_Y,
            MIN_Y + 16,
            MIN_Y + HEIGHT - 1,
            MIN_Y + HEIGHT,
        ] {
            assert_eq!(unselected.block(-1, y, -1), after.block(-1, y, -1));
        }
    }
}
