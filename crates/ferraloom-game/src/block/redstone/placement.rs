//! Placement virtuals for the redstone family, sharing the canonical context.
use super::{
    diode,
    state::{HORIZONTAL, RedstoneSide, with_facing, with_side},
    switches, torch, wire,
};
use crate::{
    block::{BlockBehavior, BlockPlaceContext, LevelAccessRef, LevelReaderRef},
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, BlockClass, BlockState, properties as p, registries::fluid::Id as Fluid, tags::BlockTags,
};
use ferraloom_util::{Axis, Direction};

impl Level {
    /// Port of Block#setPlacedBy dispatch. BlockItem's production caller is
    /// supplied by the item-placement port.
    pub fn set_placed_by(
        &mut self,
        state: BlockState,
        pos: ferraloom_util::BlockPos,
        by: Option<crate::entity::EntityRef>,
        stack: crate::block::StackView<'_>,
    ) -> Result<(), LevelError> {
        self.required_block_behavior(state.block(), crate::block::BlockMethod::SetPlacedBy)?
            .set_placed_by(state, pos, by, stack, self)
    }

    /// Port of Block#playerWillDestroy dispatch. Its return value remains
    /// required under both Strict and Warn unported-behavior policies.
    pub fn player_will_destroy(
        &mut self,
        state: BlockState,
        pos: ferraloom_util::BlockPos,
        player: crate::entity::EntityRef,
    ) -> Result<BlockState, LevelError> {
        self.required_block_behavior(state.block(), crate::block::BlockMethod::PlayerWillDestroy)?
            .player_will_destroy(state, pos, player, self)
    }
}

