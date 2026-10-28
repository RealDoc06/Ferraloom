//! A short live-server borrow at command callbacks, including callbacks fired
//! inside frame control. Feedback is sent now, never recorded for a later drain.
use super::*;
use ferraloom_game::commands::{
    execution::{CommandExecutionHost, PendingCommand},
    functions::{CollectorId, FunctionExecutionHost, FunctionScope},
    source::CommandCallbackHost,
};

pub(in crate::server) struct ServerCommandHost<'a> {
    pub server: &'a mut MinecraftServer,
    pub pending: Option<PendingCommand>,
    connection: Option<ConnectionId>,
    scope: Option<FunctionScope>,
}
impl<'a> ServerCommandHost<'a> {
    pub fn new(server: &'a mut MinecraftServer, connection: Option<ConnectionId>) -> Self {
        Self {
            server,
            pending: None,
            connection,
            scope: None,
        }
    }
    pub fn execution(server: &'a mut MinecraftServer, connection: Option<ConnectionId>) -> Self {
        let scope = server.functions.begin_execution();
        let mut host = Self::new(server, connection);
        host.scope = Some(scope);
        host
    }
}
impl Drop for ServerCommandHost<'_> {
    fn drop(&mut self) {
        if let Some(scope) = self.scope.take() {
            self.server.functions.end_execution(scope);
        }
    }
}
impl CommandCallbackHost for ServerCommandHost<'_> {
    fn add_function_result(&mut self, collector: CollectorId, value: i32) {
        self.server.functions.add_result(collector, value);
    }
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
impl FunctionExecutionHost for ServerCommandHost<'_> {
    fn create_function_collector(&mut self) -> CollectorId {
        self.server.functions.create_collector()
    }
    fn take_function_result(&mut self, collector: CollectorId) -> Option<i32> {
        self.server.functions.take_result(collector)
    }
}
