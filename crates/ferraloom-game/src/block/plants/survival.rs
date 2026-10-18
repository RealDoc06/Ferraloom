//! Ports of VegetationBlock and its support-specializing subclasses.
use super::super::{BlockBehavior, BlockMethod, LevelAccessRef, LevelReaderRef, UpdateFlags};
use crate::level::{Level, LevelError};
use ferraloom_data::{
    Block, BlockClass, BlockState, properties as p,
    tags::{BlockTags as T, FluidTags as F},
};
use ferraloom_util::{BlockPos, Direction, core::Plane, shapes};

pub struct Plant;
pub static PLANT: Plant = Plant;

pub fn supports(block: Block, method: BlockMethod) -> bool {
    use BlockClass as C;
    use BlockMethod as M;
    if block == Block::MOSS_CARPET
        && matches!(
            method,
            M::CanSurvive | M::UpdateShape | M::UpdateFromNeighbourShapes
        )
    {
        return true;
    }
    let survival = matches!(
        block.class(),
        C::FlowerBlock
            | C::EyeblossomBlock
            | C::WitherRoseBlock
            | C::TallGrassBlock
            | C::BushBlock
            | C::DryVegetationBlock
            | C::ShortDryGrassBlock
            | C::TallDryGrassBlock
            | C::SaplingBlock
            | C::AzaleaBlock
            | C::MangrovePropaguleBlock
            | C::CropBlock
            | C::BeetrootBlock
            | C::TorchflowerCropBlock
            | C::PitcherCropBlock
            | C::StemBlock
            | C::AttachedStemBlock
            | C::SweetBerryBushBlock
            | C::NetherWartBlock
            | C::MushroomBlock
            | C::NetherFungusBlock
            | C::NetherRootsBlock
            | C::NetherSproutsBlock
            | C::LilyPadBlock
            | C::CactusBlock
            | C::CactusFlowerBlock
            | C::SugarCaneBlock
            | C::BambooStalkBlock
            | C::BambooSaplingBlock
            | C::SeagrassBlock
            | C::TallSeagrassBlock
            | C::SeaPickleBlock
            | C::DoublePlantBlock
            | C::TallFlowerBlock
            | C::HangingRootsBlock
            | C::SporeBlossomBlock
            | C::BigDripleafBlock
            | C::BigDripleafStemBlock
            | C::SmallDripleafBlock
            | C::FlowerBedBlock
            | C::LeafLitterBlock
            | C::FireflyBushBlock
            | C::SnowLayerBlock
            | C::BaseCoralPlantBlock
            | C::BaseCoralFanBlock
            | C::BaseCoralWallFanBlock
    );
    if matches!(
        method,
        M::CanSurvive | M::UpdateShape | M::UpdateFromNeighbourShapes
    ) {
        return survival;
    }
    if method == M::Tick {
        return matches!(
            block.class(),
            C::CactusBlock | C::SugarCaneBlock | C::BambooStalkBlock | C::BigDripleafStemBlock
        );
    }
    false
}
fn tagged(
    level: &LevelReaderRef<'_, '_>,
    state: BlockState,
    tag: ferraloom_data::datapack::TagKey<Block>,
) -> Result<bool, LevelError> {
    Ok(level.block_tags()?.contains_key(tag, state.block()))
}
fn water(level: &LevelReaderRef<'_, '_>, pos: BlockPos) -> Result<bool, LevelError> {
    Ok(level
        .fluid_tags()?
        .contains_key(F::WATER, level.get_fluid_state(pos)?.fluid_type))
}
pub(super) fn facing(state: BlockState) -> Direction {
    match state.get(p::FACING).expect("horizontal facing") {
        p::Facing::North => Direction::North,
        p::Facing::South => Direction::South,
        p::Facing::East => Direction::East,
        p::Facing::West => Direction::West,
        _ => unreachable!("plant horizontal facing"),
    }
}
fn double_plant(block: Block) -> bool {
    block.is_instance_of(BlockClass::DoublePlantBlock)
}
fn upper(state: BlockState) -> bool {
    state.get(p::HALF) == Some(p::Half::Upper)
}

