//! Port of Entity/Player/ItemEntity name, display-name and scoreboard-name reads.
use super::{Entity, EntityData, EntityRef, EntityState, item::ItemEntity};
use crate::{
    level::{Level, LevelError},
    scoreboard::Scoreboard,
};
use ferraloom_protocol::chat::{Component, HoverEvent};
use std::borrow::Cow;

/// Port of Entity#removeAction. Only siblings are traversed; component contents
/// (including translation arguments) and the other styles remain intact.
fn remove_action(mut name: Component) -> Component {
    name.style.click_event = None;
    name.extra = name.extra.into_iter().map(remove_action).collect();
    name
}
pub(super) fn base_name<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
) -> Result<Component, LevelError> {
    let name = level
        .entity::<EntityData>(entity)?
        .entity_data
        .get(super::synched_data::CUSTOM_NAME);
    match name {
        Some(name) => Ok(remove_action((**name).clone())),
        None => behavior.type_name(entity, level),
    }
}
pub(super) fn type_name(entity: EntityRef, level: &Level) -> Result<Component, LevelError> {
    let kind = level.entity::<EntityData>(entity)?.entity_type;
    let (namespace, path) = kind
        .name()
        .split_once(':')
        .expect("registered entity identifier");
    Ok(Component::translatable(
        format!("entity.{namespace}.{}", path.replace('/', ".")),
        Vec::new(),
    ))
}
pub(super) fn item_name(entity: EntityRef, level: &Level) -> Result<Component, LevelError> {
    let item = level.entity::<ItemEntity>(entity)?;
    if let Some(name) = item
        .parent
        .entity_data
        .get(super::synched_data::CUSTOM_NAME)
    {
        // ItemEntity#getName deliberately keeps the custom component's actions.
        return Ok((**name).clone());
    }
    crate::item::name::get_item_name(item.item())
        .map_err(|_| LevelError::Inventory("invalid item name registry/component"))
}
fn formatted_name<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
    scoreboard: &Scoreboard,
) -> Result<Component, LevelError> {
    let scoreboard_name = level.entity_scoreboard_name(entity)?;
    let team = scoreboard.get_players_team(&scoreboard_name);
    let name = behavior.name(entity, level)?;
    Ok(match team {
        Some(team) => team.formatted_name(name),
        None => name,
    })
}
pub(super) fn display_name<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
    scoreboard: &Scoreboard,
) -> Result<Component, LevelError> {
    let mut name = formatted_name(behavior, entity, level, scoreboard)?;
    let base = level.entity::<EntityData>(entity)?;
    let (entity_type, uuid) = (base.entity_type, base.uuid);
    name.style.hover_event = Some(HoverEvent::ShowEntity {
        entity_type: entity_type
            .name()
            .parse()
            .expect("registered entity identifier"),
        uuid,
        name: Some(Box::new(behavior.name(entity, level)?)),
    });
    name.style.insertion = Some(uuid.to_string());
    Ok(name)
}
pub(super) fn player_display_name<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
    scoreboard: &Scoreboard,
) -> Result<Component, LevelError> {
    let name = formatted_name(behavior, entity, level, scoreboard)?;
    Ok(level
        .entity::<super::player::ServerPlayer>(entity)?
        .decorate_display_name_component(name))
}

impl Level {
    /// Port of Nameable#getPlainTextName, including Player's profile-name override.
    pub fn entity_plain_text_name(&self, entity: EntityRef) -> Result<String, LevelError> {
        if let EntityState::Player(player) = self.entity_state(entity)? {
            return Ok(player.name.clone());
        }
        Ok(crate::language::plain_text(&self.entity_name(entity)?))
    }
    pub fn entity_name(&self, entity: EntityRef) -> Result<Component, LevelError> {
        self.entity_behavior(entity)?.name(entity, self)
    }
    pub fn entity_display_name(&self, entity: EntityRef) -> Result<Component, LevelError> {
        self.entity_display_name_with_scoreboard(entity, self.shared()?.scoreboard.scoreboard())
    }
    pub fn entity_display_name_with_scoreboard(
        &self,
        entity: EntityRef,
        scoreboard: &Scoreboard,
    ) -> Result<Component, LevelError> {
        self.entity_behavior(entity)?
            .display_name(entity, self, scoreboard)
    }
    pub fn entity_scoreboard_name(&self, entity: EntityRef) -> Result<Cow<'_, str>, LevelError> {
        let state = self.entity_state(entity)?;
        let player_name = match state {
            EntityState::Player(player) => Some(player.name.as_str()),
            _ => None,
        };
        Ok(crate::scoreboard::scoreboard_name(
            player_name,
            state.as_entity().uuid,
        ))
    }
}

#[cfg(test)]
mod tests;
