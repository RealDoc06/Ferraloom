//! ServerPlayer's full shoulder NBT and Player's optional parrot metadata.
use super::ServerPlayer;
use crate::{
    block::UnportedOperation,
    entity::{EntityMethod, EntityRef, synched_data as data},
    level::{Level, LevelError},
};
use ferraloom_data::EntityType;
use ferraloom_nbt::CompoundTag;
use ferraloom_protocol::{Identifier, component::ParrotVariant, entity_data::OptionalUnsignedInt};
use ferraloom_util::random::RandomSource;

fn is_parrot(tag: &CompoundTag) -> bool {
    tag.get_string("id")
        .and_then(|name| Identifier::parse(&name.to_string_lossy()).ok())
        .and_then(|name| EntityType::from_name(name.as_str()))
        == Some(EntityType::PARROT)
}
/// Port of Player#extractParrotVariant and the legacy integer codec's CLAMP.
fn extract_parrot_variant(tag: &CompoundTag) -> Option<i32> {
    if !tag.is_empty() && is_parrot(tag) {
        tag.get_int("Variant").map(|id| id.clamp(0, 4))
    } else {
        None
    }
}
fn variant(id: Option<i32>) -> Option<ParrotVariant> {
    id.map(|id| match id.clamp(0, 4) {
        0 => ParrotVariant::RedBlue,
        1 => ParrotVariant::Blue,
        2 => ParrotVariant::Green,
        3 => ParrotVariant::YellowBlue,
        _ => ParrotVariant::Gray,
    })
}
impl ServerPlayer {
    pub fn shoulder_entity_left(&self) -> &CompoundTag {
        &self.shoulder_entity_left
    }
    pub fn shoulder_entity_right(&self) -> &CompoundTag {
        &self.shoulder_entity_right
    }
    /// Port of ServerPlayer#setShoulderEntityLeft; equal extracted variants do
    /// not dirty metadata even when the full server-side NBT object changes.
    pub fn set_shoulder_entity_left(&mut self, tag: CompoundTag) {
        let variant = OptionalUnsignedInt(extract_parrot_variant(&tag));
        self.shoulder_entity_left = tag;
        self.entity_data.set(data::SHOULDER_PARROT_LEFT, variant);
    }
    pub fn set_shoulder_entity_right(&mut self, tag: CompoundTag) {
        let variant = OptionalUnsignedInt(extract_parrot_variant(&tag));
        self.shoulder_entity_right = tag;
        self.entity_data.set(data::SHOULDER_PARROT_RIGHT, variant);
    }
    pub fn shoulder_parrot_left(&self) -> Option<ParrotVariant> {
        variant(self.entity_data.get(data::SHOULDER_PARROT_LEFT).0)
    }
    pub fn shoulder_parrot_right(&self) -> Option<ParrotVariant> {
        variant(self.entity_data.get(data::SHOULDER_PARROT_RIGHT).0)
    }
}
fn play_shoulder_entity_ambient_sound(
    player: EntityRef,
    left: bool,
    level: &mut Level,
) -> Result<(), LevelError> {
    let active = {
        let player = level.entity::<ServerPlayer>(player)?;
        let tag = if left {
            player.shoulder_entity_left()
        } else {
            player.shoulder_entity_right()
        };
        !tag.is_empty() && !tag.get_boolean_or("Silent", false)
    };
    if !active
        || level
            .entity_mut::<ServerPlayer>(player)?
            .random
            .next_int_bound(200)
            != 0
    {
        return Ok(());
    }
    let parrot = {
        let player = level.entity::<ServerPlayer>(player)?;
        is_parrot(if left {
            player.shoulder_entity_left()
        } else {
            player.shoulder_entity_right()
        })
    };
    if parrot {
        // Parrot.imitateNearbyMobs and the following ambient sound are one
        // required effect; its level/entity RNG belongs to the mobs port.
        level.unported_operation(UnportedOperation::EntityMethod(
            EntityType::PLAYER,
            EntityMethod::PlayShoulderEntityAmbientSound,
        ))?;
    }
    Ok(())
}
/// Port of ServerPlayer#removeEntitiesOnShoulder. Strict preserves the tag at
/// an unsupported spawn; Warn skips the spawn unit before native slot clearing.
pub fn remove_entities_on_shoulder(player: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    if level
        .entity::<ServerPlayer>(player)?
        .time_entity_sat_on_shoulder
        .wrapping_add(20)
        < level.game_time()
    {
        for left in [true, false] {
            let empty = {
                let player = level.entity::<ServerPlayer>(player)?;
                if left {
                    player.shoulder_entity_left().is_empty()
                } else {
                    player.shoulder_entity_right().is_empty()
                }
            };
            if !empty {
                level.unported_operation(UnportedOperation::EntityMethod(
                    EntityType::PLAYER,
                    EntityMethod::RespawnEntityOnShoulder,
                ))?;
            }
            let player = level.entity_mut::<ServerPlayer>(player)?;
            if left {
                player.set_shoulder_entity_left(CompoundTag::new());
            } else {
                player.set_shoulder_entity_right(CompoundTag::new());
            }
        }
    }
    Ok(())
}
/// Port of ServerPlayer#handleShoulderEntities. Each shoulder is read at its own
/// native call site, then the removal condition observes the current player.
pub fn handle_shoulder_entities(player: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    play_shoulder_entity_ambient_sound(player, true, level)?;
    play_shoulder_entity_ambient_sound(player, false, level)?;
    let remove = {
        let player = level.entity::<ServerPlayer>(player)?;
        player.fall_distance > 0.5
            || player.was_touching_water
            || player.abilities.flying
            || player.entity_data.get(data::SLEEPING_POS).is_some()
            || player.is_in_powder_snow
    };
    if remove {
        remove_entities_on_shoulder(player, level)?;
    }
    Ok(())
}
/// Port of ServerPlayer#setEntityOnShoulder, including its game-time capture.
pub fn set_entity_on_shoulder(
    player: EntityRef,
    tag: CompoundTag,
    level: &mut Level,
) -> Result<bool, LevelError> {
    let p = level.entity::<ServerPlayer>(player)?;
    if p.vehicle.get().is_some() || !p.on_ground || p.was_touching_water || p.is_in_powder_snow {
        return Ok(false);
    }
    let left = p.shoulder_entity_left().is_empty();
    if !left && !p.shoulder_entity_right().is_empty() {
        return Ok(false);
    }
    let time = level.game_time();
    let p = level.entity_mut::<ServerPlayer>(player)?;
    if left {
        p.set_shoulder_entity_left(tag);
    } else {
        p.set_shoulder_entity_right(tag);
    }
    p.time_entity_sat_on_shoulder = time;
    Ok(true)
}
#[cfg(test)]
mod tests;
