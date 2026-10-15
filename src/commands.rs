use std::sync::Arc;

use anyhow::{Context, Result};
use pumpkin_core::{command::context::command_context::CommandContext, server::Server};

/// Restore vanilla's console copy of `/say` without replacing its parser or broadcast.
pub fn install_console_chat_logging(server: &Arc<Server>) -> Result<()> {
    let mut dispatcher = (**server.command_dispatcher.load()).clone();
    let say = dispatcher
        .tree
        .get("say")
        .context("Engine is missing its built-in say command")?;
    let message = *dispatcher.tree[say]
        .children
        .get("message")
        .context("Engine say command is missing its message argument")?;
    let original = dispatcher.tree[message]
        .command()
        .clone()
        .context("Engine say command has no executor")?;

    dispatcher.tree[message].set_command(Some(Arc::new(move |context: &CommandContext<'_>| {
        let message = context.get_argument::<String>("message")?;
        tracing::info!(
            "[{}] {}",
            context.source.display_name.clone().to_pretty_console(),
            message
        );
        original.execute(context)
    })));
    server.command_dispatcher.store(Arc::new(dispatcher));
    Ok(())
}
