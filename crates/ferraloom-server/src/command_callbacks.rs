//! A short live-server borrow at command callbacks, including callbacks fired
//! inside frame control. Feedback is sent now, never recorded for a later drain.
use super::*;
use ferraloom_game::commands::{
    execution::{CommandExecutionHost, PendingCommand},
    source::CommandCallbackHost,
};

pub(in crate::server) struct ServerCommandHost<'a> {
    pub server: &'a mut MinecraftServer,
    pub pending: Option<PendingCommand>,
    connection: Option<ConnectionId>,
}
impl<'a> ServerCommandHost<'a> {
    pub fn new(server: &'a mut MinecraftServer, connection: Option<ConnectionId>) -> Self {
        Self {
            server,
            pending: None,
            connection,
        }
    }
}
impl CommandCallbackHost for ServerCommandHost<'_> {
    fn send_success(&mut self, source: &CommandSourceStack, message: Component, broadcast: bool) {
        self.server
            .command_success(self.connection, source, message, broadcast);
    }
}
impl CommandExecutionHost for ServerCommandHost<'_> {
    fn emit_command(&mut self, command: PendingCommand) {
        self.pending = Some(command);
    }
}
