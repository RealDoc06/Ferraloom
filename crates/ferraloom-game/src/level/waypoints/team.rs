//! ServerScoreboard's synchronous waypoint notifications. Each primitive ends
//! the shared-owner borrow before sending its Team packet and remaking connections.
use super::*;
use crate::{
    commands::team::{TeamError, check_scoreboard},
    scoreboard::PlayerTeam,
};
type Result<T> = std::result::Result<T, TeamError>;

impl Level {
    fn team_member_changed(&mut self, holder: &str) -> std::result::Result<(), LevelError> {
        self.emit_scoreboard_packets()?;
        // Pending multi-dimension ownership: ServerScoreboard#updatePlayerWaypoint
        // uses global PlayerList lookup and the player's actual level;
        // updateTeamWaypoints visits every server level for each online member.
        let player = self.all_entities().into_iter().find(|&entity| {
            self.entity::<ServerPlayer>(entity)
                .is_ok_and(|player| player.name.eq_ignore_ascii_case(holder))
        });
        if let Some(player) = player {
            self.remake_waypoint_connections(player)?;
        }
        Ok(())
    }
    /// Port of ServerScoreboard#addPlayerToTeam's per-member callbacks. Removing
    /// an old team and remaking its icon completes before adding the new team.
    pub fn add_team_member_live(&mut self, holder: &str, team: &str) -> Result<bool> {
        if self
            .shared()?
            .scoreboard
            .scoreboard()
            .get_player_team(team)
            .is_none()
        {
            check_scoreboard::<()>(Err(crate::scoreboard::Error::UnknownTeam(team.to_owned())))?;
        }
        self.remove_team_member_live(holder)?;
        let added = check_scoreboard(
            self.shared_mut()?
                .scoreboard
                .add_player_to_team(holder, team),
        )?;
        if added {
            self.team_member_changed(holder)?;
        }
        Ok(added)
    }
    /// Port of ServerScoreboard#removePlayerFromTeam.
    pub fn remove_team_member_live(&mut self, holder: &str) -> Result<bool> {
        let changed = self
            .shared_mut()?
            .scoreboard
            .remove_player_from_team(holder);
        if changed {
            self.team_member_changed(holder)?;
        }
        Ok(changed)
    }
    /// Port of ServerScoreboard#onTeamChanged and updateTeamWaypoints.
    pub fn update_team_live(
        &mut self,
        team: &str,
        update: impl FnOnce(&mut PlayerTeam),
    ) -> Result<()> {
        check_scoreboard(self.shared_mut()?.scoreboard.update_team(team, update))?;
        let members: Vec<_> = self
            .shared()?
            .scoreboard
            .scoreboard()
            .get_player_team(team)
            .expect("updated team")
            .members()
            .cloned()
            .collect();
        self.emit_scoreboard_packets()?;
        for holder in members {
            self.team_member_changed(&holder)?;
        }
        Ok(())
    }
    /// Port of ServerScoreboard#onTeamRemoved. Retain the removed team's native
    /// member iteration, while each callback observes membership already removed.
    pub fn remove_team_live(&mut self, team: &str) -> Result<()> {
        let members: Vec<_> = self
            .shared()?
            .scoreboard
            .scoreboard()
            .get_player_team(team)
            .map(|team| team.members().cloned().collect())
            .unwrap_or_default();
        check_scoreboard(self.shared_mut()?.scoreboard.remove_player_team(team))?;
        self.emit_scoreboard_packets()?;
        for holder in members {
            self.team_member_changed(&holder)?;
        }
        Ok(())
    }
}
