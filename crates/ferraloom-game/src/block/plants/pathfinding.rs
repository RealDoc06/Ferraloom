//! Ports of this family's native isPathfindable overrides. Registration lists
//! concrete classes explicitly; unknown overrides never inherit a guessed rule.
use crate::{
    block::{BlockBehavior, BlockMethod, LevelReaderRef},
    entity::ai::pathfinder::PathComputationType,
    level::LevelError,
};
use ferraloom_data::{Block, BlockClass, BlockState, properties as p};

#[derive(Clone, Copy)]
enum Rule {
    Vegetation,
    Never,
    Always,
    Snow,
}
fn rule(class: BlockClass) -> Option<Rule> {
    use BlockClass as C;
    Some(match class {
        // VegetationBlock#isPathfindable.
        C::AttachedStemBlock
        | C::BeetrootBlock
        | C::BushBlock
        | C::CactusFlowerBlock
        | C::CarrotBlock
        | C::CropBlock
        | C::DoublePlantBlock
        | C::DryVegetationBlock
        | C::EyeblossomBlock
        | C::FireflyBushBlock
        | C::FlowerBedBlock
        | C::FlowerBlock
        | C::LeafLitterBlock
        | C::LilyPadBlock
        | C::MangrovePropaguleBlock
        | C::MushroomBlock
        | C::NetherFungusBlock
        | C::NetherRootsBlock
        | C::NetherSproutsBlock
        | C::NetherWartBlock
        | C::PitcherCropBlock
        | C::PotatoBlock
        | C::SaplingBlock
        | C::SeagrassBlock
        | C::ShortDryGrassBlock
        | C::SmallDripleafBlock
        | C::StemBlock
        | C::SweetBerryBushBlock
        | C::TallDryGrassBlock
        | C::TallFlowerBlock
        | C::TallGrassBlock
        | C::TallSeagrassBlock
        | C::TorchflowerCropBlock
        | C::WitherRoseBlock => Rule::Vegetation,
        // AzaleaBlock#isPathfindable, BambooStalkBlock#isPathfindable,
        // CactusBlock#isPathfindable, ChorusPlantBlock#isPathfindable,
        // CocoaBlock#isPathfindable, ComposterBlock#isPathfindable,
        // DriedGhastBlock#isPathfindable, FarmlandBlock#isPathfindable,
        // FlowerPotBlock#isPathfindable, MudBlock#isPathfindable, PathBlock#isPathfindable,
        // SeaPickleBlock#isPathfindable, ShelfMushroomBlock#isPathfindable,
        // SnifferEggBlock#isPathfindable, SoulSandBlock#isPathfindable,
        // SpeleothemBlock#isPathfindable.
        C::AzaleaBlock
        | C::BambooStalkBlock
        | C::CactusBlock
        | C::ChorusPlantBlock
        | C::CocoaBlock
        | C::ComposterBlock
        | C::DriedGhastBlock
        | C::FarmlandBlock
        | C::FlowerPotBlock
        | C::MudBlock
        | C::PathBlock
        | C::PointedDripstoneBlock
        | C::SeaPickleBlock
        | C::ShelfMushroomBlock
        | C::SnifferEggBlock
        | C::SoulSandBlock
        | C::SulfurSpikeBlock => Rule::Never,
        // PowderSnowBlock#isPathfindable.
        C::PowderSnowBlock => Rule::Always,
        // SnowLayerBlock#isPathfindable.
        C::SnowLayerBlock => Rule::Snow,
        _ => return None,
    })
}
pub(super) fn behavior(block: Block) -> Option<&'static dyn BlockBehavior> {
    rule(block.class())?;
    Some(&PATHFINDING)
}
struct Pathfinding;
static PATHFINDING: Pathfinding = Pathfinding;
impl BlockBehavior for Pathfinding {
    fn is_pathfindable(
        &self,
        state: BlockState,
        computation: PathComputationType,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        match rule(state.block().class()).ok_or(LevelError::UnsupportedBlockMethod(
            state.block(),
            BlockMethod::IsPathfindable,
        ))? {
            Rule::Vegetation => {
                if computation == PathComputationType::Air && !state.block().has_collision() {
                    Ok(true)
                } else {
                    super::super::base_is_pathfindable(state, computation, level)
                }
            }
            Rule::Never => Ok(false),
            Rule::Always => Ok(true),
            Rule::Snow => Ok(computation == PathComputationType::Land
                && state
                    .get(p::LAYERS)
                    .expect("SnowLayerBlock LAYERS property")
                    < 5),
        }
    }
}
