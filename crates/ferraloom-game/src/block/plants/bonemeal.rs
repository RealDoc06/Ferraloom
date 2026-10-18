//! Port of BonemealableBlock. Item behavior can query this family lookup without
//! storing mutable behavior objects or losing the world's canonical RNG stream.
use super::{
    super::{LevelAccessRef, LevelReaderRef, UpdateFlags},
    crops, multiface_spreader, survival,
};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockClass, BlockState, properties as p};
use ferraloom_util::{BlockPos, core::Plane, random::RandomSource};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BonemealSource {
    Interaction,
    Mob,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BonemealType {
    NeighborSpreader,
    Grower,
}
/// Canonical gameplay calls pass Level#getRandom in vanilla. These methods borrow
/// that stream through LevelAccessRef, allowing mutations to reenter block code.
pub trait BonemealableBlock: Sync {
    fn is_valid_bonemeal_target(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
        source: BonemealSource,
    ) -> Result<bool, LevelError>;
    fn is_bonemeal_success(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &mut LevelAccessRef<'_, '_>,
        _source: BonemealSource,
    ) -> Result<bool, LevelError> {
        Ok(true)
    }
    fn perform_bonemeal(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
        source: BonemealSource,
    ) -> Result<(), LevelError>;
    fn bonemeal_type(&self) -> BonemealType {
        BonemealType::Grower
    }
    fn particle_pos(&self, pos: BlockPos) -> BlockPos {
        match self.bonemeal_type() {
            BonemealType::NeighborSpreader => pos.above(1),
            BonemealType::Grower => pos,
        }
    }
}
pub struct PlantBonemeal;
static PLANT_BONEMEAL: PlantBonemeal = PlantBonemeal;
/// None for an unported class is distinct from not implementing vanilla's
/// interface; callers can test BlockClass::BonemealableBlock before dispatch.
pub fn bonemealable_behavior(block: Block) -> Option<&'static dyn BonemealableBlock> {
    use BlockClass as C;
    matches!(
        block.class(),
        C::CropBlock
            | C::BeetrootBlock
            | C::TorchflowerCropBlock
            | C::TallGrassBlock
            | C::ShortDryGrassBlock
            | C::SweetBerryBushBlock
            | C::BushBlock
            | C::SeagrassBlock
            | C::CaveVinesBlock
            | C::CaveVinesPlantBlock
            | C::GlowLichenBlock
    )
    .then_some(&PLANT_BONEMEAL)
}
fn inside_height(level: &LevelReaderRef<'_, '_>, pos: BlockPos) -> bool {
    match level {
        LevelReaderRef::Server(level) => level
            .chunk(ferraloom_util::ChunkPos::containing(pos))
            .is_some_and(|chunk| pos.y >= chunk.min_y() && pos.y < chunk.max_y()),
        LevelReaderRef::Worldgen(region) => {
            pos.y >= region.min_y() && pos.y < region.min_y() + region.height()
        }
    }
}
fn grown_grass(state: BlockState) -> BlockState {
    if state.block() == Block::FERN {
        Block::LARGE_FERN.default_state()
    } else {
        Block::TALL_GRASS.default_state()
    }
}
impl BonemealableBlock for PlantBonemeal {
    fn is_valid_bonemeal_target(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
        _source: BonemealSource,
    ) -> Result<bool, LevelError> {
        use BlockClass as C;
        match state.block().class() {
            C::CropBlock | C::BeetrootBlock | C::TorchflowerCropBlock => {
                Ok(state.get(p::AGE).unwrap() < crops::max_age(state.block()))
            }
            C::SweetBerryBushBlock => Ok(state.get(p::AGE).unwrap() < 3),
            C::ShortDryGrassBlock => Ok(true),
            C::TallGrassBlock => Ok(level.can_survive(grown_grass(state), pos)?
                && level.get_block_state(pos.above(1))?.is_air()
                && inside_height(level, pos.above(1))),
            C::SeagrassBlock => Ok(level.get_block_state(pos.above(1))?.block() == Block::WATER),
            C::CaveVinesBlock | C::CaveVinesPlantBlock => Ok(state.get(p::BERRIES) == Some(false)),
            C::BushBlock => {
                for &d in Plane::Horizontal.faces() {
                    let at = pos.relative(d, 1);
                    if level.get_block_state(at)?.is_air() && level.can_survive(state, at)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            C::GlowLichenBlock => {
                for from in ferraloom_util::Direction::VALUES {
                    for toward in ferraloom_util::Direction::VALUES {
                        if multiface_spreader::get_spread_from_face_toward_direction(
                            level, state, pos, from, toward,
                        )?
                        .is_some()
                        {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            _ => unreachable!("registered bonemeal class"),
        }
    }
    fn perform_bonemeal(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
        _source: BonemealSource,
    ) -> Result<(), LevelError> {
        use BlockClass as C;
        match state.block().class() {
            C::CropBlock | C::BeetrootBlock | C::TorchflowerCropBlock => {
                crops::grow_crops(state, pos, level)?
            }
            C::SweetBerryBushBlock => {
                level.set_block(
                    pos,
                    state
                        .with(p::AGE, (state.get(p::AGE).unwrap() + 1).min(3))
                        .unwrap(),
                    UpdateFlags::CLIENTS,
                )?;
            }
            C::ShortDryGrassBlock => {
                level.set_block(pos, Block::TALL_DRY_GRASS.default_state(), UpdateFlags::ALL)?;
            }
            C::TallGrassBlock => {
                survival::place_double_plant(level, grown_grass(state), pos, UpdateFlags::CLIENTS)?
            }
            C::SeagrassBlock => survival::place_double_plant(
                level,
                Block::TALL_SEAGRASS.default_state(),
                pos,
                UpdateFlags::CLIENTS,
            )?,
            C::CaveVinesBlock | C::CaveVinesPlantBlock => {
                level.set_block(
                    pos,
                    state.with(p::BERRIES, true).unwrap(),
                    UpdateFlags::CLIENTS,
                )?;
            }
            C::BushBlock => {
                let mut directions = Plane::Horizontal.faces().to_vec();
                for i in (2..=directions.len()).rev() {
                    let j = level.random()?.next_int_bound(i as i32) as usize;
                    directions.swap(i - 1, j);
                }
                for d in directions {
                    let at = pos.relative(d, 1);
                    if level.get_block_state(at)?.is_air()
                        && level.as_reader().can_survive(state, at)?
                    {
                        level.set_block(at, state.block().default_state(), UpdateFlags::ALL)?;
                        break;
                    }
                }
            }
            C::GlowLichenBlock => {
                multiface_spreader::spread_using_level_random(level, state, pos)?;
            }
            _ => unreachable!("registered bonemeal class"),
        }
        Ok(())
    }
}
