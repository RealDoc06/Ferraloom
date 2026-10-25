//! Item-side view rays; traversal belongs to the level's canonical clip query.
use crate::{
    entity::{EntityData, EntityRef, player::ServerPlayer},
    level::{
        Level, LevelError,
        clip::{ClipBlock, ClipContext, ClipFluid},
    },
};
use ferraloom_util::{BlockHitResult, Vec3};

/// Port of Item#getPlayerPOVHitResult, using the live eye, view vector and block
/// interaction range. The clip context retains the native placing entity.
pub fn get_player_pov_hit_result(
    level: &Level,
    player: EntityRef,
    fluid: ClipFluid,
) -> Result<BlockHitResult, LevelError> {
    let data = level.entity::<EntityData>(player)?;
    let from = Vec3::new(
        data.position.x,
        data.position.y + f64::from(data.eye_height),
        data.position.z,
    );
    let to = from
        + data.look_angle()
            * level
                .entity::<ServerPlayer>(player)?
                .block_interaction_range();
    let context =
        ClipContext::for_entity(from, to, ClipBlock::Outline, fluid, Some(player), level)?;
    level.clip(&context)
}
