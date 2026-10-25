//! Entity#load on a live receiver. Position publication, world calls and subclass
//! reads retain their native order without replacing or taking out canonical state.
use super::{EntityLoadError, EntityPersistenceError, loaded_position};
use crate::{
    entity::{
        Entity, EntityData, EntityMethod, EntityRef, EntityState, LivingEntity, RemovalReason,
        experience_orb::ExperienceOrb,
        item::{FallingBlockEntity, ItemEntity, PrimedTnt},
        player::ServerPlayer,
        synched_data,
    },
    level::{Level, LevelError},
};
use ferraloom_data::tags::BlockTags;
use ferraloom_nbt::CompoundTag;

/// Port of Entity#load, preserving the full subclass receiver. This does not
/// create passengers or invoke postDataManipulated: EntityDataAccessor restores
/// the old UUID between this call and that virtual, exactly as vanilla does.
pub fn load_in_place(
    entity: EntityRef,
    tag: &CompoundTag,
    level: &mut Level,
) -> Result<(), EntityPersistenceError> {
    level.retain_entity(entity)?;
    let result = load(entity, tag, level);
    level.release_entity(entity);
    result
}
fn load(
    entity: EntityRef,
    tag: &CompoundTag,
    level: &mut Level,
) -> Result<(), EntityPersistenceError> {
    let behavior = level.entity_behavior(entity)?;
    level.entity_mut::<EntityData>(entity)?.load_motion(tag);
    level.set_entity_pos_raw(entity, loaded_position(tag))?;
    let yaw = level.entity_mut::<EntityData>(entity)?.load_rotation(tag);
    behavior.set_y_head_rot(entity, yaw, level)?;
    behavior.set_y_body_rot(entity, yaw, level)?;
    let base = level.entity_mut::<EntityData>(entity)?;
    base.load_scalar_fields(tag);
    base.validate_loaded_state()?;
    let position = base.position;
    level.set_entity_pos(entity, position)?;
    level
        .entity_mut::<EntityData>(entity)?
        .load_metadata(tag.clone());
    if let Some(team) = tag.get_string("Team") {
        let team = team.to_string_lossy();
        let holder = level.entity::<EntityData>(entity)?.uuid.to_string();
        let exists = level
            .shared()?
            .scoreboard
            .scoreboard()
            .get_player_team(&team)
            .is_some();
        if exists {
            level
                .shared_mut()?
                .scoreboard
                .add_player_to_team(&holder, &team)
                .map_err(|_| LevelError::Inventory("entity load team membership"))?;
            level.emit_scoreboard_packets()?;
        } else {
            eprintln!("Unable to add entity to team {team:?} (that team probably doesn't exist)");
        }
    }
    behavior.read_additional_save_data(entity, tag, level)?;
    if behavior.reposition_entity_after_load() {
        let position = level.entity::<EntityData>(entity)?.position;
        level.set_entity_pos(entity, position)?;
    }
    Ok(())
}
fn required(entity: EntityRef, method: EntityMethod, level: &Level) -> EntityPersistenceError {
    LevelError::UnsupportedEntityMethod(
        level
            .entity_state(entity)
            .expect("held receiver")
            .entity_type(),
        method,
    )
    .into()
}
fn living_data(
    entity: EntityRef,
    tag: &CompoundTag,
    level: &mut Level,
    player: bool,
) -> Result<(), EntityPersistenceError> {
    let setter = if player {
        crate::entity::living::damage_state::player_internal_set_absorption_amount
    } else {
        LivingEntity::internal_set_absorption_amount
    };
    level
        .entity_mut::<LivingEntity>(entity)?
        .read_living_prefix(tag, setter);
    if tag
        .get("sleeping_pos")
        .and_then(crate::entity::mob::block_pos)
        .is_some()
    {
        return Err(required(entity, EntityMethod::LoadSleepingPosition, level));
    }
    level
        .entity_mut::<LivingEntity>(entity)?
        .entity_data
        .set(synched_data::SLEEPING_POS, None);
    if tag
        .get_compound("Brain")
        .is_some_and(|brain| !brain.is_empty())
    {
        return Err(required(entity, EntityMethod::LoadBrain, level));
    }
    level
        .entity_mut::<LivingEntity>(entity)?
        .read_living_tail(tag);
    Ok(())
}
/// Protected readAdditionalSaveData dispatch. Each parent phase runs once and
/// every world-dependent query ends the canonical state borrow first.
pub(crate) fn read_additional_save_data<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    tag: &CompoundTag,
    level: &mut Level,
) -> Result<(), EntityPersistenceError> {
    let kind = level.entity_state(entity)?.entity_type();
    if kind == ferraloom_data::EntityType::COW {
        living_data(entity, tag, level, false)?;
        crate::entity::animal::persistence::read_cow_data(behavior, entity, tag, level)?;
    } else if kind == ferraloom_data::EntityType::PLAYER {
        living_data(entity, tag, level, true)?;
        level
            .entity_mut::<ServerPlayer>(entity)?
            .read_player_additional_save_data(tag)?;
        if tag.contains_key("recipeBook") {
            return Err(EntityLoadError::RecipeManagerRequired.into());
        }
    } else {
        match level.entity_state(entity)? {
            EntityState::Item(_) => {
                let discard = level
                    .entity_mut::<ItemEntity>(entity)?
                    .read_additional_save_data(tag);
                if discard {
                    level.remove_entity_with_reason(entity, RemovalReason::Discarded)?;
                }
            }
            EntityState::PrimedTnt(_) => level
                .entity_mut::<PrimedTnt>(entity)?
                .read_additional_save_data(tag),
            EntityState::ExperienceOrb(_) => level
                .entity_mut::<ExperienceOrb>(entity)?
                .read_additional_save_data(tag),
            EntityState::FallingBlock(_) => {
                level
                    .entity_mut::<FallingBlockEntity>(entity)?
                    .read_block_state(tag);
                let state = level.entity::<FallingBlockEntity>(entity)?.block_state;
                let default_hurt = level
                    .block_tags()?
                    .contains_key(BlockTags::ANVIL, state.block());
                level
                    .entity_mut::<FallingBlockEntity>(entity)?
                    .read_additional_save_data(tag, default_hurt);
            }
            EntityState::Cow(_) | EntityState::Player(_) => {
                unreachable!("living families dispatched above")
            }
        }
    }
    Ok(())
}
