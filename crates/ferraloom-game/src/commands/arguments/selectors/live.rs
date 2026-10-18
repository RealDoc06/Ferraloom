use super::{EntityRef, SelectorPredicate, rotation_matches};
use crate::commands::exceptions;
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_nbt::Tag;

/// Required entity reads retain the original typed failure through Brigadier.
#[derive(Debug)]
pub struct EntityReadError {
    pub error: crate::level::LevelError,
    text: String,
}
impl ferraloom_brigadier::Message for EntityReadError {
    fn text(&self) -> &str {
        &self.text
    }
}
pub fn selector_read_error(error: crate::level::LevelError) -> CommandSyntaxException {
    CommandSyntaxException::new(
        ferraloom_brigadier::ExceptionKind::Custom("ferraloom.entity.read".into()),
        std::sync::Arc::new(EntityReadError {
            error,
            text: format!("Required entity read failed: {error:?}"),
        }),
    )
}
/// Canonical live selector reads; the caller supplies the same-phase scoreboard.
pub fn live_entity_matches(
    level: &crate::level::Level,
    scoreboard: &crate::scoreboard::Scoreboard,
    type_tags: &std::collections::BTreeMap<String, Vec<ferraloom_data::EntityType>>,
    entity: EntityRef,
    predicate: &SelectorPredicate,
) -> Result<bool, CommandSyntaxException> {
    let state = level.entity_state(entity).map_err(selector_read_error)?;
    let base = state.as_entity();
    let player = match state {
        crate::entity::EntityState::Player(player) => Some(player),
        _ => None,
    };
    Ok(match predicate {
        SelectorPredicate::Alive => level
            .entity_behavior(entity)
            .map_err(selector_read_error)?
            .is_alive(entity, level)
            .map_err(selector_read_error)?,
        SelectorPredicate::Name { value, inverted } => {
            (level
                .entity_plain_text_name(entity)
                .map_err(selector_read_error)?
                == *value)
                != *inverted
        }
        SelectorPredicate::GameMode { value, inverted } => {
            player.is_some_and(|player| (player.game_mode == *value) != *inverted)
        }
        SelectorPredicate::Team { value, inverted } => {
            let team = scoreboard.get_players_team(
                &level
                    .entity_scoreboard_name(entity)
                    .map_err(selector_read_error)?,
            );
            (team.map_or("", |team| team.name()) == value) != *inverted
        }
        SelectorPredicate::Type {
            value,
            tag,
            inverted,
        } => {
            let matched = if *tag {
                type_tags
                    .get(value)
                    .is_some_and(|types| types.contains(&base.entity_type))
            } else {
                value == base.entity_type.name()
            };
            matched != *inverted
        }
        SelectorPredicate::Tag { value, inverted } => {
            let matched = if value.is_empty() {
                base.tags.is_empty()
            } else {
                base.tags.contains(value)
            };
            matched != *inverted
        }
        SelectorPredicate::Nbt { value, inverted } => {
            let tag = level
                .entity_nbt_to_compare(entity)
                .map_err(selector_read_error)?;
            ferraloom_nbt::compare_nbt(Some(value), Some(&Tag::Compound(tag)), true) != *inverted
        }
        SelectorPredicate::Level(range) => {
            player.is_some_and(|player| range.contains(f64::from(player.experience_level)))
        }
        SelectorPredicate::Pitch(range) => rotation_matches(*range, base.pitch),
        SelectorPredicate::Yaw(range) => rotation_matches(*range, base.yaw),
        SelectorPredicate::Scores(scores) => {
            let holder = level
                .entity_scoreboard_name(entity)
                .map_err(selector_read_error)?;
            scores.iter().all(|(objective, range)| {
                scoreboard.get_objective(objective).is_some()
                    && scoreboard
                        .get_player_score(&holder, objective)
                        .is_some_and(|score| range.contains(f64::from(score.value)))
            })
        }
        SelectorPredicate::Advancements(_) | SelectorPredicate::Predicate { .. } => {
            return Err(exceptions::error("ferraloom.selector.pending", &[]));
        }
    })
}
