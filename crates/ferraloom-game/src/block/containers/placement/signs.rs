//! Banner, sign and skull placement, with native support and hanging-chain reads.
use super::{direction, facing};
use crate::{
    block::{BlockBehavior, BlockMethod, BlockPlaceContext, LevelAccessRef, LevelReaderRef},
    level::LevelError,
};
use ferraloom_data::{
    Block, BlockClass as C, BlockState, properties as p, registries::fluid, tags::BlockTags,
};
use ferraloom_util::{BlockPos, Direction, shapes};

pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    if method == BlockMethod::CanSurvive {
        return matches!(
            block.class(),
            C::BannerBlock
                | C::WallBannerBlock
                | C::StandingSignBlock
                | C::WallSignBlock
                | C::CeilingHangingSignBlock
        );
    }
    if matches!(
        method,
        BlockMethod::UpdateShape | BlockMethod::UpdateFromNeighbourShapes
    ) {
        return block.is_instance_of(C::SignBlock)
            || matches!(block.class(), C::BannerBlock | C::WallBannerBlock);
    }
    method == BlockMethod::GetStateForPlacement
        && (block.is_instance_of(C::SkullBlock)
            || block.is_instance_of(C::WallSkullBlock)
            || matches!(
                block.class(),
                C::BannerBlock
                    | C::WallBannerBlock
                    | C::StandingSignBlock
                    | C::WallSignBlock
                    | C::CeilingHangingSignBlock
                    | C::WallHangingSignBlock
            ))
}
/// Port of RotationSegment#convertToSegment(float): the multiplication is f32,
/// Math.round uses ties towards positive infinity and saturating int conversion.
fn rotation_segment(degrees: f32) -> u8 {
    (((f64::from(degrees * (16.0_f32 / 360.0_f32)) + 0.5).floor() as i32) & 15) as u8
}
/// Ports StandingSignBlock/BannerBlock's below.isSolid and the wall variants'
/// legacy-solid backing test. CeilingHangingSignBlock uses SupportType.CENTER,
/// without Block#canSupportCenter's extra UNSTABLE_BOTTOM_CENTER tag check.
pub(super) fn can_survive(
    state: BlockState,
    pos: BlockPos,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    if matches!(state.block().class(), C::StandingSignBlock | C::BannerBlock) {
        return Ok(reader.get_block_state(pos.below(1))?.is_solid());
    }
    if state.block().class() == C::CeilingHangingSignBlock {
        let above = pos.above(1);
        let support = reader.get_block_state(above)?;
        if let Some(sturdy) = support.cached_center_support(Direction::Down) {
            return Ok(sturdy);
        }
        let shape = reader
            .required_behavior(support.block(), BlockMethod::IsFaceSturdy)?
            .support_shape(support, above, reader)?;
        let center = shapes::create(
            7.0 / 16.0,
            0.0,
            7.0 / 16.0,
            9.0 / 16.0,
            10.0 / 16.0,
            9.0 / 16.0,
        );
        return Ok(!shapes::join_is_not_empty(
            &shape.get_face_shape(Direction::Down),
            &center,
            shapes::BooleanOp::OnlySecond,
        ));
    }
    let back = direction(state.get(p::FACING).unwrap()).opposite();
    Ok(reader.get_block_state(pos.relative(back, 1))?.is_solid())
}
/// Port of WallHangingSignBlock#canPlace/canAttachTo. Chained signs match axes;
/// other supports require a full sturdy face, in clockwise-first query order.
pub fn wall_hanging_sign_can_place(
    state: BlockState,
    reader: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
) -> Result<bool, LevelError> {
    let forward = direction(state.get(p::FACING).unwrap());
    for side in [forward.clockwise(), forward.counter_clockwise()] {
        let attach_pos = pos.relative(side, 1);
        let attach = reader.get_block_state(attach_pos)?;
        let accepted = if reader
            .block_tags()?
            .contains_key(BlockTags::WALL_HANGING_SIGNS, attach.block())
        {
            direction(attach.get(p::FACING).unwrap()).axis() == forward.axis()
        } else {
            reader.is_face_sturdy(attach, attach_pos, side.opposite())?
        };
        if accepted {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Port of CeilingHangingSignBlock#getStateForPlacement, including the native
/// collision-face query before the secondary-use and aligned-chain overrides.
fn ceiling(
    block: Block,
    context: &BlockPlaceContext<'_>,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    let waterlogged = reader.get_fluid_state(context.clicked_pos())?.fluid_type == fluid::Id::WATER;
    let above = context.clicked_pos().above(1);
    let support = reader.get_block_state(above)?;
    let below_sign = reader
        .block_tags()?
        .contains_key(BlockTags::ALL_HANGING_SIGNS, support.block());
    let forward = Direction::from_y_rot(f64::from(context.rotation(reader)?));
    let shape = reader.collision_shape_without_context(support, above)?;
    let mut attached = !shapes::is_face_full(&shape, Direction::Down)
        || context.is_secondary_use_active(reader)?;
    if below_sign && !context.is_secondary_use_active(reader)? {
        // hasProperty compares the native property domain, not just its name.
        let axis = if support.property_values("facing")
            == Block::OAK_WALL_HANGING_SIGN
                .default_state()
                .property_values("facing")
        {
            Some(direction(support.get(p::FACING).unwrap()).axis())
        } else if support.property_values("rotation")
            == block.default_state().property_values("rotation")
        {
            match support.get(p::ROTATION) {
                Some(0 | 8) => Some(ferraloom_util::Axis::Z),
                Some(4 | 12) => Some(ferraloom_util::Axis::X),
                _ => None,
            }
        } else {
            None
        };
        if axis == Some(forward.axis()) {
            attached = false;
        }
    }
    let rotation = if attached {
        rotation_segment(context.rotation(reader)? + 180.0_f32)
    } else {
        (forward.opposite().data_2d() << 2) as u8
    };
    Ok(block
        .default_state()
        .with(p::ATTACHED, attached)
        .unwrap()
        .with(p::ROTATION, rotation)
        .unwrap()
        .with(p::WATERLOGGED, waterlogged)
        .unwrap())
}
/// Ports the sign/banner/skull getStateForPlacement overrides. Wall selection
/// keeps the original context's looking order and stops at the first support.
pub(super) fn get_state_for_placement(
    block: Block,
    context: &BlockPlaceContext<'_>,
    reader: &LevelReaderRef<'_, '_>,
) -> Result<Option<BlockState>, LevelError> {
    let state = block.default_state();
    if block.class() == C::CeilingHangingSignBlock {
        return Ok(Some(ceiling(block, context, reader)?));
    }
    if block.class() == C::BannerBlock {
        return Ok(Some(
            state
                .with(
                    p::ROTATION,
                    rotation_segment(context.rotation(reader)? + 180.0_f32),
                )
                .unwrap(),
        ));
    }
    if block.class() == C::StandingSignBlock {
        let waterlogged =
            reader.get_fluid_state(context.clicked_pos())?.fluid_type == fluid::Id::WATER;
        return Ok(Some(
            state
                .with(
                    p::ROTATION,
                    rotation_segment(context.rotation(reader)? + 180.0_f32),
                )
                .unwrap()
                .with(p::WATERLOGGED, waterlogged)
                .unwrap(),
        ));
    }
    let skull = block.is_instance_of(C::AbstractSkullBlock);
    let state = if skull {
        state
            .with(
                p::POWERED,
                reader.has_neighbor_signal(context.clicked_pos())?,
            )
            .unwrap()
    } else {
        state
    };
    if block.is_instance_of(C::SkullBlock) {
        return Ok(Some(
            state
                .with(p::ROTATION, rotation_segment(context.rotation(reader)?))
                .unwrap(),
        ));
    }
    let waterlogged = if matches!(block.class(), C::WallSignBlock | C::WallHangingSignBlock) {
        reader.get_fluid_state(context.clicked_pos())?.fluid_type == fluid::Id::WATER
    } else {
        false
    };
    for side in context.nearest_looking_directions(reader)? {
        if !side.axis().is_horizontal()
            || (block.class() == C::WallHangingSignBlock
                && side.axis() == context.clicked_face().axis())
        {
            continue;
        }
        let placed = state.with(p::FACING, facing(side.opposite())).unwrap();
        let survives = if block.is_instance_of(C::WallSkullBlock) {
            !reader.can_be_replaced(
                reader.get_block_state(context.clicked_pos().relative(side, 1))?,
                context,
            )?
        } else {
            reader.can_survive(placed, context.clicked_pos())?
        };
        if survives
            && (block.class() != C::WallHangingSignBlock
                || wall_hanging_sign_can_place(placed, reader, context.clicked_pos())?)
        {
            return Ok(Some(
                if matches!(block.class(), C::WallSignBlock | C::WallHangingSignBlock) {
                    placed.with(p::WATERLOGGED, waterlogged).unwrap()
                } else {
                    placed
                },
            ));
        }
    }
    Ok(None)
}

/// Ports the sign/banner updateShape overrides. A failed attachment returns air
/// before SignBlock's water tick; other directions reach that superclass hook.
pub(super) fn update_shape(
    state: BlockState,
    pos: BlockPos,
    side: Direction,
    neighbor_pos: BlockPos,
    neighbor: BlockState,
    level: &mut LevelAccessRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    let check = match state.block().class() {
        C::StandingSignBlock | C::BannerBlock => side == Direction::Down,
        C::WallSignBlock | C::WallBannerBlock => {
            side == direction(state.get(p::FACING).unwrap()).opposite()
        }
        C::CeilingHangingSignBlock => side == Direction::Up,
        C::WallHangingSignBlock => {
            side.axis() == direction(state.get(p::FACING).unwrap()).clockwise().axis()
        }
        _ => false,
    };
    if check && !level.as_reader().can_survive(state, pos)? {
        return Ok(Block::AIR.default_state());
    }
    if state.block().is_instance_of(C::SignBlock) && state.get(p::WATERLOGGED) == Some(true) {
        level.schedule_fluid_tick(pos, fluid::Id::WATER, 5)?;
    }
    crate::block::BASE_BLOCK.update_shape(state, pos, side, neighbor_pos, neighbor, level)
}
