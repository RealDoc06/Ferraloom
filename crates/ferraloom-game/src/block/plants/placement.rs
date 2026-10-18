//! Ports of plant, foliage, ground and snow placement virtuals.
use super::{
    super::{BlockBehavior, BlockMethod, BlockPlaceContext, LevelAccessRef, LevelReaderRef},
    growing, leaves, moss, multiface,
};
use crate::level::LevelError;
use ferraloom_data::{
    Block, BlockClass, BlockState, properties as p,
    tags::{BlockTags, FluidTags},
};
use ferraloom_util::{BlockPos, ChunkPos, Direction, random::RandomSource};

pub struct PlantPlacement;
pub static PLANT_PLACEMENT: PlantPlacement = PlantPlacement;

pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    use BlockClass as C;
    if method == BlockMethod::CanBeReplaced {
        return block.is_instance_of(C::GrowingPlantBodyBlock)
            || block.is_instance_of(C::MultifaceBlock)
            || matches!(
                block.class(),
                C::PitcherCropBlock
                    | C::FlowerBedBlock
                    | C::LeafLitterBlock
                    | C::SnowLayerBlock
                    | C::SeaPickleBlock
            );
    }
    if method != BlockMethod::GetStateForPlacement {
        return false;
    }
    block.is_instance_of(C::GrowingPlantBlock)
        || block.is_instance_of(C::LeavesBlock)
        || block.is_instance_of(C::BaseCoralPlantTypeBlock)
        || block.is_instance_of(C::DoublePlantBlock)
        || block.is_instance_of(C::MultifaceBlock)
        || matches!(
            block.class(),
            C::SnowyBlock
                | C::GrassBlock
                | C::MyceliumBlock
                | C::PathBlock
                | C::FarmlandBlock
                | C::BambooStalkBlock
                | C::MangrovePropaguleBlock
                | C::HangingRootsBlock
                | C::SeaPickleBlock
                | C::SeagrassBlock
                | C::BigDripleafBlock
                | C::MossyCarpetBlock
                | C::FlowerBedBlock
                | C::LeafLitterBlock
                | C::SnowLayerBlock
        )
}

fn facing(direction: Direction) -> p::Facing {
    match direction {
        Direction::North => p::Facing::North,
        Direction::East => p::Facing::East,
        Direction::South => p::Facing::South,
        Direction::West => p::Facing::West,
        _ => unreachable!("horizontal plant placement"),
    }
}
fn full_water(reader: &LevelReaderRef<'_, '_>, pos: BlockPos) -> Result<bool, LevelError> {
    let fluid = reader.get_fluid_state(pos)?;
    Ok(fluid.amount == 8
        && reader
            .fluid_tags()?
            .contains_key(FluidTags::WATER, fluid.fluid_type))
}
fn below_max_y(reader: &LevelReaderRef<'_, '_>, pos: BlockPos) -> Result<bool, LevelError> {
    let max = match reader {
        LevelReaderRef::Server(level) => {
            let chunk_pos = ChunkPos::containing(pos);
            level
                .chunk(chunk_pos)
                .ok_or(LevelError::MissingChunk(chunk_pos))?
                .max_y()
        }
        LevelReaderRef::Worldgen(region) => region.min_y() + region.height(),
    };
    Ok(pos.y < max - 1)
}

