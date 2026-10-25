//! Player experience arithmetic and ServerPlayer's ordered experience packet.
use super::ServerPlayer;
use crate::{
    entity::{EntityRef, synched_data},
    level::{Level, LevelError, effects::LevelEffect},
};
use ferraloom_data::registries::sound_event::Id as Sound;
use ferraloom_protocol::{codec::VarInt, play::clientbound as cb};

impl ServerPlayer {
    /// Port of Player#getScore; there is one canonical shared data value.
    pub fn score(&self) -> i32 {
        self.entity_data.get(synched_data::PLAYER_SCORE).0
    }
    pub fn set_score(&mut self, value: i32) {
        self.entity_data
            .set(synched_data::PLAYER_SCORE, VarInt(value));
    }
    pub fn increase_score(&mut self, amount: i32) {
        self.set_score(self.score().wrapping_add(amount));
    }
    /// Port of Player#getXpNeededForNextLevel, including Java int overflow.
    pub fn xp_needed_for_next_level(&self) -> i32 {
        if self.experience_level >= 30 {
            112_i32.wrapping_add(self.experience_level.wrapping_sub(30).wrapping_mul(9))
        } else if self.experience_level >= 15 {
            37_i32.wrapping_add(self.experience_level.wrapping_sub(15).wrapping_mul(5))
        } else {
            7_i32.wrapping_add(self.experience_level.wrapping_mul(2))
        }
    }
}

/// Port of ServerPlayer#giveExperienceLevels and its Player parent body.
/// Level-up sounds happen synchronously before the server invalidates its XP cache.
pub fn give_experience_levels(
    player: EntityRef,
    amount: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    if amount == 0 {
        return Ok(());
    }
    let sound = {
        let player = level.entity_mut::<ServerPlayer>(player)?;
        player.experience_level = player.experience_level.saturating_add(amount);
        if player.experience_level < 0 {
            player.experience_level = 0;
            player.experience_progress = 0.0;
            player.total_experience = 0;
        }
        if amount > 0
            && player.experience_level % 5 == 0
            && (player.last_level_up_time as f32) < player.tick_count as f32 - 100.0
        {
            let volume = if player.experience_level > 30 {
                1.0
            } else {
                player.experience_level as f32 / 30.0
            };
            Some((player.position, volume * 0.75))
        } else {
            None
        }
    };
    if let Some((position, volume)) = sound {
        let source = level.entity_behavior(player)?.sound_source();
        level.play_sound(
            None,
            position,
            Sound::ENTITY_PLAYER_LEVELUP,
            source,
            volume,
            1.0,
        )?;
        let player = level.entity_mut::<ServerPlayer>(player)?;
        player.last_level_up_time = player.tick_count;
    }
    level
        .entity_mut::<ServerPlayer>(player)?
        .last_sent_experience = -1;
    Ok(())
}

/// Port of ServerPlayer#giveExperiencePoints and Player#giveExperiencePoints.
/// Retain the native float operations and virtual level changes between loops.
pub fn give_experience_points(
    player: EntityRef,
    amount: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    if amount == 0 {
        return Ok(());
    }
    {
        let player = level.entity_mut::<ServerPlayer>(player)?;
        player.increase_score(amount);
        player.experience_progress += amount as f32 / player.xp_needed_for_next_level() as f32;
        // Native clamps only after the Java addition has wrapped.
        player.total_experience = player.total_experience.wrapping_add(amount).max(0);
    }
    while level.entity::<ServerPlayer>(player)?.experience_progress < 0.0 {
        let state = level.entity::<ServerPlayer>(player)?;
        let remaining = state.experience_progress * state.xp_needed_for_next_level() as f32;
        if state.experience_level > 0 {
            give_experience_levels(player, -1, level)?;
            let state = level.entity_mut::<ServerPlayer>(player)?;
            state.experience_progress = 1.0 + remaining / state.xp_needed_for_next_level() as f32;
        } else {
            give_experience_levels(player, -1, level)?;
            level
                .entity_mut::<ServerPlayer>(player)?
                .experience_progress = 0.0;
        }
    }
    while level.entity::<ServerPlayer>(player)?.experience_progress >= 1.0 {
        let state = level.entity_mut::<ServerPlayer>(player)?;
        state.experience_progress =
            (state.experience_progress - 1.0) * state.xp_needed_for_next_level() as f32;
        give_experience_levels(player, 1, level)?;
        let state = level.entity_mut::<ServerPlayer>(player)?;
        state.experience_progress /= state.xp_needed_for_next_level() as f32;
    }
    level
        .entity_mut::<ServerPlayer>(player)?
        .last_sent_experience = -1;
    Ok(())
}

/// Port of ServerPlayer#doTick's SetExperience slot, before LOCATION criteria.
pub fn send_experience_update(player: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let state = level.entity_mut::<ServerPlayer>(player)?;
    if state.total_experience == state.last_sent_experience {
        return Ok(());
    }
    state.last_sent_experience = state.total_experience;
    let packet = cb::Packet::SetExperience(cb::SetExperience {
        experience_progress: state.experience_progress,
        total_experience: VarInt(state.total_experience),
        experience_level: VarInt(state.experience_level),
    });
    level.emit(LevelEffect::PlayerPacket {
        recipients: vec![player],
        packet: Box::new(packet),
    });
    Ok(())
}

#[cfg(test)]
mod orb_tests;
#[cfg(test)]
mod tests;
