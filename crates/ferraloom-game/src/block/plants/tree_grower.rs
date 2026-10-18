//! Port of TreeGrower's species, weighted feature selection and sapling queries.
//! Feature placement is wired through the live worldgen boundary separately.
use super::super::{LevelAccessRef, LevelReaderRef};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockState, tags::BlockTags};
use ferraloom_util::{
    BlockPos,
    random::{Random, RandomSource},
};
use ferraloom_worldgen::feature::FeatureRegistry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeGrower {
    Oak,
    Spruce,
    Mangrove,
    Azalea,
    Birch,
    Jungle,
    Acacia,
    Cherry,
    DarkOak,
    PaleOak,
    Poplar,
}

/// The original four receivers and their positions, in vanilla's capture order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TwoByTwoSaplingPos {
    pub offset_x: i32,
    pub offset_z: i32,
    pub blocks: [(BlockState, BlockPos); 4],
}

fn choose(random: &mut Random, choices: &[(&'static str, i32)]) -> Option<&'static str> {
    if choices.is_empty() {
        return None;
    }
    // WeightedList draws even when the list contains a single weight-one entry.
    let mut selection = random.next_int_bound(choices.iter().map(|(_, weight)| weight).sum());
    for &(feature, weight) in choices {
        selection -= weight;
        if selection < 0 {
            return Some(feature);
        }
    }
    unreachable!("positive tree weights")
}

impl TreeGrower {
    pub fn for_block(block: Block) -> Option<Self> {
        Some(match block {
            Block::OAK_SAPLING => Self::Oak,
            Block::SPRUCE_SAPLING => Self::Spruce,
            Block::MANGROVE_PROPAGULE => Self::Mangrove,
            Block::AZALEA | Block::FLOWERING_AZALEA => Self::Azalea,
            Block::BIRCH_SAPLING => Self::Birch,
            Block::JUNGLE_SAPLING => Self::Jungle,
            Block::ACACIA_SAPLING => Self::Acacia,
            Block::CHERRY_SAPLING => Self::Cherry,
            Block::DARK_OAK_SAPLING => Self::DarkOak,
            Block::PALE_OAK_SAPLING => Self::PaleOak,
            Block::POPLAR_SAPLING => Self::Poplar,
            _ => return None,
        })
    }
    /// Port of TreeGrower#getConfiguredFeature. Flower variants replace the
    /// normal weighted list; they do not add another selection draw.
    pub fn configured_feature(self, random: &mut Random, flowers: bool) -> Option<&'static str> {
        let choices: &[(&str, i32)] = match self {
            Self::Oak if flowers => &[("oak_bees_005", 9), ("fancy_oak_bees_005", 1)],
            Self::Oak => &[("oak", 9), ("fancy_oak", 1)],
            Self::Spruce => &[("spruce", 1)],
            Self::Mangrove => &[("mangrove", 15), ("tall_mangrove", 85)],
            Self::Azalea => &[("azalea_tree", 1)],
            Self::Birch if flowers => &[("birch_bees_005", 1)],
            Self::Birch => &[("birch", 1)],
            Self::Jungle => &[("jungle_tree_no_vine", 1)],
            Self::Acacia => &[("acacia", 1)],
            Self::Cherry if flowers => &[("cherry_bees_005", 1)],
            Self::Cherry => &[("cherry", 1)],
            Self::DarkOak | Self::PaleOak => &[],
            Self::Poplar => &[
                ("red_poplar", 1),
                ("orange_poplar", 1),
                ("yellow_poplar", 1),
            ],
        };
        choose(random, choices)
    }
    pub fn configured_mega_feature(self, random: &mut Random) -> Option<&'static str> {
        choose(
            random,
            match self {
                Self::Spruce => &[("mega_spruce", 1), ("mega_pine", 1)],
                Self::Jungle => &[("mega_jungle_tree", 1)],
                Self::DarkOak => &[("dark_oak", 1)],
                Self::PaleOak => &[("pale_oak_bonemeal", 1)],
                _ => &[],
            },
        )
    }
    /// Port of TreeGrower#getMinimumHeight against the actual decoded feature.
    pub fn minimum_height(self, registry: &FeatureRegistry) -> Option<i32> {
        let name = match self {
            Self::Oak => "oak",
            Self::Spruce => "spruce",
            Self::Mangrove => "mangrove",
            Self::Azalea => "azalea_tree",
            Self::Birch => "birch",
            Self::Jungle => "jungle_tree_no_vine",
            Self::Acacia => "acacia",
            Self::Cherry => "cherry",
            Self::Poplar => "red_poplar",
            Self::DarkOak | Self::PaleOak => return None,
        };
        registry
            .features
            .get(&name.parse().expect("builtin feature identifier"))
            .and_then(|feature| feature.tree_minimum_height())
    }
    /// Port of TreeGrower#canGrow. The tree draw precedes the mega-tree draw.
    pub fn can_grow(
        self,
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let flowers = has_flowers(pos, &level.as_reader())?;
        let tree = self.configured_feature(level.random()?, flowers);
        let mega = self.configured_mega_feature(level.random()?);
        Ok(tree.is_some()
            || mega.is_none()
            || find_two_by_two(state, pos, &level.as_reader())?.is_some())
    }
}

pub fn has_flowers(pos: BlockPos, level: &LevelReaderRef<'_, '_>) -> Result<bool, LevelError> {
    let tags = level.block_tags()?;
    for z in -2..=2 {
        for y in -1..=1 {
            for x in -2..=2 {
                if tags.contains_key(
                    BlockTags::FLOWERS,
                    level.get_block_state(pos.offset(x, y, z))?.block(),
                ) {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

/// Port of TreeGrower#findTwoByTwoSaplingPos, retaining all four captured states
/// even if their stages differ, so failed growth can restore each exact receiver.
pub fn find_two_by_two(
    state: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<Option<TwoByTwoSaplingPos>, LevelError> {
    for dx in [0, -1] {
        for dz in [0, -1] {
            let positions = [
                pos.offset(dx, 0, dz),
                pos.offset(dx + 1, 0, dz),
                pos.offset(dx, 0, dz + 1),
                pos.offset(dx + 1, 0, dz + 1),
            ];
            let blocks = [
                (level.get_block_state(positions[0])?, positions[0]),
                (level.get_block_state(positions[1])?, positions[1]),
                (level.get_block_state(positions[2])?, positions[2]),
                (level.get_block_state(positions[3])?, positions[3]),
            ];
            if blocks
                .iter()
                .all(|(candidate, _)| candidate.block() == state.block())
            {
                return Ok(Some(TwoByTwoSaplingPos {
                    offset_x: dx,
                    offset_z: dz,
                    blocks,
                }));
            }
        }
    }
    Ok(None)
}
