use super::PlainTextFunction;
use crate::commands::{
    CommandSourceStack,
    execution::{CommandExecutionHost, CommandQueueEntry, ExecutionContext, Frame, queue_command},
    source::CommandResultCallback,
};
use std::sync::Arc;

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
