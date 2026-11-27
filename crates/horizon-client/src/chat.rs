/// `ChatVisiblity`: the Chat option's three values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatVisibility {
    Full,
    System,
    Hidden,
}
impl ChatVisibility {
    /// Options.chatVisibility's saved value (0 full, 1 system, 2 hidden).
    pub fn from_option(value: u8) -> Self {
        match value {
            0 => Self::Full,
            1 => Self::System,
            _ => Self::Hidden,
        }
    }
}

/// `ChatAbilities`: what the Chat option allows, as `Minecraft.computeChatAbilities`
/// derives it from the visibility alone (`ChatRestriction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatAbilities {
    pub send_messages: bool,
    pub send_commands: bool,
    pub receive_player_messages: bool,
    pub receive_system_messages: bool,
}
impl ChatAbilities {
    /// `ChatRestriction.CHAT_AND_COMMANDS_DISABLED_BY_OPTIONS` (Hidden) takes
    /// chat, commands, player messages and system messages;
    /// `CHAT_DISABLED_BY_OPTIONS` (System) takes the chat and player messages.
    pub fn new(visibility: ChatVisibility) -> Self {
        match visibility {
            ChatVisibility::Full => Self {
                send_messages: true,
                send_commands: true,
                receive_player_messages: true,
                receive_system_messages: true,
            },
            ChatVisibility::System => Self {
                send_messages: false,
                send_commands: true,
                receive_player_messages: false,
                receive_system_messages: true,
            },
            ChatVisibility::Hidden => Self {
                send_messages: false,
                send_commands: false,
                receive_player_messages: false,
                receive_system_messages: false,
            },
        }
    }
    /// `visibleMessagesFilter`: the client's own lines always show.
    pub fn shows(&self, source: ChatSource) -> bool {
        match source {
            ChatSource::Player => self.receive_player_messages,
            ChatSource::Server => self.receive_system_messages,
            ChatSource::Client => true,
        }
    }
}

impl Default for ChatAbilities {
    fn default() -> Self {
        Self::new(ChatVisibility::Full)
    }
}

/// Where a chat line came from (`GuiMessageSource`): the visibility filters
/// player messages and the server's, while the client's own always show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatSource {
    Player,
    Server,
    Client,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Commands Only hides player messages, Hidden the server's too; the client's own lines always show.
    #[test]
    fn the_chat_option_decides_what_shows() {
        for (visibility, player, server) in [
            (ChatVisibility::Full, true, true),
            (ChatVisibility::System, false, true),
            (ChatVisibility::Hidden, false, false),
        ] {
            let abilities = ChatAbilities::new(visibility);
            assert_eq!(
                abilities.shows(ChatSource::Player),
                player,
                "{visibility:?}"
            );
            assert_eq!(
                abilities.shows(ChatSource::Server),
                server,
                "{visibility:?}"
            );
            assert!(abilities.shows(ChatSource::Client), "{visibility:?}");
        }
    }
}
