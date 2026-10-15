use super::chat_message::is_chat_message_illegal;
#[allow(clippy::wildcard_imports)]
use super::*;

impl JavaClient {
    pub async fn handle_chat_command(
        &self,
        player: &Arc<Player>,
        server: &Arc<Server>,
        command: &SChatCommand<'_>,
    ) {
        // tryHandleChat rejects illegal text before dispatch or spam accounting.
        if is_chat_message_illegal(command.command) {
            self.kick(TextComponent::translate_cross(
                translation::java::MULTIPLAYER_DISCONNECT_ILLEGAL_CHARACTERS,
                translation::java::MULTIPLAYER_DISCONNECT_ILLEGAL_CHARACTERS,
                [],
            ))
            .await;
            return;
        }
        player.update_last_action_time();
        let command_str = command.command.strip_prefix('/').unwrap_or(command.command);
        send_cancellable! {{
            server;
            PlayerCommandSendEvent {
                player: player.clone(),
                command: command_str.to_string(),
                cancelled: false
            };

            'after: {
                let command = event.command;
                let dispatcher = server.command_dispatcher.load();
                dispatcher.handle_command(
                    &player.get_command_source(server),
                    &command,
                );

                if server.advanced_config.commands.log_console {
                    info!(
                        "Player ({}): executed command /{}",
                        player.gameprofile.name,
                        command
                    );
                }
            }
        }}

        // Both unsigned and signed packets reach this handler. Vanilla charges
        // after dispatch, even when parsing/execution returns a command error.
        player.check_chat_spam(server, crate::entity::player::SpamType::Command);
    }
}
