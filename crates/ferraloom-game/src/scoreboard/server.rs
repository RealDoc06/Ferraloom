//! Port of ServerScoreboard's tracking and ordered broadcast hooks.
use super::*;
use ferraloom_protocol::{
    codec::VarInt,
    play::{ObjectiveOperation, TeamOperation, TeamParameters, clientbound as cb},
};
use std::{collections::HashSet, sync::Arc};

#[derive(Default, Debug)]
pub struct ServerScoreboard {
    data: Arc<Scoreboard>,
    tracked: HashSet<String>,
    packets: Vec<cb::Packet>,
}
impl ServerScoreboard {
    pub fn new(data: Scoreboard) -> Self {
        let tracked = data.display_slots.iter().flatten().cloned().collect();
        Self {
            data: Arc::new(data),
            tracked,
            packets: Vec::new(),
        }
    }
    pub fn scoreboard(&self) -> &Scoreboard {
        &self.data
    }
    pub fn snapshot(&self) -> Arc<Scoreboard> {
        Arc::clone(&self.data)
    }
    pub(crate) fn scoreboard_mut(&mut self) -> &mut Scoreboard {
        Arc::make_mut(&mut self.data)
    }
    /// Drain broadcasts in mutation order, including mutations preceding a command failure.
    pub fn drain_packets(&mut self) -> Vec<cb::Packet> {
        std::mem::take(&mut self.packets)
    }
    /// Port of PlayerList#updateEntireScoreboard. Team creation precedes objectives; each objective is sent once.
    pub fn join_packets(&self) -> Vec<cb::Packet> {
        let mut packets: Vec<_> = self.data.teams().map(|team| team_packet(team, 0)).collect();
        let mut seen = HashSet::new();
        for objective in self.data.display_slots.iter().flatten() {
            if seen.insert(objective) {
                packets.extend(self.start_tracking_packets(objective));
            }
        }
        packets
    }
    fn start_tracking_packets(&self, name: &str) -> Vec<cb::Packet> {
        let mut packets = vec![objective_packet(
            name,
            self.data.get_objective(name).unwrap(),
            0,
        )];
        for (index, objective) in self.data.display_slots.iter().enumerate() {
            if objective.as_deref() == Some(name) {
                packets.push(display_packet(index, Some(name)));
            }
        }
        for (holder, score) in self.data.scores_for_objective(name) {
            packets.push(score_packet(holder, name, score));
        }
        packets
    }
    fn stop_tracking(&mut self, name: &str) {
        self.packets.push(objective_packet(
            name,
            self.data.get_objective(name).unwrap(),
            1,
        ));
        for (index, objective) in self.data.display_slots.iter().enumerate() {
            if objective.as_deref() == Some(name) {
                self.packets.push(display_packet(index, Some(name)));
            }
        }
        self.tracked.remove(name);
    }
    pub fn add_objective(&mut self, name: String, objective: Objective) -> Result<()> {
        self.scoreboard_mut().add_objective(name, objective)
    }
    pub fn update_objective(
        &mut self,
        name: &str,
        update: impl FnOnce(&mut Objective),
    ) -> Result<()> {
        update(self.scoreboard_mut().objective_mut(name)?);
        if self.tracked.contains(name) {
            self.packets.push(objective_packet(
                name,
                self.data.get_objective(name).unwrap(),
                2,
            ));
        }
        Ok(())
    }
    pub fn remove_objective(&mut self, name: &str) -> Result<()> {
        self.data.require_objective(name)?;
        for index in 0..self.data.display_slots.len() {
            if self.data.display_slots[index].as_deref() == Some(name) {
                self.set_display_objective(
                    save::enum_value(
                        crate::commands::arguments::scoreboard::DISPLAY_SLOTS[index],
                        crate::commands::arguments::scoreboard::DISPLAY_SLOTS,
                    )
                    .expect("display slot"),
                    None,
                )?;
            }
        }
        // setDisplayObjective stops tracking the old objective once its last slot is cleared.
        if self.tracked.contains(name) {
            self.stop_tracking(name);
        }
        self.scoreboard_mut().remove_objective(name)?;
        Ok(())
    }
    pub fn set_display_objective(&mut self, slot: DisplaySlot, name: Option<String>) -> Result<()> {
        let old = self.data.get_display_objective(slot).map(str::to_owned);
        self.scoreboard_mut()
            .set_display_objective(slot, name.clone())?;
        if old != name
            && let Some(old) = old
        {
            if self
                .data
                .display_slots
                .iter()
                .any(|v| v.as_deref() == Some(old.as_str()))
            {
                self.packets
                    .push(display_packet(slot as usize, name.as_deref()));
            } else {
                self.stop_tracking(&old);
            }
        }
        if let Some(name) = name {
            if self.tracked.contains(&name) {
                self.packets
                    .push(display_packet(slot as usize, Some(&name)));
            } else {
                self.packets.extend(self.start_tracking_packets(&name));
                self.tracked.insert(name);
            }
        }
        Ok(())
    }
    pub fn add_player_team(&mut self, name: &str) {
        if self.data.get_player_team(name).is_some() {
            return;
        }
        self.scoreboard_mut().add_player_team(name);
        self.packets
            .push(team_packet(self.data.get_player_team(name).unwrap(), 0));
    }
    pub fn update_team(&mut self, name: &str, update: impl FnOnce(&mut PlayerTeam)) -> Result<()> {
        update(self.scoreboard_mut().team_mut(name)?);
        self.packets
            .push(team_packet(self.data.get_player_team(name).unwrap(), 2));
        Ok(())
    }
    pub fn remove_player_team(&mut self, name: &str) -> Result<()> {
        let team = self.scoreboard_mut().remove_player_team(name)?;
        self.packets.push(team_packet(&team, 1));
        Ok(())
    }
    pub fn add_player_to_team(&mut self, holder: &str, name: &str) -> Result<bool> {
        if self.data.get_player_team(name).is_none() {
            return Err(Error::UnknownTeam(name.to_owned()));
        }
        self.remove_player_from_team(holder);
        let added = self.scoreboard_mut().add_player_to_team(holder, name)?;
        if added {
            self.packets.push(member_packet(name, holder, true));
        }
        Ok(added)
    }
    pub fn remove_player_from_team(&mut self, holder: &str) -> bool {
        let Some(name) = self
            .data
            .get_players_team(holder)
            .map(|t| t.name().to_owned())
        else {
            return false;
        };
        self.scoreboard_mut().remove_player_from_team(holder);
        self.packets.push(member_packet(&name, holder, false));
        true
    }
    /// Port of ScoreAccess#set. Broadcast only a new score or a changed value/display.
    pub fn set_score(
        &mut self,
        holder: &str,
        objective: &str,
        value: i32,
        display: Option<&Component>,
        force_writable: bool,
    ) -> Result<()> {
        self.set_score_with_sync(holder, objective, value, display, force_writable, false)
    }
    pub(crate) fn set_score_with_sync(
        &mut self,
        holder: &str,
        objective: &str,
        value: i32,
        display: Option<&Component>,
        force_writable: bool,
        requires_sync: bool,
    ) -> Result<()> {
        let old = self.data.get_player_score(holder, objective).cloned();
        self.scoreboard_mut()
            .set_score(holder, objective, value, display, force_writable)?;
        let new = self.data.get_player_score(holder, objective).unwrap();
        if (requires_sync || old.as_ref() != Some(new)) && self.tracked.contains(objective) {
            self.packets.push(score_packet(holder, objective, new));
        }
        Ok(())
    }
    /// Port of ScoreAccess display/format setters. Format writes always sync, even if equal.
    pub fn update_score(
        &mut self,
        holder: &str,
        objective: &str,
        update: impl FnOnce(&mut Score),
        force_sync: bool,
    ) -> Result<()> {
        let old = self.data.get_player_score(holder, objective).cloned();
        update(
            self.scoreboard_mut()
                .get_or_create_score(holder, objective)?,
        );
        let new = self.data.get_player_score(holder, objective).unwrap();
        if (force_sync || old.as_ref() != Some(new)) && self.tracked.contains(objective) {
            self.packets.push(score_packet(holder, objective, new));
        }
        Ok(())
    }
    /// Port of ScoreAccess#setLocked: an existing score's lock change is persisted without a score packet.
    pub fn set_score_locked(&mut self, holder: &str, objective: &str, locked: bool) -> Result<()> {
        let new = self.data.get_player_score(holder, objective).is_none();
        self.scoreboard_mut()
            .get_or_create_score(holder, objective)?
            .locked = locked;
        if new && self.tracked.contains(objective) {
            self.packets.push(score_packet(
                holder,
                objective,
                self.data.get_player_score(holder, objective).unwrap(),
            ));
        }
        Ok(())
    }
    pub fn reset_score(&mut self, holder: &str, objective: Option<&str>) {
        if !self.data.scores.contains_key(&holder.to_owned()) {
            return;
        }
        let had_score =
            objective.is_some_and(|obj| self.data.get_player_score(holder, obj).is_some());
        self.scoreboard_mut().reset_score(holder, objective);
        if !self.data.scores.contains_key(&holder.to_owned()) {
            if !self.tracked.is_empty() {
                self.packets.push(cb::Packet::ResetScore(cb::ResetScore {
                    owner: holder.to_owned(),
                    objective_name: None,
                }));
            }
        } else if had_score && objective.is_some_and(|obj| self.tracked.contains(obj)) {
            self.packets.push(cb::Packet::ResetScore(cb::ResetScore {
                owner: holder.to_owned(),
                objective_name: objective.map(str::to_owned),
            }));
        }
    }
    /// Port of Scoreboard#forAllObjectives and ServerPlayer's criterion setters.
    /// The entity owner publishes snapshot() after its synchronous update batch.
    pub fn set_criterion_score(
        &mut self,
        holder: &str,
        criterion: &str,
        value: i32,
        display: Option<&Component>,
    ) -> Result<()> {
        let objectives: Vec<_> = self
            .data
            .objectives_for_criterion(criterion)
            .map(str::to_owned)
            .collect();
        for name in objectives {
            self.set_score(holder, &name, value, display, true)?;
        }
        Ok(())
    }
    /// Port of Scoreboard#forAllObjectives with ScoreAccess#add (Java wrapping arithmetic).
    pub fn add_criterion_score(
        &mut self,
        holder: &str,
        criterion: &str,
        delta: i32,
        display: Option<&Component>,
    ) -> Result<()> {
        let objectives: Vec<_> = self
            .data
            .objectives_for_criterion(criterion)
            .map(str::to_owned)
            .collect();
        for name in objectives {
            let old = self
                .data
                .get_player_score(holder, &name)
                .map_or(0, |score| score.value);
            self.set_score(holder, &name, old.wrapping_add(delta), display, true)?;
        }
        Ok(())
    }
    /// Port of ServerPlayer#awardStat. The stat counter saturates; the linked score adds the original delta.
    pub fn award_stat(
        &mut self,
        stats: &mut crate::stats::ServerStatsCounter,
        holder: &str,
        stat: crate::stats::Stat,
        delta: i32,
        display: Option<&Component>,
    ) -> Result<()> {
        stats.increment(stat, delta);
        self.add_criterion_score(
            holder,
            &format!(
                "{}:{}",
                stat.stat_type().name().replace(':', "."),
                stat.value_name().replace(':', ".")
            ),
            delta,
            display,
        )
    }
    /// Port of ServerPlayer#resetStat. Reset scores, without removing their records or trigger lock state.
    pub fn reset_stat(
        &mut self,
        stats: &mut crate::stats::ServerStatsCounter,
        holder: &str,
        stat: crate::stats::Stat,
        display: Option<&Component>,
    ) -> Result<()> {
        stats.set_value(stat, 0);
        self.set_criterion_score(
            holder,
            &format!(
                "{}:{}",
                stat.stat_type().name().replace(':', "."),
                stat.value_name().replace(':', ".")
            ),
            0,
            display,
        )
    }
}
fn objective_packet(name: &str, objective: &Objective, method: u8) -> cb::Packet {
    let operation = match method {
        0 => ObjectiveOperation::Add {
            display_name: objective.display_name.clone(),
            render_type: objective.render_type,
            number_format: objective.number_format.clone(),
        },
        1 => ObjectiveOperation::Remove {},
        2 => ObjectiveOperation::Change {
            display_name: objective.display_name.clone(),
            render_type: objective.render_type,
            number_format: objective.number_format.clone(),
        },
        _ => unreachable!(),
    };
    cb::Packet::SetObjective(Box::new(cb::SetObjective {
        objective_name: name.to_owned(),
        operation,
    }))
}
fn display_packet(index: usize, objective: Option<&str>) -> cb::Packet {
    let slot = save::enum_value(
        crate::commands::arguments::scoreboard::DISPLAY_SLOTS[index],
        crate::commands::arguments::scoreboard::DISPLAY_SLOTS,
    )
    .expect("display slot");
    cb::Packet::SetDisplayObjective(cb::SetDisplayObjective {
        slot,
        objective_name: objective.unwrap_or_default().to_owned(),
    })
}
fn score_packet(holder: &str, objective: &str, score: &Score) -> cb::Packet {
    cb::Packet::SetScore(Box::new(cb::SetScore {
        owner: holder.to_owned(),
        objective_name: objective.to_owned(),
        score: VarInt(score.value),
        display: score.display.clone(),
        number_format: score.number_format.clone(),
    }))
}
fn team_packet(team: &PlayerTeam, method: u8) -> cb::Packet {
    let parameters = || TeamParameters {
        display_name: team.display_name.clone(),
        player_prefix: team.prefix.clone(),
        player_suffix: team.suffix.clone(),
        name_tag_visibility: team.name_tag_visibility,
        collision_rule: team.collision_rule,
        color: team.color,
        options: team.pack_options(),
    };
    let operation = match method {
        0 => TeamOperation::Add {
            parameters: parameters(),
            players: team.members().cloned().collect(),
        },
        1 => TeamOperation::Remove {},
        2 => TeamOperation::Change {
            parameters: parameters(),
        },
        _ => unreachable!(),
    };
    cb::Packet::SetPlayerTeam(Box::new(cb::SetPlayerTeam {
        name: team.name().to_owned(),
        operation,
    }))
}
fn member_packet(team: &str, holder: &str, add: bool) -> cb::Packet {
    let players = vec![holder.to_owned()];
    let operation = if add {
        TeamOperation::Join { players }
    } else {
        TeamOperation::Leave { players }
    };
    cb::Packet::SetPlayerTeam(Box::new(cb::SetPlayerTeam {
        name: team.to_owned(),
        operation,
    }))
}
