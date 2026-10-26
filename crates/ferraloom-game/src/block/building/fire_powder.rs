//! Native fire placement/survival and concrete powder's liquid conversion.
use crate::{
    block::{BlockBehavior, BlockMethod, BlockPlaceContext, LevelAccessRef, LevelReaderRef},
    level::{Level, LevelError},
};
use ferraloom_data::{
    Block, BlockClass as C, BlockState, properties as p,
    tags::{BlockTags, FluidTags},
};
use ferraloom_util::{BlockPos, Direction};

pub(super) struct FirePowder;
pub(super) static FIRE_POWDER: FirePowder = FirePowder;
pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    match method {
        BlockMethod::GetStateForPlacement
        | BlockMethod::UpdateShape
        | BlockMethod::UpdateFromNeighbourShapes => matches!(
            block.class(),
            C::FireBlock | C::SoulFireBlock | C::ConcretePowderBlock
        ),
        BlockMethod::CanSurvive => matches!(block.class(), C::FireBlock | C::SoulFireBlock),
        BlockMethod::OnPlace | BlockMethod::Tick | BlockMethod::OnLand => {
            block.class() == C::ConcretePowderBlock
        }
        _ => false,
    }
}
fn soul_base(state: BlockState, level: &LevelReaderRef<'_, '_>) -> Result<bool, LevelError> {
    Ok(level
        .block_tags()?
        .contains_key(BlockTags::SOUL_FIRE_BASE_BLOCKS, state.block()))
}
fn with_fire_side(state: BlockState, side: Direction, burn: bool) -> BlockState {
    macro_rules! set {
        ($property:ident, $ty:ident) => {
            state
                .with(
                    p::$property,
                    if burn { p::$ty::True } else { p::$ty::False },
                )
                .unwrap()
        };
    }
    match side {
        Direction::Up => state.with(p::UP, burn).unwrap(),
        Direction::North => set!(NORTH, North),
        Direction::South => set!(SOUTH, South),
        Direction::West => set!(WEST, West),
        Direction::East => set!(EAST, East),
        Direction::Down => unreachable!("fire has no downward connection"),
    }
}
/// Port of FireBlock#getStateForPlacement(BlockGetter, BlockPos).
fn fire_placement(pos: BlockPos, level: &LevelReaderRef<'_, '_>) -> Result<BlockState, LevelError> {
    let below = pos.below(1);
    let base = level.get_block_state(below)?;
    let mut state = Block::FIRE.default_state();
    if !base.can_burn_in_fire() && !level.is_face_sturdy(base, below, Direction::Up)? {
        for side in Direction::VALUES {
            if side != Direction::Down {
                state = with_fire_side(
                    state,
                    side,
                    level
                        .get_block_state(pos.relative(side, 1))?
                        .can_burn_in_fire(),
                );
            }
        }
    }
    Ok(state)
}
/// Port of BaseFireBlock#getState. Unlike FireBlock's placement override, this
/// helper selects soul fire, including FireBlock#getStateWithAge's update path.
pub(super) fn fire_state(
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<BlockState, LevelError> {
    if soul_base(level.get_block_state(pos.below(1))?, level)? {
        Ok(Block::SOUL_FIRE.default_state())
    } else {
        fire_placement(pos, level)
    }
}
fn can_solidify(state: BlockState, level: &LevelReaderRef<'_, '_>) -> Result<bool, LevelError> {
    Ok(level
        .fluid_tags()?
        .contains_key(FluidTags::WATER, state.data().fluid.fluid_type))
}
/// Port of ConcretePowderBlock#touchesLiquid. The first read of each iteration
/// is at the previously visited position; the sturdy query uses the original
/// powder position, exactly as the native mutable-position loop does.
fn touches_liquid(pos: BlockPos, level: &LevelReaderRef<'_, '_>) -> Result<bool, LevelError> {
    let mut at = pos;
    for side in Direction::VALUES {
        let prior = level.get_block_state(at)?;
        if side != Direction::Down || can_solidify(prior, level)? {
            at = pos.relative(side, 1);
            let neighbor = level.get_block_state(at)?;
            if can_solidify(neighbor, level)?
                && !level.is_face_sturdy(neighbor, pos, side.opposite())?
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
impl BlockBehavior for FirePowder {
    /// Port of ConcretePowderBlock's inherited FallingBlock#onPlace.
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        super::super::foundational::FALLING_BLOCK.on_place(state, pos, old, moved, level)
    }

    /// Port of the inherited FallingBlock#tick, sharing sand's real entity
    /// creation path. Hydration during flight/landing belongs to that entity.
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        super::super::foundational::FALLING_BLOCK.tick(state, pos, level)
    }

    /// Port of ConcretePowderBlock#onLand. Reuse the same native liquid-contact
    /// queries as placement, then end the reader borrow before flags-3 mutation.
    fn on_land(
        &self,
        state: BlockState,
        pos: BlockPos,
        replaced: BlockState,
        _entity: crate::entity::EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let reader = LevelReaderRef::Server(level);
        if can_solidify(replaced, &reader)? || touches_liquid(pos, &reader)? {
            let concrete = state.block().concrete_powder_target().unwrap();
            level.set_block_and_update(pos, concrete.default_state())?;
        }
        Ok(())
    }
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let reader = level.as_reader();
        let pos = context.clicked_pos();
        Ok(Some(match block.class() {
            C::FireBlock => fire_placement(pos, &reader)?,
            C::SoulFireBlock => fire_state(pos, &reader)?,
            C::ConcretePowderBlock => {
                let old = reader.get_block_state(pos)?;
                if can_solidify(old, &reader)? || touches_liquid(pos, &reader)? {
                    block.concrete_powder_target().unwrap().default_state()
                } else {
                    block.default_state()
                }
            }
            _ => unreachable!("registered fire/powder placement"),
        }))
    }
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        let below = pos.below(1);
        let base = level.get_block_state(below)?;
        if state.block().class() == C::SoulFireBlock {
            return soul_base(base, level);
        }
        if level.is_face_sturdy(base, below, Direction::Up)? {
            return Ok(true);
        }
        for side in Direction::VALUES {
            if level
                .get_block_state(pos.relative(side, 1))?
                .can_burn_in_fire()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        let reader = level.as_reader();
        if state.block().class() == C::ConcretePowderBlock {
            if touches_liquid(pos, &reader)? {
                return Ok(state
                    .block()
                    .concrete_powder_target()
                    .unwrap()
                    .default_state());
            }
            return super::super::foundational::FALLING_BLOCK.update_shape(
                state,
                pos,
                direction,
                neighbor_pos,
                neighbor,
                level,
            );
        }
        if !self.can_survive(state, pos, &reader)? {
            return Ok(Block::AIR.default_state());
        }
        if state.block().class() == C::SoulFireBlock {
            return Ok(state.block().default_state());
        }
        let result = fire_state(pos, &reader)?;
        Ok(if result.block() == Block::FIRE {
            result.with(p::AGE, state.get(p::AGE).unwrap()).unwrap()
        } else {
            result
        })
    }
}
