use super::Scoreboard;
use ferraloom_protocol::chat::Component;
pub use ferraloom_protocol::play::{
    TeamCollisionRule as CollisionRule, TeamColor, TeamVisibility as Visibility,
};
use ferraloom_util::{collections::HashSet, uuid::Uuid};
use std::borrow::Cow;

/// Port of PlayerTeam. Membership and identity cannot be modified outside Scoreboard.
#[derive(Clone, Debug)]
pub struct PlayerTeam {
    name: String,
    pub display_name: Component,
    pub prefix: Component,
    pub suffix: Component,
    pub color: Option<TeamColor>,
    pub allow_friendly_fire: bool,
    pub see_friendly_invisibles: bool,
    pub name_tag_visibility: Visibility,
    pub death_message_visibility: Visibility,
    pub collision_rule: CollisionRule,
    pub(super) members: HashSet<String>,
}
impl PlayerTeam {
    pub(super) fn new(name: String) -> Self {
        Self {
            display_name: Component::literal(&name),
            name,
            prefix: Component::default(),
            suffix: Component::default(),
            color: None,
            allow_friendly_fire: true,
            see_friendly_invisibles: true,
            name_tag_visibility: Visibility::Always,
            death_message_visibility: Visibility::Always,
            collision_rule: CollisionRule::Always,
            members: HashSet::new(),
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn members(&self) -> impl ExactSizeIterator<Item = &String> {
        self.members.iter()
    }
    /// Port of Team#isAlliedTo. Team identity is scoped to a single scoreboard snapshot.
    pub fn is_allied_to(&self, other: Option<&Self>) -> bool {
        other.is_some_and(|other| std::ptr::eq(self, other))
    }
    pub fn pack_options(&self) -> u8 {
        u8::from(self.allow_friendly_fire) | (u8::from(self.see_friendly_invisibles) << 1)
    }
    pub fn unpack_options(&mut self, options: u8) {
        self.allow_friendly_fire = options & 1 != 0;
        self.see_friendly_invisibles = options & 2 != 0;
    }
    /// Port of PlayerTeam#getFormattedDisplayName (brackets, insertion and hover on the display name).
    pub fn formatted_display_name(&self) -> Component {
        use ferraloom_protocol::chat::{Argument, HoverEvent, TextColor};
        let mut display = self.display_name.clone();
        display.style.insertion = Some(self.name.clone());
        display.style.hover_event = Some(HoverEvent::ShowText(Box::new(Component::literal(
            &self.name,
        ))));
        let mut result = Component::translatable(
            "chat.square_brackets",
            vec![Argument::Component(Box::new(display))],
        );
        if let Some(color) = self.color {
            result.style.color = Some(
                TextColor::parse(crate::commands::arguments::scoreboard::COLORS[color as usize])
                    .expect("registered team color"),
            );
        }
        result
    }
    /// Port of PlayerTeam#getFormattedName, retaining component style inheritance.
    pub fn formatted_name(&self, name: Component) -> Component {
        let mut result = Component {
            extra: vec![self.prefix.clone(), name, self.suffix.clone()],
            ..Component::default()
        };
        if let Some(color) = self.color {
            result.style.color = Some(
                ferraloom_protocol::chat::TextColor::parse(
                    crate::commands::arguments::scoreboard::COLORS[color as usize],
                )
                .expect("registered team color"),
            );
        }
        result
    }
}
/// Port of Player#getScoreboardName and Entity#getScoreboardName. Custom entity names do not enter team lookup.
pub fn scoreboard_name(player_name: Option<&str>, uuid: Uuid) -> Cow<'_, str> {
    player_name.map_or_else(|| Cow::Owned(uuid.to_string()), Cow::Borrowed)
}
/// Port of Entity#getTeam. Callers supply the actual player name, or None for a non-player.
pub fn entity_team<'a>(
    scoreboard: &'a Scoreboard,
    player_name: Option<&str>,
    uuid: Uuid,
) -> Option<&'a PlayerTeam> {
    scoreboard.get_players_team(&scoreboard_name(player_name, uuid))
}
/// Port of Entity#isAlliedTo's base implementation. Specialized considersEntityAsAlly overrides remain the entity dispatcher's responsibility.
pub fn entity_is_allied_to(
    scoreboard: &Scoreboard,
    own_name: &str,
    other_name: Option<&str>,
    same_entity: bool,
) -> bool {
    other_name.is_some_and(|other| {
        same_entity
            || scoreboard
                .get_players_team(own_name)
                .is_some_and(|team| team.is_allied_to(scoreboard.get_players_team(other)))
    })
}
/// Team portion of EntitySelector#pushableBy on the server. Caller checks spectator/isPushable first.
pub fn can_collide(own: Option<&PlayerTeam>, other: Option<&PlayerTeam>) -> bool {
    let a = own.map_or(CollisionRule::Always, |team| team.collision_rule);
    let b = other.map_or(CollisionRule::Always, |team| team.collision_rule);
    let allied = own.is_some_and(|team| team.is_allied_to(other));
    if a == CollisionRule::Never || b == CollisionRule::Never {
        return false;
    }
    if (a == CollisionRule::PushOwnTeam || b == CollisionRule::PushOwnTeam) && allied {
        return false;
    }
    (a != CollisionRule::PushOtherTeams && b != CollisionRule::PushOtherTeams) || allied
}

/// Read projection used by EntitySelector#pushableBy. Borrowed team references are Copy handles.
/// The source handle may be captured when the predicate is built; targets resolve on each call.
pub trait TeamRead {
    type Team: Copy;
    fn team_by_scoreboard_name(&self, name: &str) -> Option<Self::Team>;
    fn collision_rule(&self, team: Self::Team) -> CollisionRule;
    fn is_allied_to(&self, team: Self::Team, other: Option<Self::Team>) -> bool;
}
impl<'a> TeamRead for &'a Scoreboard {
    type Team = &'a PlayerTeam;
    fn team_by_scoreboard_name(&self, name: &str) -> Option<Self::Team> {
        self.get_players_team(name)
    }
    fn collision_rule(&self, team: Self::Team) -> CollisionRule {
        team.collision_rule
    }
    fn is_allied_to(&self, team: Self::Team, other: Option<Self::Team>) -> bool {
        team.is_allied_to(other)
    }
}