impl BlockBehavior for PlantPlacement {
    fn can_be_replaced(
        &self,
        state: BlockState,
        context: &BlockPlaceContext<'_>,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let block = state.block();
        let held = context.item_in_hand(level)?.item();
        if block.class() == BlockClass::PitcherCropBlock {
            return Ok(false);
        }
        if block.is_instance_of(BlockClass::MultifaceBlock) {
            return Ok(held != block.as_item()
                || Direction::VALUES
                    .into_iter()
                    .any(|face| !multiface::has_face(state, face)));
        }
        if block.class() == BlockClass::SnowLayerBlock {
            let layers = state.get(p::LAYERS).unwrap();
            return Ok(if held != block.as_item() || layers >= 8 {
                layers == 1
            } else {
                !context.replacing_clicked_on_block(level)?
                    || context.clicked_face() == Direction::Up
            });
        }
        if matches!(
            block.class(),
            BlockClass::SeaPickleBlock | BlockClass::FlowerBedBlock | BlockClass::LeafLitterBlock
        ) {
            let amount = match block.class() {
                BlockClass::SeaPickleBlock => state.get(p::PICKLES).unwrap(),
                BlockClass::FlowerBedBlock => state.get(p::FLOWER_AMOUNT).unwrap(),
                _ => state.get(p::SEGMENT_AMOUNT).unwrap(),
            };
            if !context.is_secondary_use_active(level)? && held == block.as_item() && amount < 4 {
                return Ok(true);
            }
        }
        let base = super::super::BASE_BLOCK.can_be_replaced(state, context, level)?;
        Ok(base
            && (!block.is_instance_of(BlockClass::GrowingPlantBodyBlock)
                || held != growing::family(block).0.as_item()))
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        use BlockClass as C;
        let pos = context.clicked_pos();
        let reader = level.as_reader();
        let mut state = block.default_state();
        if block.is_instance_of(C::LeavesBlock) {
            state = state
                .with(p::PERSISTENT, true)
                .unwrap()
                .with(
                    p::WATERLOGGED,
                    super::source_water(reader.get_fluid_state(pos)?),
                )
                .unwrap();
            return Ok(Some(leaves::update_distance(state, pos, &reader)?));
        }
        if block.is_instance_of(C::GrowingPlantBlock) {
            if block == Block::KELP && !full_water(&reader, pos)? {
                return Ok(None);
            }
            let (head, body, direction) = growing::family(block);
            let forward = reader.get_block_state(pos.relative(direction, 1))?.block();
            if forward == head || forward == body {
                return Ok(Some(body.default_state()));
            }
            if block == head {
                state = state
                    .with(p::AGE, level.random()?.next_int_bound(25) as u8)
                    .unwrap();
            }
            return Ok(Some(state));
        }
        if block.is_instance_of(C::MultifaceBlock) {
            let old = reader.get_block_state(pos)?;
            for face in context.nearest_looking_directions(&reader)? {
                if let Some(state) = multiface::state_for_placement(&reader, block, old, pos, face)?
                {
                    return Ok(Some(state));
                }
            }
            return Ok(None);
        }
        if block.is_instance_of(C::BaseCoralPlantTypeBlock) {
            state = state
                .with(p::WATERLOGGED, full_water(&reader, pos)?)
                .unwrap();
            if block.is_instance_of(C::BaseCoralWallFanBlock) {
                for direction in context.nearest_looking_directions(&reader)? {
                    if direction.axis().is_horizontal() {
                        state = state.with(p::FACING, facing(direction.opposite())).unwrap();
                        if reader.can_survive(state, pos)? {
                            return Ok(Some(state));
                        }
                    }
                }
                return Ok(None);
            }
            return Ok(Some(state));
        }
        if block.is_instance_of(C::DoublePlantBlock) && block != Block::PITCHER_CROP {
            if !below_max_y(&reader, pos)?
                || !reader.can_be_replaced(reader.get_block_state(pos.above(1))?, context)?
            {
                return Ok(None);
            }
            if block == Block::TALL_SEAGRASS && !full_water(&reader, pos.above(1))? {
                return Ok(None);
            }
            if block == Block::SMALL_DRIPLEAF {
                state = state
                    .with(
                        p::FACING,
                        facing(context.horizontal_direction(&reader)?.opposite()),
                    )
                    .unwrap()
                    .with(
                        p::WATERLOGGED,
                        reader.fluid_tags()?.contains_key(
                            FluidTags::WATER,
                            reader.get_fluid_state(pos)?.fluid_type,
                        ),
                    )
                    .unwrap();
            }
            return Ok(Some(state));
        }
        match block.class() {
            C::PitcherCropBlock => {}
            C::HangingRootsBlock => {
                state = state
                    .with(
                        p::WATERLOGGED,
                        super::source_water(reader.get_fluid_state(pos)?),
                    )
                    .unwrap();
            }
            C::MangrovePropaguleBlock => {
                state = state
                    .with(
                        p::WATERLOGGED,
                        super::source_water(reader.get_fluid_state(pos)?),
                    )
                    .unwrap()
                    .with(p::AGE, 4)
                    .unwrap();
            }
            C::BambooStalkBlock => {
                if !reader.get_fluid_state(pos)?.empty {
                    return Ok(None);
                }
                let below = reader.get_block_state(pos.below(1))?;
                if !reader
                    .block_tags()?
                    .contains_key(BlockTags::SUPPORTS_BAMBOO, below.block())
                {
                    return Ok(None);
                }
                state = match below.block() {
                    Block::BAMBOO_SAPLING => state.with(p::AGE, 0).unwrap(),
                    Block::BAMBOO => state
                        .with(p::AGE, u8::from(below.get(p::AGE).unwrap() > 0))
                        .unwrap(),
                    _ => {
                        let above = reader.get_block_state(pos.above(1))?;
                        if above.block() == Block::BAMBOO {
                            state.with(p::AGE, above.get(p::AGE).unwrap()).unwrap()
                        } else {
                            Block::BAMBOO_SAPLING.default_state()
                        }
                    }
                };
            }
            C::SeagrassBlock => {
                if !full_water(&reader, pos)? {
                    return Ok(None);
                }
            }
            C::SeaPickleBlock => {
                let old = reader.get_block_state(pos)?;
                state = if old.block() == block {
                    old.with(p::PICKLES, (old.get(p::PICKLES).unwrap() + 1).min(4))
                        .unwrap()
                } else {
                    state
                        .with(
                            p::WATERLOGGED,
                            super::source_water(reader.get_fluid_state(pos)?),
                        )
                        .unwrap()
                };
            }
            C::SnowLayerBlock => {
                let old = reader.get_block_state(pos)?;
                if old.block() == block {
                    state = old
                        .with(p::LAYERS, (old.get(p::LAYERS).unwrap() + 1).min(8))
                        .unwrap();
                }
            }
            C::SnowyBlock | C::GrassBlock | C::MyceliumBlock => {
                state = state
                    .with(
                        p::SNOWY,
                        reader.block_tags()?.contains_key(
                            BlockTags::SNOW,
                            reader.get_block_state(pos.above(1))?.block(),
                        ),
                    )
                    .unwrap();
            }
            C::FarmlandBlock => {
                if !reader.can_survive(state, pos)? {
                    state = Block::DIRT.default_state();
                }
            }
            C::PathBlock => {
                if !reader.can_survive(state, pos)? {
                    state = match level {
                        LevelAccessRef::Server(server) => {
                            server.push_entities_up(state, Block::DIRT.default_state(), pos)?
                        }
                        LevelAccessRef::Worldgen(_) => {
                            return Err(LevelError::UnsupportedPushEntitiesUp);
                        }
                    };
                }
            }
            C::BigDripleafBlock => {
                let below = reader.get_block_state(pos.below(1))?;
                state = state
                    .with(
                        p::WATERLOGGED,
                        super::source_water(reader.get_fluid_state(pos)?),
                    )
                    .unwrap()
                    .with(
                        p::FACING,
                        if matches!(
                            below.block(),
                            Block::BIG_DRIPLEAF | Block::BIG_DRIPLEAF_STEM
                        ) {
                            below.get(p::FACING).unwrap()
                        } else {
                            facing(context.horizontal_direction(&reader)?.opposite())
                        },
                    )
                    .unwrap();
            }
            C::FlowerBedBlock | C::LeafLitterBlock => {
                let old = reader.get_block_state(pos)?;
                if old.block() == block {
                    state = if block.class() == C::FlowerBedBlock {
                        old.with(
                            p::FLOWER_AMOUNT,
                            (old.get(p::FLOWER_AMOUNT).unwrap() + 1).min(4),
                        )
                        .unwrap()
                    } else {
                        old.with(
                            p::SEGMENT_AMOUNT,
                            (old.get(p::SEGMENT_AMOUNT).unwrap() + 1).min(4),
                        )
                        .unwrap()
                    };
                } else {
                    state = state
                        .with(
                            p::FACING,
                            facing(context.horizontal_direction(&reader)?.opposite()),
                        )
                        .unwrap();
                }
            }
            C::MossyCarpetBlock => state = moss::updated(state, pos, &reader, true)?,
            _ => unreachable!("registered plant placement"),
        }
        Ok(Some(state))
    }
}
