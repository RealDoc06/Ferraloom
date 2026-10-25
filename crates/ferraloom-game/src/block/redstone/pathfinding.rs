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
    Never,
    Open,
    Water,
    Trapdoor,
}
fn rule(class: BlockClass) -> Option<Rule> {
    use BlockClass as C;
    Some(match class {
        // AnvilBlock#isPathfindable, ChainBlock#isPathfindable,
        // CrossCollisionBlock#isPathfindable, DragonEggBlock#isPathfindable,
        // EndPortalFrameBlock#isPathfindable, FenceBlock#isPathfindable,
        // GrindstoneBlock#isPathfindable, HeavyCoreBlock#isPathfindable,
        // LanternBlock#isPathfindable, MovingPistonBlock#isPathfindable,
        // PistonBaseBlock#isPathfindable, PistonHeadBlock#isPathfindable,
        // RodBlock#isPathfindable, StairBlock#isPathfindable,
        // StonecutterBlock#isPathfindable, WallBlock#isPathfindable.
        C::AnvilBlock
        | C::ChainBlock
        | C::DragonEggBlock
        | C::EndPortalFrameBlock
        | C::EndRodBlock
        | C::FenceBlock
        | C::GrindstoneBlock
        | C::HeavyCoreBlock
        | C::IronBarsBlock
        | C::LanternBlock
        | C::LightningRodBlock
        | C::MovingPistonBlock
        | C::PistonBaseBlock
        | C::PistonHeadBlock
        | C::StainedGlassPaneBlock
        | C::StairBlock
        | C::StonecutterBlock
        | C::WallBlock
        | C::WeatheringCopperBarsBlock
        | C::WeatheringCopperChainBlock
        | C::WeatheringCopperStairBlock
        | C::WeatheringLanternBlock
        | C::WeatheringLightningRodBlock => Rule::Never,
        // DoorBlock#isPathfindable, FenceGateBlock#isPathfindable.
        C::DoorBlock | C::FenceGateBlock | C::WeatheringCopperDoorBlock => Rule::Open,
        // SlabBlock#isPathfindable.
        C::SlabBlock | C::WeatheringCopperSlabBlock => Rule::Water,
        // TrapDoorBlock#isPathfindable.
        C::TrapDoorBlock | C::WeatheringCopperTrapDoorBlock => Rule::Trapdoor,
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
            Rule::Open => Ok(match computation {
                PathComputationType::Land | PathComputationType::Air => {
                    state.get(p::OPEN).expect("native OPEN property")
                }
                PathComputationType::Water => false,
            }),
            Rule::Water => Ok(computation == PathComputationType::Water
                && level.fluid_tags()?.contains_key(
                    ferraloom_data::tags::FluidTags::WATER,
                    state.data().fluid.fluid_type,
                )),
            Rule::Trapdoor => Ok(match computation {
                PathComputationType::Land | PathComputationType::Air => {
                    state.get(p::OPEN).expect("TrapDoorBlock OPEN property")
                }
                PathComputationType::Water => state
                    .get(p::WATERLOGGED)
                    .expect("TrapDoorBlock WATERLOGGED property"),
            }),
        }
    }
}
