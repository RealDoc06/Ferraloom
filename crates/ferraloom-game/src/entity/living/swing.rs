//! LivingEntity's transient hand animation and effect-modified swing cadence.
use crate::{
    entity::{
        EntityRef, LivingEntity,
        tracking::{TrackingAudience, TrackingPacket},
    },
    level::{Level, LevelError},
};
use ferraloom_data::registries::mob_effect::Id as MobEffect;
use ferraloom_protocol::{
    codec::VarInt,
    component::SwingAnimation,
    play::{InteractionHand, clientbound as cb},
};
use ferraloom_util::{java, mth};

/// Port of LivingEntity.SwingDescription, an immutable captured animation.
#[derive(Clone, Debug, PartialEq)]
pub struct SwingDescription {
    pub hand: InteractionHand,
    pub animation: SwingAnimation,
    pub duration_ticks: i32,
}
/// Port of LivingEntity.SwingState. This transient state is never persisted.
#[derive(Debug, Default)]
pub struct SwingState {
    current_swing: Option<SwingDescription>,
    ticks: i32,
    old_animation: f32,
    animation: f32,
}
impl SwingState {
    pub fn current_swing(&self) -> Option<&SwingDescription> {
        self.current_swing.as_ref()
    }
    pub fn is_swinging(&self) -> bool {
        self.current_swing.is_some()
    }
    /// Port of SwingState#startIfAble. The restart gate uses the previous
    /// duration; an equal description preserves its interpolation progress.
    pub fn start_if_able(
        &mut self,
        hand: InteractionHand,
        animation: SwingAnimation,
        duration_ticks: i32,
    ) -> bool {
        if self
            .current_swing
            .as_ref()
            .is_some_and(|swing| self.ticks <= swing.duration_ticks / 2 && self.ticks > 0)
        {
            return false;
        }
        let next = SwingDescription {
            hand,
            animation,
            duration_ticks,
        };
        if self.current_swing.as_ref() != Some(&next) {
            self.old_animation = 0.0;
            self.animation = 0.0;
        }
        self.current_swing = Some(next);
        self.ticks = 0;
        true
    }
    /// Port of SwingState#tick, including the post-increment end boundary.
    pub fn tick(&mut self) {
        self.old_animation = self.animation;
        if let Some(swing) = &self.current_swing {
            if swing.duration_ticks > 0 {
                self.animation =
                    java::min_f32(self.ticks as f32 / swing.duration_ticks as f32, 1.0);
            }
            let old_ticks = self.ticks;
            self.ticks = self.ticks.wrapping_add(1);
            if old_ticks > swing.duration_ticks {
                self.current_swing = None;
                self.animation = 0.0;
            }
        } else {
            self.animation = 0.0;
        }
    }
    pub fn get_animation(&self, partial_ticks: f32) -> f32 {
        let to = if self.animation >= self.old_animation {
            self.animation
        } else {
            self.animation + 1.0
        };
        mth::lerp_f32(partial_ticks, self.old_animation, to)
    }
}

/// Port of LivingEntity#getModifiedSwingDuration. Haste/conduit take precedence
/// over mining fatigue, and a modified duration may be zero or negative.
pub fn modified_swing_duration(
    entity: EntityRef,
    animation: &SwingAnimation,
    level: &Level,
) -> Result<i32, LevelError> {
    let living = level.entity::<LivingEntity>(entity)?;
    let haste = living.get_effect(MobEffect::HASTE);
    let conduit = living.get_effect(MobEffect::CONDUIT_POWER);
    if haste.is_some() || conduit.is_some() {
        let amplifier = haste
            .map_or(0, |effect| effect.amplifier())
            .max(conduit.map_or(0, |effect| effect.amplifier()));
        return Ok(animation.duration.0.wrapping_sub(amplifier.wrapping_add(1)));
    }
    Ok(
        if let Some(fatigue) = living.get_effect(MobEffect::MINING_FATIGUE) {
            animation
                .duration
                .0
                .wrapping_add(fatigue.amplifier().wrapping_add(1).wrapping_mul(2))
        } else {
            animation.duration.0
        },
    )
}

/// Port of LivingEntity#swing. State starts before the synchronous tracker send;
/// the packet carries the original animation, not its effect-modified duration.
pub fn swing(
    entity: EntityRef,
    hand: InteractionHand,
    animation: SwingAnimation,
    send_to_self: bool,
    level: &mut Level,
) -> Result<bool, LevelError> {
    let duration = modified_swing_duration(entity, &animation, level)?;
    if !level
        .entity_mut::<LivingEntity>(entity)?
        .swing_state
        .start_if_able(hand, animation.clone(), duration)
    {
        return Ok(false);
    }
    let network = level.entity::<LivingEntity>(entity)?.network_id;
    level
        .broadcast_tracked_packet(
            entity,
            TrackingPacket {
                audience: if send_to_self {
                    TrackingAudience::PlayersAndSelf
                } else {
                    TrackingAudience::Players
                },
                packet: cb::Packet::SwingAnimation(cb::SwingAnimationPacket {
                    entity_id: VarInt(network),
                    hand,
                    animation,
                }),
            },
        )
        .map_err(|error| error.into_level(level))?;
    Ok(true)
}

#[cfg(test)]
mod tests;