/// Port of VegetationBlock#mayPlaceOn, preserving each subclass's support tags.
fn may_place_on(
    state: BlockState,
    below: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    use BlockClass as C;
    let tag = match state.block().class() {
        C::DryVegetationBlock | C::ShortDryGrassBlock | C::TallDryGrassBlock => {
            T::SUPPORTS_DRY_VEGETATION
        }
        C::AzaleaBlock => T::SUPPORTS_AZALEA,
        C::MangrovePropaguleBlock => T::SUPPORTS_MANGROVE_PROPAGULE,
        C::CropBlock | C::BeetrootBlock | C::TorchflowerCropBlock | C::PitcherCropBlock => {
            T::SUPPORTS_CROPS
        }
        C::StemBlock | C::AttachedStemBlock => {
            if matches!(
                state.block(),
                Block::MELON_STEM | Block::ATTACHED_MELON_STEM
            ) {
                T::SUPPORTS_MELON_STEM
            } else {
                T::SUPPORTS_PUMPKIN_STEM
            }
        }
        C::NetherWartBlock => T::SUPPORTS_NETHER_WART,
        C::WitherRoseBlock => T::SUPPORTS_WITHER_ROSE,
        C::NetherSproutsBlock => T::SUPPORTS_NETHER_SPROUTS,
        C::NetherRootsBlock => {
            if state.block() == Block::CRIMSON_ROOTS {
                T::SUPPORTS_CRIMSON_ROOTS
            } else {
                T::SUPPORTS_WARPED_ROOTS
            }
        }
        C::NetherFungusBlock => {
            if state.block() == Block::CRIMSON_FUNGUS {
                T::SUPPORTS_CRIMSON_FUNGUS
            } else {
                T::SUPPORTS_WARPED_FUNGUS
            }
        }
        C::SeagrassBlock | C::TallSeagrassBlock => {
            return Ok(level.is_face_sturdy(below, pos, Direction::Up)?
                && !tagged(level, below, T::CANNOT_SUPPORT_SEAGRASS)?);
        }
        C::SeaPickleBlock => {
            return Ok(!level
                .collision_shape_without_context(below, pos)?
                .get_face_shape(Direction::Up)
                .is_empty()
                || level.is_face_sturdy(below, pos, Direction::Up)?);
        }
        C::CactusFlowerBlock => {
            return Ok(tagged(level, below, T::SUPPORT_OVERRIDE_CACTUS_FLOWER)?
                || level.can_support_center(pos, Direction::Up)?);
        }
        C::LilyPadBlock => {
            return Ok((level
                .fluid_tags()?
                .contains_key(F::SUPPORTS_LILY_PAD, below.data().fluid.fluid_type)
                || tagged(level, below, T::SUPPORTS_LILY_PAD)?)
                && level.get_fluid_state(pos.above(1))?.empty);
        }
        C::SmallDripleafBlock => {
            return Ok(tagged(level, below, T::SUPPORTS_SMALL_DRIPLEAF)?
                || (super::source_water(level.get_fluid_state(pos.above(1))?)
                    && tagged(level, below, T::SUPPORTS_VEGETATION)?));
        }
        _ => T::SUPPORTS_VEGETATION,
    };
    tagged(level, below, tag)
}
impl BlockBehavior for Plant {
    /// Port of the family's canSurvive virtuals, using the same reader in worldgen.
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        use BlockClass as C;
        let block = state.block();
        let below_pos = pos.below(1);
        if double_plant(block) && upper(state) {
            let below = level.get_block_state(below_pos)?;
            return Ok(below.block() == block && !upper(below));
        }
        match block.class() {
            C::CarpetBlock => Ok(!level.get_block_state(below_pos)?.is_air()),
            C::MangrovePropaguleBlock if state.get(p::HANGING) == Some(true) => tagged(
                level,
                level.get_block_state(pos.above(1))?,
                T::SUPPORTS_HANGING_MANGROVE_PROPAGULE,
            ),
            C::HangingRootsBlock => level.is_face_sturdy(
                level.get_block_state(pos.above(1))?,
                pos.above(1),
                Direction::Down,
            ),
            C::SporeBlossomBlock => {
                Ok(level.can_support_center(pos.above(1), Direction::Down)? && !water(level, pos)?)
            }
            C::LeafLitterBlock => {
                level.is_face_sturdy(level.get_block_state(below_pos)?, below_pos, Direction::Up)
            }
            C::BambooStalkBlock | C::BambooSaplingBlock => {
                tagged(level, level.get_block_state(below_pos)?, T::SUPPORTS_BAMBOO)
            }
            C::MushroomBlock => {
                let below = level.get_block_state(below_pos)?;
                Ok(
                    tagged(level, below, T::OVERRIDES_MUSHROOM_LIGHT_REQUIREMENT)?
                        || (level.get_raw_brightness(pos, 0)? < 13 && below.data().solid_render),
                )
            }
            C::CropBlock | C::BeetrootBlock | C::TorchflowerCropBlock | C::PitcherCropBlock => {
                if !(level.get_raw_brightness(pos, 0)? >= 8) {
                    return Ok(false);
                }
                may_place_on(state, level.get_block_state(below_pos)?, below_pos, level)
            }
            C::TallSeagrassBlock => {
                Ok(
                    may_place_on(state, level.get_block_state(below_pos)?, below_pos, level)?
                        && water(level, pos)?
                        && level.get_fluid_state(pos)?.amount == 8,
                )
            }
            C::CactusBlock => {
                for d in Plane::Horizontal.faces().iter().copied() {
                    let neighbor = level.get_block_state(pos.relative(d, 1))?;
                    if neighbor.is_solid()
                        || level
                            .fluid_tags()?
                            .contains_key(F::LAVA, neighbor.data().fluid.fluid_type)
                    {
                        return Ok(false);
                    }
                }
                let below = level.get_block_state(below_pos)?;
                Ok(
                    (below.block() == block || tagged(level, below, T::SUPPORTS_CACTUS)?)
                        && !level.get_block_state(pos.above(1))?.liquid(),
                )
            }
            C::SugarCaneBlock => {
                let below = level.get_block_state(below_pos)?;
                if below.block() == block {
                    return Ok(true);
                }
                if tagged(level, below, T::SUPPORTS_SUGAR_CANE)? {
                    for d in Plane::Horizontal.faces().iter().copied() {
                        let neighbor = level.get_block_state(below_pos.relative(d, 1))?;
                        if level.fluid_tags()?.contains_key(
                            F::SUPPORTS_SUGAR_CANE_ADJACENTLY,
                            neighbor.data().fluid.fluid_type,
                        ) || tagged(level, neighbor, T::SUPPORTS_SUGAR_CANE_ADJACENTLY)?
                        {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            C::BigDripleafBlock => {
                let below = level.get_block_state(below_pos)?;
                Ok(matches!(
                    below.block(),
                    Block::BIG_DRIPLEAF | Block::BIG_DRIPLEAF_STEM
                ) || tagged(level, below, T::SUPPORTS_BIG_DRIPLEAF)?)
            }
            C::BigDripleafStemBlock => {
                let below = level.get_block_state(below_pos)?;
                Ok(
                    (below.block() == block || tagged(level, below, T::SUPPORTS_BIG_DRIPLEAF)?)
                        && matches!(
                            level.get_block_state(pos.above(1))?.block(),
                            Block::BIG_DRIPLEAF | Block::BIG_DRIPLEAF_STEM
                        ),
                )
            }
            C::SnowLayerBlock => {
                let below = level.get_block_state(below_pos)?;
                if tagged(level, below, T::CANNOT_SUPPORT_SNOW_LAYER)? {
                    return Ok(false);
                }
                Ok(tagged(level, below, T::SUPPORT_OVERRIDE_SNOW_LAYER)?
                    || shapes::is_face_full(
                        &level.collision_shape_without_context(below, below_pos)?,
                        Direction::Up,
                    )
                    || (below.block() == block && below.get(p::LAYERS) == Some(8)))
            }
            C::BaseCoralWallFanBlock | C::CoralWallFanBlock => {
                let d = facing(state);
                let support = pos.relative(d.opposite(), 1);
                level.is_face_sturdy(level.get_block_state(support)?, support, d)
            }
            C::BaseCoralPlantBlock
            | C::BaseCoralFanBlock
            | C::CoralPlantBlock
            | C::CoralFanBlock => {
                level.is_face_sturdy(level.get_block_state(below_pos)?, below_pos, Direction::Up)
            }
            _ => may_place_on(state, level.get_block_state(below_pos)?, below_pos, level),
        }
    }
    /// Port of the family's updateShape virtuals, including deferred support loss.
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        use BlockClass as C;
        let block = state.block();
        let air = Block::AIR.default_state();
        match block.class() {
            C::CactusBlock | C::SugarCaneBlock | C::BambooStalkBlock | C::BigDripleafStemBlock => {
                if (block.class() != C::BigDripleafStemBlock || direction.axis().is_vertical())
                    && !self.can_survive(state, pos, &level.as_reader())?
                {
                    level.schedule_block_tick(pos, block, 1)?;
                }
                schedule_water(state, pos, level)?;
                if block.class() == C::BambooStalkBlock
                    && direction == Direction::Up
                    && neighbor.block() == Block::BAMBOO
                    && neighbor.get(p::AGE) > state.get(p::AGE)
                {
                    return Ok(state.with(p::AGE, 1).unwrap());
                }
                return Ok(state);
            }
            C::BambooSaplingBlock => {
                if !self.can_survive(state, pos, &level.as_reader())? {
                    return Ok(air);
                }
                return Ok(
                    if direction == Direction::Up && neighbor.block() == Block::BAMBOO {
                        Block::BAMBOO.default_state()
                    } else {
                        state
                    },
                );
            }
            C::HangingRootsBlock | C::SporeBlossomBlock => {
                if direction == Direction::Up
                    && !self.can_survive(state, pos, &level.as_reader())?
                {
                    return Ok(air);
                }
                schedule_water(state, pos, level)?;
                return Ok(state);
            }
            C::BigDripleafBlock => {
                if direction == Direction::Down
                    && !self.can_survive(state, pos, &level.as_reader())?
                {
                    return Ok(air);
                }
                schedule_water(state, pos, level)?;
                return Ok(if direction == Direction::Up && neighbor.block() == block {
                    Block::BIG_DRIPLEAF_STEM
                        .default_state()
                        .with_properties_of(state)
                } else {
                    state
                });
            }
            C::BaseCoralPlantBlock
            | C::BaseCoralFanBlock
            | C::BaseCoralWallFanBlock
            | C::CoralPlantBlock
            | C::CoralFanBlock
            | C::CoralWallFanBlock => {
                schedule_water(state, pos, level)?;
                let check = if matches!(
                    block.class(),
                    C::BaseCoralWallFanBlock | C::CoralWallFanBlock
                ) {
                    direction.opposite() == facing(state)
                } else {
                    direction == Direction::Down
                };
                return Ok(
                    if check && !self.can_survive(state, pos, &level.as_reader())? {
                        air
                    } else {
                        state
                    },
                );
            }
            C::AttachedStemBlock => {
                let (fruit, stem) = if block == Block::ATTACHED_MELON_STEM {
                    (Block::MELON, Block::MELON_STEM)
                } else {
                    (Block::PUMPKIN, Block::PUMPKIN_STEM)
                };
                if direction == facing(state) && neighbor.block() != fruit {
                    return Ok(stem.default_state().with(p::AGE, 7).unwrap());
                }
            }
            _ => {}
        }
        if matches!(
            block.class(),
            C::MangrovePropaguleBlock | C::SmallDripleafBlock
        ) {
            schedule_water(state, pos, level)?;
        }
        if double_plant(block)
            && !(block.class() == C::PitcherCropBlock && state.get(p::AGE).unwrap() < 3)
        {
            let toward_other = if upper(state) {
                Direction::Down
            } else {
                Direction::Up
            };
            if direction == toward_other
                && (neighbor.block() != block || upper(neighbor) == upper(state))
            {
                return Ok(air);
            }
        }
        if !self.can_survive(state, pos, &level.as_reader())? {
            return Ok(air);
        }
        if block.class() == C::SeagrassBlock {
            level.schedule_fluid_tick(pos, ferraloom_data::registries::fluid::Id::WATER, 5)?;
        }
        if block.class() == C::SeaPickleBlock {
            schedule_water(state, pos, level)?;
        }
        Ok(state)
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if !self.can_survive(state, pos, &LevelReaderRef::Server(level))? {
            level.destroy_block(pos, true, super::super::UPDATE_LIMIT)?;
        }
        Ok(())
    }
}
pub(super) fn schedule_water(
    state: BlockState,
    pos: BlockPos,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    if state.get(p::WATERLOGGED) == Some(true) {
        level.schedule_fluid_tick(pos, ferraloom_data::registries::fluid::Id::WATER, 5)?;
    }
    Ok(())
}
/// Port of DoublePlantBlock#placeAt, copying each location's water independently.
pub fn place_double_plant(
    level: &mut LevelAccessRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
    flags: UpdateFlags,
) -> Result<(), LevelError> {
    for (at, half) in [(pos, p::Half::Lower), (pos.above(1), p::Half::Upper)] {
        let mut next = state.with(p::HALF, half).expect("double plant half");
        if next.get(p::WATERLOGGED).is_some() {
            next = next
                .with(p::WATERLOGGED, water(&level.as_reader(), at)?)
                .unwrap();
        }
        level.set_block(at, next, flags)?;
    }
    Ok(())
}
