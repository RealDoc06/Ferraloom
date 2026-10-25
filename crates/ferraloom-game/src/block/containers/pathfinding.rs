//! Ports of this family's native isPathfindable overrides. Registration lists
//! concrete classes explicitly; unknown overrides never inherit a guessed rule.
use crate::{
    block::{BlockBehavior, BlockMethod, LevelReaderRef},
    entity::ai::pathfinder::PathComputationType,
    level::LevelError,
};
use ferraloom_data::{Block, BlockClass, BlockState};

#[derive(Clone, Copy)]
enum Rule {
    Never,
    Water,
}
fn rule(class: BlockClass) -> Option<Rule> {
    use BlockClass as C;
    Some(match class {
        // AbstractBedBlock#isPathfindable, AbstractCauldronBlock#isPathfindable,
        // AbstractSkullBlock#isPathfindable, BellBlock#isPathfindable,
        // BrewingStandBlock#isPathfindable, CakeBlock#isPathfindable,
        // CampfireBlock#isPathfindable, CandleCakeBlock#isPathfindable,
        // ChestBlock#isPathfindable, ConduitBlock#isPathfindable,
        // DecoratedPotBlock#isPathfindable, EnchantingTableBlock#isPathfindable,
        // EnderChestBlock#isPathfindable, HopperBlock#isPathfindable,
        // LecternBlock#isPathfindable, RespawnAnchorBlock#isPathfindable,
        // SculkSensorBlock#isPathfindable, WallHangingSignBlock#isPathfindable.
        C::BedBlock
        | C::BellBlock
        | C::BrewingStandBlock
        | C::CakeBlock
        | C::CalibratedSculkSensorBlock
        | C::CampfireBlock
        | C::CandleCakeBlock
        | C::CauldronBlock
        | C::ChestBlock
        | C::ConduitBlock
        | C::CopperChestBlock
        | C::DecoratedPotBlock
        | C::EnchantingTableBlock
        | C::EnderChestBlock
        | C::HopperBlock
        | C::LavaCauldronBlock
        | C::LayeredCauldronBlock
        | C::LecternBlock
        | C::PiglinWallSkullBlock
        | C::PlayerHeadBlock
        | C::PlayerWallHeadBlock
        | C::RespawnAnchorBlock
        | C::SculkSensorBlock
        | C::SkullBlock
        | C::StrawBedBlock
        | C::TrappedChestBlock
        | C::WallHangingSignBlock
        | C::WallSkullBlock
        | C::WeatheringCopperChestBlock
        | C::WitherSkullBlock
        | C::WitherWallSkullBlock => Rule::Never,
        // CopperGolemStatueBlock#isPathfindable, ShelfBlock#isPathfindable.
        C::CopperGolemStatueBlock | C::ShelfBlock | C::WeatheringCopperGolemStatueBlock => {
            Rule::Water
        }
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
            Rule::Never => Ok(false),
            Rule::Water => Ok(computation == PathComputationType::Water
                && level.fluid_tags()?.contains_key(
                    ferraloom_data::tags::FluidTags::WATER,
                    state.data().fluid.fluid_type,
                )),
        }
    }
}
