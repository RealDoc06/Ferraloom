//! Port of SnowLayerBlock's melt callback. Physical per-state layer shapes are
//! already cached natively; support loss shares the vegetation update port.
use super::super::BlockBehavior;
use crate::level::{Level, LevelError};
use ferraloom_data::{BlockState, properties as p, registries::game_event::Id as GameEvent};
use ferraloom_util::{BlockPos, Vec3};
pub struct Snow;
pub static SNOW: Snow = Snow;
impl BlockBehavior for Snow {
    fn support_shape(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _level: &super::super::LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        Ok(ferraloom_util::shapes::create(
            0.0,
            0.0,
            0.0,
            1.0,
            f64::from(state.get(p::LAYERS).unwrap()) / 8.0,
            1.0,
        ))
    }

    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if level.brightness(ferraloom_light::LightLayer::Block, pos)? > 11 {
            super::super::drops::drop_resources_simple(state, pos, level)?;
            level.remove_block(pos, false)?;
            level.game_event(
                GameEvent::BLOCK_DESTROY,
                Vec3::at_center_of(pos.into()),
                None,
                Some(state),
            )?;
        }
        Ok(())
    }
}
