use super::{CollectorId, CompiledFunction, PlainTextFunction};
use crate::commands::{
    CommandSourceStack,
    builtins::{component, integer},
    execution::{CommandExecutionHost, CommandQueueEntry, ExecutionContext, Frame, queue_command},
    permissions::PermissionSet,
    source::CommandResultCallback,
};
use ferraloom_protocol::chat::Component;
use std::sync::Arc;

pub trait FunctionExecutionHost: CommandExecutionHost {
    fn create_function_collector(&mut self) -> CollectorId;
    fn take_function_result(&mut self, collector: CollectorId) -> Option<i32>;
}

#[derive(Debug)]
pub struct UnsupportedMacroFunction(pub ferraloom_data::datapack::Identifier);
impl std::fmt::Display for UnsupportedMacroFunction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "MacroFunction not yet supported: {}", self.0)
    }
}
impl std::error::Error for UnsupportedMacroFunction {}

/// Port of FunctionCommand#queueFunctions. Result aggregation is native mutable
/// state in the host; the callback itself captures only its typed collector id.
pub fn queue_functions<H: FunctionExecutionHost>(
    context: &mut ExecutionContext<H>,
    frame: Frame,
    functions: &[Arc<CompiledFunction>],
    original: CommandSourceStack,
    returning: bool,
    host: &mut H,
) -> Result<(), UnsupportedMacroFunction> {
    let source = original
        .clone()
        .with_suppressed_output()
        .with_maximum_permission(PermissionSet::for_level(2))
        .with_callback(CommandResultCallback::default());
    let collector = (!returning && functions.len() > 1 && !original.callback.is_empty())
        .then(|| host.create_function_collector());
    let callback = if returning {
        original
            .callback
            .clone()
            .chain(frame.return_value_consumer.clone())
    } else if let Some(collector) = collector {
        CommandResultCallback::new(move |host, _, value| host.add_function_result(collector, value))
    } else {
        original.callback.clone()
    };
    for function in functions {
        let callback = if original.silent {
            callback.clone()
        } else {
            let callback = callback.clone();
            let original = original.clone();
            let id = function.id().to_string();
            CommandResultCallback::new(move |host, success, value| {
                host.send_success(
                    &original,
                    Component::translatable(
                        "commands.function.result",
                        vec![component(Component::literal(&id)), integer(value)],
                    ),
                    true,
                );
                callback.on_result(host, success, value);
            })
        };
        let CompiledFunction::Plain(function) = function.as_ref() else {
            return Err(UnsupportedMacroFunction(function.id().clone()));
        };
        queue_function(
            context,
            frame.clone(),
            function.clone(),
            source.clone(),
            callback,
            returning,
        );
    }
    if returning {
        context.queue_next(CommandQueueEntry::new(
            frame,
            |context, frame, host: &mut H| {
                frame.return_failure(host);
                context.discard(frame);
            },
        ));
    } else if let Some(collector) = collector {
        context.queue_next(CommandQueueEntry::new(frame, move |_, _, host: &mut H| {
            if let Some(value) = host.take_function_result(collector) {
                original.callback.on_success(host, value);
            }
        }));
    }
    Ok(())
}

/// Port of CallFunction. Function entry and body execution share the caller's queue.
pub fn queue_function<H: CommandExecutionHost>(
    context: &mut ExecutionContext<H>,
    frame: Frame,
    function: Arc<PlainTextFunction>,
    source: CommandSourceStack,
    callback: CommandResultCallback,
    return_parent: bool,
) {
    context.queue_next(CommandQueueEntry::new(frame, move |context, frame, _| {
        context.increment_cost();
        let child = frame.child(callback, return_parent);
        match function.entries.len() {
            0 => {}
            1 | 2 => {
                for entry in function.entries.iter() {
                    let mut command = entry.clone();
                    command.source = source.clone();
                    queue_command(context, child.clone(), command, false);
                }
            }
            _ => queue_continuation(context, child, function, source, 0),
        }
    }));
}

/// Port of ContinuationTask: retain the next index instead of expanding the body
/// eagerly, so frame discard and the command-chain limit see native queue entries.
fn queue_continuation<H: CommandExecutionHost>(
    context: &mut ExecutionContext<H>,
    frame: Frame,
    function: Arc<PlainTextFunction>,
    source: CommandSourceStack,
    index: usize,
) {
    context.queue_next(CommandQueueEntry::new(frame, move |context, frame, _| {
        let mut command = function.entries[index].clone();
        command.source = source.clone();
        queue_command(context, frame.clone(), command, false);
        if index + 1 < function.entries.len() {
            queue_continuation(context, frame.clone(), function, source, index + 1);
        }
    }));
}
