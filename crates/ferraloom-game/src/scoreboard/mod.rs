//! Canonical vanilla scoreboard state. The server publishes immutable phase snapshots to Levels.
mod save;
mod server;
pub use server::ServerScoreboard;
#[cfg(test)]
mod command_tests;
mod team;
#[cfg(test)]
mod tests;

use ferraloom_protocol::chat::Component;
pub use ferraloom_protocol::play::{DisplaySlot, NumberFormat, ObjectiveRenderType as RenderType};
use ferraloom_util::collections::Object2ObjectOpenHashMap as Map;
pub use save::ScoreboardSaveData;
pub use team::{
    CollisionRule, PlayerTeam, TeamColor, TeamRead, Visibility, can_collide, entity_is_allied_to,
    entity_team, scoreboard_name,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    DuplicateObjective(String),
    UnknownObjective(String),
    UnknownTeam(String),
    WrongTeam { holder: String, team: String },
    ReadOnly(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scoreboard: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

/// Port of ObjectiveCriteria. Statistic criteria are writable, as in Stat's superclass constructor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectiveCriteria {
    pub name: String,
    pub read_only: bool,
    pub default_render_type: RenderType,
}
impl ObjectiveCriteria {
    pub fn by_name(name: &str) -> Option<Self> {
        let mut reader = ferraloom_brigadier::StringReader::new(name);
        let parsed = crate::commands::arguments::scoreboard::criteria(&mut reader).ok()?;
        if reader.can_read() {
            return None;
        }
        Some(Self {
            name: parsed.name,
            read_only: matches!(name, "health" | "food" | "air" | "armor" | "xp" | "level"),
            default_render_type: if parsed.hearts {
                RenderType::Hearts
            } else {
                RenderType::Integer
            },
        })
    }
}
/// Port of Objective. The scoreboard key owns its immutable name.
#[derive(Clone, Debug, PartialEq)]
pub struct Objective {
    pub criteria: ObjectiveCriteria,
    pub display_name: Component,
    pub render_type: RenderType,
    pub display_auto_update: bool,
    pub number_format: Option<NumberFormat>,
}
impl Objective {
    pub fn new(criteria: ObjectiveCriteria, display_name: Component) -> Self {
        Self {
            render_type: criteria.default_render_type,
            criteria,
            display_name,
            display_auto_update: false,
            number_format: None,
        }
    }
}
/// Port of Score. New scores are locked; the disk codec's omitted Locked field defaults to false.
#[derive(Clone, Debug, PartialEq)]
pub struct Score {
    pub value: i32,
    pub locked: bool,
    pub display: Option<Component>,
    pub number_format: Option<NumberFormat>,
}
impl Default for Score {
    fn default() -> Self {
        Self {
            value: 0,
            locked: true,
            display: None,
            number_format: None,
        }
    }
}

/// Port of Scoreboard. All membership changes go through this owner to keep its two indexes consistent.
#[derive(Clone, Debug)]
pub struct Scoreboard {
    objectives: Map<String, Objective>,
    objectives_by_criteria: Map<String, Vec<String>>,
    scores: Map<String, Vec<(String, Score)>>,
    display_slots: [Option<String>; 19],
    teams: Map<String, PlayerTeam>,
    teams_by_player: Map<String, String>,
}
impl Default for Scoreboard {
    fn default() -> Self {
        Self {
            objectives: Map::with_capacity_and_load_factor(16, 0.5),
            objectives_by_criteria: Map::new(),
            scores: Map::with_capacity_and_load_factor(16, 0.5),
            display_slots: std::array::from_fn(|_| None),
            teams: Map::new(),
            teams_by_player: Map::new(),
        }
    }
}
impl Scoreboard {
    pub fn get_objective(&self, name: &str) -> Option<&Objective> {
        self.objectives.get(&name.to_owned())
    }
    pub fn objectives(&self) -> impl DoubleEndedIterator<Item = (&str, &Objective)> {
        self.objectives.iter().map(|(k, v)| (k.as_str(), v))
    }
    pub fn add_objective(&mut self, name: String, objective: Objective) -> Result<()> {
        if self.get_objective(&name).is_some() {
            return Err(Error::DuplicateObjective(name));
        }
        self.objectives_by_criteria
            .get_or_insert_with(objective.criteria.name.clone(), Vec::new)
            .push(name.clone());
        self.objectives.insert(name, objective);
        Ok(())
    }
    /// Port of objectivesByCriteria's insertion-ordered objective lists.
    pub fn objectives_for_criterion(&self, criterion: &str) -> impl Iterator<Item = &str> {
        self.objectives_by_criteria
            .get(&criterion.to_owned())
            .into_iter()
            .flatten()
            .map(String::as_str)
    }
    pub fn objective_mut(&mut self, name: &str) -> Result<&mut Objective> {
        self.objectives
            .get_mut(&name.to_owned())
            .ok_or_else(|| Error::UnknownObjective(name.to_owned()))
    }
    pub fn remove_objective(&mut self, name: &str) -> Result<Objective> {
        let objective = self
            .objectives
            .remove(&name.to_owned())
            .ok_or_else(|| Error::UnknownObjective(name.to_owned()))?;
        self.objectives_by_criteria
            .get_mut(&objective.criteria.name)
            .unwrap()
            .retain(|key| key != name);
        for slot in &mut self.display_slots {
            if slot.as_deref() == Some(name) {
                *slot = None;
            }
        }
        let holders: Vec<_> = self.scores.keys().cloned().collect();
        for holder in holders {
            self.scores
                .get_mut(&holder)
                .unwrap()
                .retain(|(key, _)| key != name);
        }
        Ok(objective)
    }
    pub fn get_display_objective(&self, slot: DisplaySlot) -> Option<&str> {
        self.display_slots[slot as usize].as_deref()
    }
    pub fn set_display_objective(&mut self, slot: DisplaySlot, name: Option<String>) -> Result<()> {
        if let Some(name) = &name {
            self.require_objective(name)?;
        }
        self.display_slots[slot as usize] = name;
        Ok(())
    }
    fn require_objective(&self, name: &str) -> Result<&Objective> {
        self.get_objective(name)
            .ok_or_else(|| Error::UnknownObjective(name.to_owned()))
    }
    pub fn get_player_score(&self, holder: &str, objective: &str) -> Option<&Score> {
        self.scores
            .get(&holder.to_owned())?
            .iter()
            .find(|(key, _)| key == objective)
            .map(|(_, score)| score)
    }
    /// Port of ScoreContents#getScore and ReadOnlyScoreInfo#formatValue. Missing objective or score resolves to Component.empty().
    pub fn formatted_score(&self, holder: &str, objective: &str) -> Component {
        let Some(obj) = self.get_objective(objective) else {
            return Component::default();
        };
        let Some(score) = self.get_player_score(holder, objective) else {
            return Component::default();
        };
        match score.number_format.as_ref().or(obj.number_format.as_ref()) {
            Some(NumberFormat::Blank {}) => Component::default(),
            Some(NumberFormat::Fixed { value }) => value.clone(),
            Some(NumberFormat::Styled { style }) => Component {
                style: style.clone(),
                ..Component::literal(score.value.to_string())
            },
            None => Component::literal(score.value.to_string()),
        }
    }
    pub fn get_or_create_score(&mut self, holder: &str, objective: &str) -> Result<&mut Score> {
        self.require_objective(objective)?;
        let scores = self.scores.get_or_insert_with(holder.to_owned(), Vec::new);
        let index = scores
            .iter()
            .position(|(key, _)| key == objective)
            .unwrap_or_else(|| {
                scores.push((objective.to_owned(), Score::default()));
                scores.len() - 1
            });
        Ok(&mut scores[index].1)
    }
    /// Port of ScoreAccess#set, including displayAutoUpdate and the read-only gate.
    pub fn set_score(
        &mut self,
        holder: &str,
        objective: &str,
        value: i32,
        display: Option<&Component>,
        force_writable: bool,
    ) -> Result<()> {
        let obj = self.require_objective(objective)?;
        if obj.criteria.read_only && !force_writable {
            return Err(Error::ReadOnly(objective.to_owned()));
        }
        let update = obj.display_auto_update;
        let score = self.get_or_create_score(holder, objective)?;
        score.value = value;
        if update && let Some(display) = display {
            score.display = Some(display.clone());
        }
        Ok(())
    }
    pub fn tracked_players(&self) -> impl Iterator<Item = &str> {
        self.scores.keys().rev().map(String::as_str)
    }
    pub fn player_scores(&self, holder: &str) -> impl Iterator<Item = (&str, &Score)> {
        self.scores
            .get(&holder.to_owned())
            .into_iter()
            .flatten()
            .map(|(key, score)| (key.as_str(), score))
    }
    /// Port of Scoreboard#listPlayerScores(Objective): fastutil forEach order, unlike getTrackedPlayers' stream.
    pub fn scores_for_objective<'a>(
        &'a self,
        objective: &'a str,
    ) -> impl Iterator<Item = (&'a str, &'a Score)> {
        self.scores.iter().filter_map(move |(holder, scores)| {
            scores
                .iter()
                .find(|(key, _)| key == objective)
                .map(|(_, score)| (holder.as_str(), score))
        })
    }
    pub fn reset_score(&mut self, holder: &str, objective: Option<&str>) {
        let key = holder.to_owned();
        if let Some(objective) = objective {
            if let Some(scores) = self.scores.get_mut(&key) {
                scores.retain(|(name, _)| name != objective);
                if scores.is_empty() {
                    self.scores.remove(&key);
                }
            }
        } else {
            self.scores.remove(&key);
        }
    }
    pub fn get_player_team(&self, name: &str) -> Option<&PlayerTeam> {
        self.teams.get(&name.to_owned())
    }
    pub fn get_players_team(&self, holder: &str) -> Option<&PlayerTeam> {
        self.get_player_team(self.teams_by_player.get(&holder.to_owned())?)
    }
    pub fn teams(&self) -> impl DoubleEndedIterator<Item = &PlayerTeam> {
        self.teams.values()
    }
    pub fn add_player_team(&mut self, name: &str) -> &mut PlayerTeam {
        self.teams
            .get_or_insert_with(name.to_owned(), || PlayerTeam::new(name.to_owned()))
    }
    pub fn team_mut(&mut self, name: &str) -> Result<&mut PlayerTeam> {
        self.teams
            .get_mut(&name.to_owned())
            .ok_or_else(|| Error::UnknownTeam(name.to_owned()))
    }
    pub fn remove_player_team(&mut self, name: &str) -> Result<PlayerTeam> {
        let team = self
            .teams
            .remove(&name.to_owned())
            .ok_or_else(|| Error::UnknownTeam(name.to_owned()))?;
        for holder in team.members() {
            self.teams_by_player.remove(holder);
        }
        Ok(team)
    }
    /// Port of Scoreboard#addPlayerToTeam. Re-adding to the same team returns true too.
    pub fn add_player_to_team(&mut self, holder: &str, team: &str) -> Result<bool> {
        if self.get_player_team(team).is_none() {
            return Err(Error::UnknownTeam(team.to_owned()));
        }
        self.remove_player_from_team(holder);
        self.teams_by_player
            .insert(holder.to_owned(), team.to_owned());
        Ok(self
            .teams
            .get_mut(&team.to_owned())
            .unwrap()
            .members
            .insert(holder.to_owned()))
    }
    pub fn remove_player_from_team(&mut self, holder: &str) -> bool {
        if let Some(team) = self.teams_by_player.remove(&holder.to_owned()) {
            self.teams
                .get_mut(&team)
                .unwrap()
                .members
                .remove(&holder.to_owned());
            true
        } else {
            false
        }
    }
    pub fn remove_player_from_specific_team(&mut self, holder: &str, team: &str) -> Result<()> {
        if self
            .teams_by_player
            .get(&holder.to_owned())
            .map(String::as_str)
            != Some(team)
        {
            return Err(Error::WrongTeam {
                holder: holder.to_owned(),
                team: team.to_owned(),
            });
        }
        self.remove_player_from_team(holder);
        Ok(())
    }
}
