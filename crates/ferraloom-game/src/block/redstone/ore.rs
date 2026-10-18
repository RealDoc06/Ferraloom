//! RedStoneOreBlock's lighting, server particles and item-placement pass-through.
use crate::{
    block::{
        BlockBehavior, BlockPlaceContext, InteractionResult, LevelReaderRef, PlacementHit,
        UseContext, UseOnContext,
    },
    entity::{EntityRef, player::ServerPlayer},
    inventory::{ContainerHandle, ContainerSlot, StackOperation},
    level::{Level, LevelError},
};
use ferraloom_data::{BlockState, properties as p};
use ferraloom_protocol::{
    entity_data::{Dust, Particle},
    play::InteractionHand,
};
use ferraloom_util::{Axis, BlockPos, Direction, Vec3, random::RandomSource};
pub struct RedStoneOreBlock;
pub static ORE: RedStoneOreBlock = RedStoneOreBlock;

/// Port of RedStoneOreBlock#spawnParticles. Each exposed face consumes exactly
/// two nextFloat draws, in X/Y/Z coordinate order and Direction.values order.
fn spawn_particles(pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
    for direction in Direction::VALUES {
        if level
            .get_block_state(pos.relative(direction, 1))?
            .data()
            .solid_render
        {
            continue;
        }
        let step = direction.step();
        let random = level.random();
        let x = if direction.axis() == Axis::X {
            0.5 + 0.5625 * f64::from(step.x)
        } else {
            f64::from(random.next_float())
        };
        let y = if direction.axis() == Axis::Y {
            0.5 + 0.5625 * f64::from(step.y)
        } else {
            f64::from(random.next_float())
        };
        let z = if direction.axis() == Axis::Z {
            0.5 + 0.5625 * f64::from(step.z)
        } else {
            f64::from(random.next_float())
        };
        level.add_particle(
            Particle::Dust(Dust {
                color: 0xff0000,
                scale: 1.0,
            }),
            Vec3::new(
                f64::from(pos.x) + x,
                f64::from(pos.y) + y,
                f64::from(pos.z) + z,
            ),
            Vec3::ZERO,
        );
    }
    Ok(())
}
/// Port of RedStoneOreBlock#interact. Particles precede the unlit-state write.
pub(super) fn interact(
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    spawn_particles(pos, level)?;
    if !state.get(p::LIT).unwrap() {
        level.set_block_and_update(pos, state.with(p::LIT, true).unwrap())?;
    }
    Ok(())
}
impl BlockBehavior for RedStoneOreBlock {
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if state.get(p::LIT).unwrap() {
            level.set_block_and_update(pos, state.with(p::LIT, false).unwrap())?;
        }
        Ok(())
    }
    fn step_on(
        &self,
        state: BlockState,
        pos: BlockPos,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !level
            .entity_behavior(entity)?
            .is_stepping_carefully(entity, level)?
        {
            interact(state, pos, level)?;
        }
        crate::block::BASE_BLOCK.step_on(state, pos, entity, level)
    }
    fn use_item_on(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        let slot = match context.hand {
            InteractionHand::MainHand => level
                .entity::<ServerPlayer>(context.player)?
                .inventory
                .selected_slot(),
            InteractionHand::OffHand => 40,
        };
        let mut operation = StackOperation::new(level);
        let original = operation
            .pin(ContainerSlot {
                container: ContainerHandle::PlayerInventory(context.player),
                slot,
            })
            .map_err(super::stack_error)?;
        interact(state, pos, operation.level)?;
        // Native reads the passed ItemStack after interact's callbacks. Keep
        // that original receiver, including an old hand replaced by a callback.
        if operation
            .level
            .stack(original)
            .map_err(super::stack_error)?
            .item()
            .data()
            .block
            .is_some()
        {
            let clicked = context.hit.block_pos;
            let use_on = UseOnContext::new_retained(
                Some(context.player),
                context.hand,
                original,
                PlacementHit {
                    pos: clicked,
                    face: context.hit.direction,
                    inside: context.hit.inside,
                    location: Vec3::new(
                        f64::from(clicked.x) + f64::from(context.hit.offset_x),
                        f64::from(clicked.y) + f64::from(context.hit.offset_y),
                        f64::from(clicked.z) + f64::from(context.hit.offset_z),
                    ),
                },
            );
            let reader = LevelReaderRef::Server(operation.level);
            if BlockPlaceContext::new(&reader, use_on)?.can_place(&reader)? {
                return Ok(InteractionResult::Pass);
            }
        }
        Ok(InteractionResult::SUCCESS)
    }
}
