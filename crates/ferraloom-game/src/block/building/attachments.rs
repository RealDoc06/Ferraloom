//! Standing/wall torches, ladders, lanterns, clusters, end rods and grindstones.
use super::{facing, schedule_water, with_facing};
use crate::{
    block::{BlockBehavior, BlockMethod, BlockPlaceContext, LevelAccessRef, LevelReaderRef},
    level::LevelError,
};
use ferraloom_data::{
    Block, BlockClass as C, BlockState, properties as p, registries::fluid::Id as Fluid,
};
use ferraloom_util::{Axis, BlockPos, Direction};

pub(super) struct Attachments;
pub(super) static ATTACHMENTS: Attachments = Attachments;
fn lantern(block: Block) -> bool {
    matches!(block.class(), C::LanternBlock | C::WeatheringLanternBlock)
}
pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    let attached = lantern(block)
        || matches!(
            block.class(),
            C::LadderBlock | C::WallTorchBlock | C::AmethystClusterBlock | C::GrindstoneBlock
        );
    match method {
        BlockMethod::GetStateForPlacement => attached || block.class() == C::EndRodBlock,
        BlockMethod::CanSurvive
        | BlockMethod::UpdateShape
        | BlockMethod::UpdateFromNeighbourShapes => attached || block.class() == C::TorchBlock,
        _ => false,
    }
}
fn support_direction(state: BlockState) -> Direction {
    if lantern(state.block()) {
        if state.get(p::HANGING) == Some(true) {
            Direction::Up
        } else {
            Direction::Down
        }
    } else {
        facing(state).opposite()
    }
}

impl BlockBehavior for Attachments {
    /// Ports each registered native getStateForPlacement, retaining the native
    /// nearest-direction ordering and LadderBlock's existing-ladder exclusion.
    fn get_state_for_placement(
        &self,
        block: Block,
        context: &BlockPlaceContext<'_>,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<BlockState>, LevelError> {
        let reader = level.as_reader();
        let pos = context.clicked_pos();
        let state = block.default_state();
        if block.class() == C::EndRodBlock {
            let face = context.clicked_face();
            let other = reader.get_block_state(pos.relative(face.opposite(), 1))?;
            return Ok(Some(with_facing(
                state,
                if other.block() == block && facing(other) == face {
                    face.opposite()
                } else {
                    face
                },
            )));
        }
        if block.class() == C::LadderBlock && !context.replacing_clicked_on_block(&reader)? {
            let old = reader.get_block_state(pos.relative(context.clicked_face().opposite(), 1))?;
            if old.block() == block && facing(old) == context.clicked_face() {
                return Ok(None);
            }
        }
        let water = if block.class() == C::WallTorchBlock || block.class() == C::GrindstoneBlock {
            false
        } else {
            reader.get_fluid_state(pos)?.fluid_type == Fluid::WATER
        };
        if block.class() == C::AmethystClusterBlock {
            return Ok(Some(with_facing(
                state.with(p::WATERLOGGED, water).unwrap(),
                context.clicked_face(),
            )));
        }
        for direction in context.nearest_looking_directions(&reader)? {
            let candidate = if lantern(block) {
                if direction.axis() != Axis::Y {
                    continue;
                }
                state.with(p::HANGING, direction == Direction::Up).unwrap()
            } else if block.class() == C::GrindstoneBlock {
                if direction.axis() == Axis::Y {
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
                }
            } else {
                if direction.axis() == Axis::Y {
                    continue;
                }
                with_facing(state, direction.opposite())
            };
            if self.can_survive(candidate, pos, &reader)? {
                return Ok(Some(if candidate.get(p::WATERLOGGED).is_some() {
                    candidate.with(p::WATERLOGGED, water).unwrap()
                } else {
                    candidate
                }));
            }
        }
        Ok(None)
    }

    /// Ports LanternBlock#canSurvive and the full-face attachment predicates.
    /// GrindstoneBlock's override deliberately accepts every support state.
    fn can_survive(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        // Port of BaseTorchBlock#canSurvive, inherited by both standing torches.
        if state.block().class() == C::TorchBlock {
            return level.can_support_center(pos.below(1), Direction::Up);
        }
        if state.block().class() == C::GrindstoneBlock {
            return Ok(true);
        }
        let side = support_direction(state);
        let at = pos.relative(side, 1);
        if lantern(state.block()) {
            level.can_support_center(at, side.opposite())
        } else {
            level.is_face_sturdy(level.get_block_state(at)?, at, side.opposite())
        }
    }

    /// Ports native updateShape ordering: an unsupported ladder returns air
    /// before its fluid tick; lanterns and clusters schedule that tick first.
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        // BaseTorchBlock#updateShape invalidates only the lower support and
        // otherwise invokes Block's base body, without a fluid tick.
        if state.block().class() == C::TorchBlock {
            if direction == Direction::Down && !self.can_survive(state, pos, &level.as_reader())? {
                return Ok(Block::AIR.default_state());
            }
            return crate::block::BASE_BLOCK.update_shape(
                state,
                pos,
                direction,
                _neighbor_pos,
                _neighbor,
                level,
            );
        }
        if state.block().class() == C::GrindstoneBlock {
            return Ok(state);
        }
        let broken = |reader: &LevelReaderRef<'_, '_>| -> Result<bool, LevelError> {
            Ok(direction == support_direction(state) && !self.can_survive(state, pos, reader)?)
        };
        if state.block().class() == C::LadderBlock && broken(&level.as_reader())? {
            return Ok(Block::AIR.default_state());
        }
        schedule_water(state, pos, level)?;
        Ok(
            if state.block().class() != C::LadderBlock && broken(&level.as_reader())? {
                Block::AIR.default_state()
            } else {
                state
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};

    #[test]
    fn standing_torches_match_native_center_support_and_downward_invalidation() {
        let mut level = super::super::tests::level();
        let pos = super::super::tests::POS;
        let mut count = 0;
        for line in include_str!("../../../tests/java/items/standing-torches.tsv").lines() {
            let fields: Vec<_> = line.split('\t').collect();
            let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
            let support = BlockState::from_id(fields[2].parse().unwrap()).unwrap();
            level.set_block_state(pos.below(1), support).unwrap();
            let reader = LevelReaderRef::Server(&level);
            let survival = reader
                .required_behavior(state.block(), BlockMethod::CanSurvive)
                .unwrap();
            assert_eq!(
                survival.can_survive(state, pos, &reader).unwrap(),
                fields[3].parse::<bool>().unwrap(),
                "{line}"
            );
            *level.random() = Random::Legacy(LegacyRandomSource::new(1234));
            for (index, direction) in Direction::VALUES.into_iter().enumerate() {
                let neighbor_pos = pos.relative(direction, 1);
                let neighbor = level.get_block_state(neighbor_pos).unwrap();
                let mut access = LevelAccessRef::Server(&mut level);
                let behavior = access
                    .as_reader()
                    .required_behavior(state.block(), BlockMethod::UpdateShape)
                    .unwrap();
                assert_eq!(
                    behavior
                        .update_shape(state, pos, direction, neighbor_pos, neighbor, &mut access)
                        .unwrap()
                        .id(),
                    fields[4 + index].parse::<u16>().unwrap(),
                    "{line}"
                );
            }
            assert_eq!(
                level.random().next_long(),
                fields[10].parse::<i64>().unwrap(),
                "{line}"
            );
            count += 1;
        }
        assert_eq!(count, 34);
    }
}