pub struct RedstonePlacement;
pub static PLACEMENT: RedstonePlacement = RedstonePlacement;
pub(super) fn supports(block: Block) -> bool {
    matches!(
        block.class(),
        BlockClass::CrafterBlock
            | BlockClass::RedstoneWireBlock
            | BlockClass::RepeaterBlock
            | BlockClass::ComparatorBlock
            | BlockClass::ObserverBlock
            | BlockClass::RedstoneWallTorchBlock
            | BlockClass::LeverBlock
            | BlockClass::ButtonBlock
            | BlockClass::PistonBaseBlock
            | BlockClass::DispenserBlock
            | BlockClass::DropperBlock
            | BlockClass::HopperBlock
            | BlockClass::RailBlock
            | BlockClass::PoweredRailBlock
            | BlockClass::DetectorRailBlock
            | BlockClass::NoteBlock
            | BlockClass::TripWireBlock
            | BlockClass::TripWireHookBlock
            | BlockClass::RedstoneLampBlock
            | BlockClass::LightningRodBlock
            | BlockClass::WeatheringLightningRodBlock
    ) || block.is_instance_of(BlockClass::DoorBlock)
        || block.is_instance_of(BlockClass::TrapDoorBlock)
        || block.is_instance_of(BlockClass::FenceGateBlock)
}
/// Port of DoorBlock#getHinge. Neighbor solidity is captured before click-side
/// tie breaking, and either neighboring lower door participates regardless of type.
#[allow(clippy::neg_cmp_op_on_partial_ord)] // Native negation includes unordered click coordinates.
fn hinge(
    context: &BlockPlaceContext<'_>,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<p::Hinge, LevelError> {
    let pos = context.clicked_pos();
    let direction = context.horizontal_direction(reader)?;
    let left = pos.relative(direction.counter_clockwise(), 1);
    let right = pos.relative(direction.clockwise(), 1);
    let l = reader.get_block_state(left)?;
    let la = reader.get_block_state(left.above(1))?;
    let r = reader.get_block_state(right)?;
    let ra = reader.get_block_state(right.above(1))?;
    let balance = -i32::from(reader.is_collision_shape_full_block(l, left)?)
        - i32::from(reader.is_collision_shape_full_block(la, left.above(1))?)
        + i32::from(reader.is_collision_shape_full_block(r, right)?)
        + i32::from(reader.is_collision_shape_full_block(ra, right.above(1))?);
    let left_door =
        l.block().is_instance_of(BlockClass::DoorBlock) && l.get(p::HALF) == Some(p::Half::Lower);
    let right_door =
        r.block().is_instance_of(BlockClass::DoorBlock) && r.get(p::HALF) == Some(p::Half::Lower);
    if (left_door && !right_door) || balance > 0 {
        return Ok(p::Hinge::Right);
    }
    if (right_door && !left_door) || balance < 0 {
        return Ok(p::Hinge::Left);
    }
    let click = context.click_location();
    let x = click.x - f64::from(pos.x);
    let z = click.z - f64::from(pos.z);
    let step = direction.step();
    Ok(
        if (step.x >= 0 || !(z < 0.5))
            && (step.x <= 0 || !(z > 0.5))
            && (step.z >= 0 || !(x > 0.5))
            && (step.z <= 0 || !(x < 0.5))
        {
            p::Hinge::Left
        } else {
            p::Hinge::Right
        },
    )
}
impl BlockBehavior for RedstonePlacement {
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        access: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let reader = access.as_reader();
        let pos = context.clicked_pos();
        let state = block.default_state();
        let result = match block.class() {
            BlockClass::CrafterBlock => {
                let direction = context.nearest_looking_direction(&reader)?.opposite();
                let orientation = match direction {
                    Direction::Down => match context.horizontal_direction(&reader)?.opposite() {
                        Direction::East => p::Orientation::DownEast,
                        Direction::North => p::Orientation::DownNorth,
                        Direction::South => p::Orientation::DownSouth,
                        Direction::West => p::Orientation::DownWest,
                        _ => unreachable!(),
                    },
                    Direction::Up => match context.horizontal_direction(&reader)? {
                        Direction::East => p::Orientation::UpEast,
                        Direction::North => p::Orientation::UpNorth,
                        Direction::South => p::Orientation::UpSouth,
                        Direction::West => p::Orientation::UpWest,
                        _ => unreachable!(),
                    },
                    Direction::North => p::Orientation::NorthUp,
                    Direction::South => p::Orientation::SouthUp,
                    Direction::West => p::Orientation::WestUp,
                    Direction::East => p::Orientation::EastUp,
                };
                state
                    .with(p::ORIENTATION, orientation)
                    .unwrap()
                    .with(p::TRIGGERED, reader.has_neighbor_signal(pos)?)
                    .unwrap()
            }
            BlockClass::RedstoneLampBlock => state
                .with(p::LIT, reader.has_neighbor_signal(pos)?)
                .unwrap(),
            BlockClass::LightningRodBlock | BlockClass::WeatheringLightningRodBlock => {
                with_facing(state, context.clicked_face())
                    .with(
                        p::WATERLOGGED,
                        reader.get_block_state(pos)?.data().fluid.fluid_type == Fluid::WATER,
                    )
                    .unwrap()
            }
            BlockClass::TripWireBlock => {
                let mut placed = state;
                for direction in HORIZONTAL {
                    placed = super::tripwire::with_connection(
                        placed,
                        direction,
                        reader.get_block_state(pos.relative(direction, 1))?,
                    );
                }
                placed
            }
            BlockClass::TripWireHookBlock => {
                for direction in context.nearest_looking_directions(&reader)? {
                    if direction.axis() == Axis::Y {
                        continue;
                    }
                    let placed = with_facing(state, direction.opposite());
                    if super::tripwire::HOOK.can_survive(placed, pos, &reader)? {
                        return Ok(Some(placed));
                    }
                }
                return Ok(None);
            }
            BlockClass::NoteBlock => super::note::set_instrument(&reader, pos, state)?,
            BlockClass::RedstoneWireBlock => {
                let cross = HORIZONTAL
                    .into_iter()
                    .fold(state, |s, d| with_side(s, d, RedstoneSide::Side));
                wire::connection_state(&reader, cross, pos)?
            }
            BlockClass::RepeaterBlock | BlockClass::ComparatorBlock => {
                let placed = with_facing(state, context.horizontal_direction(&reader)?.opposite());
                if block.class() == BlockClass::RepeaterBlock {
                    placed
                        .with(p::LOCKED, diode::locked(&reader, pos, placed)?)
                        .unwrap()
                } else {
                    placed
                }
            }
            BlockClass::ObserverBlock => {
                with_facing(state, context.nearest_looking_direction(&reader)?)
            }
            BlockClass::PistonBaseBlock | BlockClass::DispenserBlock | BlockClass::DropperBlock => {
                with_facing(
                    state,
                    context.nearest_looking_direction(&reader)?.opposite(),
                )
            }
            BlockClass::HopperBlock => {
                let direction = context.clicked_face().opposite();
                with_facing(
                    state,
                    if direction.axis() == Axis::Y {
                        Direction::Down
                    } else {
                        direction
                    },
                )
            }
            BlockClass::RailBlock
            | BlockClass::PoweredRailBlock
            | BlockClass::DetectorRailBlock => state
                .with(
                    p::SHAPE,
                    if context.horizontal_direction(&reader)?.axis() == Axis::X {
                        p::Shape::EastWest
                    } else {
                        p::Shape::NorthSouth
                    },
                )
                .unwrap()
                .with(
                    p::WATERLOGGED,
                    reader.get_block_state(pos)?.data().fluid.fluid_type == Fluid::WATER,
                )
                .unwrap(),
            BlockClass::LeverBlock
            | BlockClass::ButtonBlock
            | BlockClass::RedstoneWallTorchBlock => {
                for direction in context.nearest_looking_directions(&reader)? {
                    let placed = if block.class() == BlockClass::RedstoneWallTorchBlock {
                        if direction.axis() == Axis::Y {
                            continue;
                        }
                        with_facing(state, direction.opposite())
                    } else if direction.axis() == Axis::Y {
                        with_facing(
                            state
                                .with(
                                    p::FACE,
                                    if direction == Direction::Up {
                                        p::Face::Ceiling
                                    } else {
                                        p::Face::Floor
                                    },
                                )
                                .unwrap(),
                            context.horizontal_direction(&reader)?,
                        )
                    } else {
                        with_facing(
                            state.with(p::FACE, p::Face::Wall).unwrap(),
                            direction.opposite(),
                        )
                    };
                    let survives = if block.class() == BlockClass::RedstoneWallTorchBlock {
                        torch::TORCH.can_survive(placed, pos, &reader)?
                    } else {
                        switches::SWITCHES.can_survive(placed, pos, &reader)?
                    };
                    if survives {
                        return Ok(Some(placed));
                    }
                }
                return Ok(None);
            }
            _ if block.is_instance_of(BlockClass::DoorBlock) => {
                let max_y = match reader {
                    LevelReaderRef::Server(level) => level.max_build_height(),
                    LevelReaderRef::Worldgen(region) => region.min_y() + region.height() - 1,
                };
                if pos.y >= max_y
                    || !reader.can_be_replaced(reader.get_block_state(pos.above(1))?, context)?
                {
                    return Ok(None);
                }
                let powered =
                    reader.has_neighbor_signal(pos)? || reader.has_neighbor_signal(pos.above(1))?;
                with_facing(state, context.horizontal_direction(&reader)?)
                    .with(p::HINGE, hinge(context, &reader)?)
                    .unwrap()
                    .with(p::POWERED, powered)
                    .unwrap()
                    .with(p::OPEN, powered)
                    .unwrap()
            }
            _ if block.is_instance_of(BlockClass::TrapDoorBlock) => {
                let face = context.clicked_face();
                let (facing, half) =
                    if !context.replacing_clicked_on_block(&reader)? && face.axis() != Axis::Y {
                        (
                            face,
                            if context.click_location().y - f64::from(pos.y) > 0.5 {
                                p::Half::Top
                            } else {
                                p::Half::Bottom
                            },
                        )
                    } else {
                        (
                            context.horizontal_direction(&reader)?.opposite(),
                            if face == Direction::Up {
                                p::Half::Bottom
                            } else {
                                p::Half::Top
                            },
                        )
                    };
                let powered = reader.has_neighbor_signal(pos)?;
                with_facing(state, facing)
                    .with(p::HALF, half)
                    .unwrap()
                    .with(p::OPEN, powered)
                    .unwrap()
                    .with(p::POWERED, powered)
                    .unwrap()
                    .with(
                        p::WATERLOGGED,
                        reader.get_fluid_state(pos)?.fluid_type == Fluid::WATER,
                    )
                    .unwrap()
            }
            _ if block.is_instance_of(BlockClass::FenceGateBlock) => {
                let facing = context.horizontal_direction(&reader)?;
                let perpendicular = facing.clockwise();
                let tags = reader.block_tags()?;
                let wall = tags.contains_key(
                    BlockTags::WALLS,
                    reader
                        .get_block_state(pos.relative(perpendicular, 1))?
                        .block(),
                ) || tags.contains_key(
                    BlockTags::WALLS,
                    reader
                        .get_block_state(pos.relative(perpendicular.opposite(), 1))?
                        .block(),
                );
                let powered = reader.has_neighbor_signal(pos)?;
                with_facing(state, facing)
                    .with(p::OPEN, powered)
                    .unwrap()
                    .with(p::POWERED, powered)
                    .unwrap()
                    .with(p::IN_WALL, wall)
                    .unwrap()
            }
            _ => unreachable!("redstone placement registration"),
        };
        Ok(Some(result))
    }
}
