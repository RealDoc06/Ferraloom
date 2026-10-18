//! Port of MessageArgument#resolveChatMessage and OutgoingChatMessage#create.
use super::source::{CommandSigningContext, SignedArgument};
use ferraloom_protocol::chat::Component;
use ferraloom_util::Uuid;

/// Native chat types used by SayCommand and EmoteCommands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BroadcastChatType {
    Say,
    Emote,
}
impl BroadcastChatType {
    pub fn registry_key(self) -> &'static str {
        match self {
            Self::Say => "say_command",
            Self::Emote => "emote_command",
        }
    }
    /// Port of PlayerList#verifyChatTrusted and MinecraftServer#logChatMessage.
    pub fn console_line(
        self,
        name: Component,
        content: Component,
        signed_timestamp: Option<i64>,
        now_nanos: i128,
    ) -> String {
        let key = match self {
            Self::Say => "chat.type.announcement",
            Self::Emote => "chat.type.emote",
        };
        let decorated = super::exceptions::plain_text(&Component::translatable(
            key,
            vec![
                super::builtins::component(name),
                super::builtins::component(content),
            ],
        ));
        let trusted = signed_timestamp.is_some_and(|timestamp| {
            now_nanos <= i128::from(timestamp) * 1_000_000 + 300_000_000_000
        });
        if trusted {
            decorated
        } else {
            format!("[Not Secure] {decorated}")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutgoingChatKind {
    Disguised,
    Player,
}
impl CommandSigningContext {
    pub fn argument(&self, name: &str) -> Option<&SignedArgument> {
        self.arguments
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, argument)| argument)
    }
    /// An argument may be present without a signature. Only absence takes the system path.
    pub fn outgoing_chat_kind(&self, name: &str) -> OutgoingChatKind {
        if self
            .argument(name)
            .is_some_and(|argument| argument.sender != Uuid::NIL)
        {
            OutgoingChatKind::Player
        } else {
            OutgoingChatKind::Disguised
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn broadcast_console_decoration_and_signature_expiry_match_java() {
        #[derive(serde::Deserialize)]
        struct Vector {
            command: String,
            signed: bool,
            timestamp: i64,
            #[serde(rename = "nowNanos")]
            now_nanos: i128,
            line: String,
        }
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("vanilla/command-chat-log-vectors.json")).unwrap();
        for vector in vectors {
            let chat_type = if vector.command == "say" {
                BroadcastChatType::Say
            } else {
                BroadcastChatType::Emote
            };
            assert_eq!(
                chat_type.console_line(
                    Component::literal("Doc"),
                    Component::literal("hello 👋"),
                    vector.signed.then_some(vector.timestamp),
                    vector.now_nanos,
                ),
                vector.line
            );
        }
    }
    #[test]
    fn unsigned_player_argument_and_absent_argument_match_java_packet_variants() {
        let expected: std::collections::BTreeMap<String, String> =
            serde_json::from_str(include_str!("vanilla/message-variants.json")).unwrap();
        let anonymous = CommandSigningContext::default();
        let unsigned = CommandSigningContext {
            arguments: Arc::from([(
                "message".into(),
                SignedArgument {
                    sender: Uuid::new(1, 2),
                    index: 0,
                    signature: None,
                    content: "hello".into(),
                    timestamp: 123,
                    salt: 17,
                    last_seen: Arc::from([]),
                },
            )]),
        };
        for (context, key) in [
            (&anonymous, "absent_argument"),
            (&unsigned, "present_unsigned_argument"),
        ] {
            assert_eq!(
                format!("{:?}", context.outgoing_chat_kind("message")),
                expected[key]
            );
        }
        assert_eq!(
            format!("{:?}", unsigned.clone().outgoing_chat_kind("message")),
            expected["present_unsigned_argument_again"]
        );
        let argument = unsigned.argument("message").unwrap();
        assert_eq!(
            (&*argument.content, argument.timestamp, argument.salt),
            ("hello", 123, 17)
        );
        assert_eq!(
            unsigned.outgoing_chat_kind("another_argument"),
            OutgoingChatKind::Disguised
        );
    }
}
