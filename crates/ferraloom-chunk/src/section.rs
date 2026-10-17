use ferraloom_data::{Block, BlockState, registries::fluid};

use crate::{PalettedContainer, Strategy};

/// ID in the dimension's live biome registry, including datapack biomes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BiomeId(pub u32);

/// Port of LevelChunkSection's four derived counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SectionCounts {
    pub non_empty_blocks: u16,
    pub fluids: u16,
    pub ticking_blocks: u16,
    pub ticking_fluids: u16,
}

fn contribution(state: BlockState) -> SectionCounts {
    let block = state.block();
    if block == Block::AIR || block == Block::CAVE_AIR || block == Block::VOID_AIR {
        return SectionCounts::default();
    }
    let data = state.data();
    SectionCounts {
        non_empty_blocks: 1,
        fluids: u16::from(!data.fluid.empty),
        ticking_blocks: u16::from(data.random_ticking),
        // LavaFluid#isRandomlyTicking applies to both source and flowing lava.
        ticking_fluids: u16::from(
            !data.fluid.empty
                && matches!(
                    data.fluid.fluid_type,
                    fluid::Id::LAVA | fluid::Id::FLOWING_LAVA
                ),
        ),
    }
}

/// Port of LevelChunkSection. Biome coordinates are quart coordinates (0..4).
#[derive(Clone, Debug)]
pub struct LevelChunkSection {
    states: PalettedContainer<BlockState>,
    biomes: PalettedContainer<BiomeId>,
    counts: SectionCounts,
}

impl LevelChunkSection {
    pub fn new(
        block_strategy: Strategy<BlockState>,
        biome_strategy: Strategy<BiomeId>,
        default_biome: BiomeId,
    ) -> Self {
        Self::from_containers(
            PalettedContainer::new(block_strategy, Block::AIR.default_state()),
            PalettedContainer::new(biome_strategy, default_biome),
        )
    }
    pub fn from_containers(
        states: PalettedContainer<BlockState>,
        biomes: PalettedContainer<BiomeId>,
    ) -> Self {
        assert_eq!(states.strategy().kind(), crate::ContainerKind::BlockStates);
        assert_eq!(biomes.strategy().kind(), crate::ContainerKind::Biomes);
        let mut section = Self {
            states,
            biomes,
            counts: SectionCounts::default(),
        };
        section.recalc_block_counts();
        section
    }
    pub fn states(&self) -> &PalettedContainer<BlockState> {
        &self.states
    }
    pub fn biomes(&self) -> &PalettedContainer<BiomeId> {
        &self.biomes
    }
    pub fn biomes_mut(&mut self) -> &mut PalettedContainer<BiomeId> {
        &mut self.biomes
    }
    pub fn counts(&self) -> SectionCounts {
        self.counts
    }
    pub fn get_block_state(&self, x: u8, y: u8, z: u8) -> BlockState {
        self.states.get(x, y, z)
    }
    pub fn set_block_state(&mut self, x: u8, y: u8, z: u8, state: BlockState) -> BlockState {
        let previous = self.states.set(x, y, z, state);
        let old = contribution(previous);
        let new = contribution(state);
        self.counts.non_empty_blocks =
            self.counts.non_empty_blocks - old.non_empty_blocks + new.non_empty_blocks;
        self.counts.fluids = self.counts.fluids - old.fluids + new.fluids;
        self.counts.ticking_blocks =
            self.counts.ticking_blocks - old.ticking_blocks + new.ticking_blocks;
        self.counts.ticking_fluids =
            self.counts.ticking_fluids - old.ticking_fluids + new.ticking_fluids;
        previous
    }
    pub fn has_only_air(&self) -> bool {
        self.counts.non_empty_blocks == 0
    }
    pub fn has_fluid(&self) -> bool {
        self.counts.fluids > 0
    }
    pub fn is_randomly_ticking_blocks(&self) -> bool {
        self.counts.ticking_blocks > 0
    }
    pub fn is_randomly_ticking_fluids(&self) -> bool {
        self.counts.ticking_fluids > 0
    }
    pub fn is_randomly_ticking(&self) -> bool {
        self.is_randomly_ticking_blocks() || self.is_randomly_ticking_fluids()
    }
    pub fn recalc_block_counts(&mut self) {
        let mut total = SectionCounts::default();
        for (state, count) in self.states.counts() {
            let c = contribution(state);
            let count = count as u16;
            total.non_empty_blocks += c.non_empty_blocks * count;
            total.fluids += c.fluids * count;
            total.ticking_blocks += c.ticking_blocks * count;
            total.ticking_fluids += c.ticking_fluids * count;
        }
        self.counts = total;
    }
}
