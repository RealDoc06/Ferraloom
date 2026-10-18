//! Port of the 26.3 execution queue. All actions run inline on the exclusive caller.
use super::{exceptions::error, source::CommandResultCallback};
use ferraloom_brigadier::CommandSyntaxException;
use std::collections::VecDeque;

/// Port of Frame. Discard control is a depth, avoiding callbacks borrowing the queue.
#[derive(Clone, Debug)]
pub struct Frame {
    pub depth: usize,
    pub return_value_consumer: CommandResultCallback,
    discard_all: bool,
}
impl Frame {
    pub fn return_success(&self, value: i32) {
        self.return_value_consumer.on_success(value);
    }
    pub fn return_failure(&self) {
        self.return_value_consumer.on_failure();
    }
}
type EntryAction<H> = Box<dyn FnOnce(&mut ExecutionContext<H>, &Frame, &mut H) + Send>;
/// Port of CommandQueueEntry. The action owns its source and arguments.
pub struct CommandQueueEntry<H> {
    pub frame: Frame,
    action: EntryAction<H>,
}
impl<H> CommandQueueEntry<H> {
    pub fn new(
        frame: Frame,
        action: impl FnOnce(&mut ExecutionContext<H>, &Frame, &mut H) + Send + 'static,
    ) -> Self {
        Self {
            frame,
            action: Box::new(action),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueResult {
    Complete,
    CommandLimit,
    QueueOverflow,
}
/// Port of ExecutionContext, including next-command insertion and reentrant frame depths.
pub struct ExecutionContext<H> {
    command_limit: i32,
    pub fork_limit: i32,
    command_quota: i32,
    queue_overflow: bool,
    queue: VecDeque<CommandQueueEntry<H>>,
    new_top_commands: Vec<CommandQueueEntry<H>>,
    current_frame_depth: usize,
}
impl<H> ExecutionContext<H> {
    pub const MAX_QUEUE_DEPTH: usize = 10_000_000;
    pub fn new(command_limit: i32, fork_limit: i32) -> Self {
        Self {
            command_limit,
            fork_limit,
            command_quota: command_limit,
            queue_overflow: false,
            queue: VecDeque::new(),
            new_top_commands: Vec::new(),
            current_frame_depth: 0,
        }
    }
    pub fn command_limit(&self) -> i32 {
        self.command_limit
    }
    pub fn remaining_quota(&self) -> i32 {
        self.command_quota
    }
    pub fn increment_cost(&mut self) {
        self.command_quota -= 1;
    }
    pub fn top_frame(&self, callback: CommandResultCallback) -> Frame {
        if self.current_frame_depth == 0 {
            Frame {
                depth: 0,
                return_value_consumer: callback,
                discard_all: true,
            }
        } else {
            Frame {
                depth: self.current_frame_depth + 1,
                return_value_consumer: callback,
                discard_all: false,
            }
        }
    }
    pub fn queue_next(&mut self, entry: CommandQueueEntry<H>) {
        if self.new_top_commands.len() + self.queue.len() > Self::MAX_QUEUE_DEPTH {
            self.queue_overflow = true;
            self.new_top_commands.clear();
            self.queue.clear();
        }
        if !self.queue_overflow {
            self.new_top_commands.push(entry);
        }
    }
    pub fn discard_at_depth_or_higher(&mut self, depth: usize) {
        while self
            .queue
            .front()
            .is_some_and(|entry| entry.frame.depth >= depth)
        {
            self.queue.pop_front();
        }
    }
    pub fn discard(&mut self, frame: &Frame) {
        if frame.discard_all {
            self.queue.clear();
        } else {
            self.discard_at_depth_or_higher(frame.depth);
        }
    }
    fn push_new_commands(&mut self) {
        for command in self.new_top_commands.drain(..).rev() {
            self.queue.push_front(command);
        }
    }
    /// Drives one queue entry. The server may await an existing synchronous IO contract
    /// before requesting the next entry, while retaining exclusive world ownership.
    pub fn run_next(&mut self, host: &mut H) -> Result<bool, QueueResult> {
        self.push_new_commands();
        if self.command_quota <= 0 {
            self.current_frame_depth = 0;
            return Err(QueueResult::CommandLimit);
        }
        let Some(entry) = self.queue.pop_front() else {
            return Ok(false);
        };
        self.current_frame_depth = entry.frame.depth;
        (entry.action)(self, &entry.frame, host);
        if self.queue_overflow {
            self.current_frame_depth = 0;
            return Err(QueueResult::QueueOverflow);
        }
        self.push_new_commands();
        Ok(true)
    }
    pub fn run_command_queue(&mut self, host: &mut H) -> QueueResult {
        loop {
            match self.run_next(host) {
                Ok(true) => {}
                Ok(false) => return QueueResult::Complete,
                Err(result) => return result,
            }
        }
    }
    /// Port of BuildContexts' >= fork-limit check, before adding each captured expansion.
    pub fn append_forks<T>(
        &self,
        sources: &mut Vec<T>,
        new_sources: impl IntoIterator<Item = T>,
    ) -> Result<(), CommandSyntaxException> {
        let new_sources = new_sources.into_iter().collect::<Vec<_>>();
        if sources.len() + new_sources.len() >= self.fork_limit.max(0) as usize {
            return Err(error("command.forkLimit", &[self.fork_limit.to_string()]));
        }
        sources.extend(new_sources);
        Ok(())
    }
    /// Port of ReturnValueCustomExecutor and ReturnFailCustomExecutor.
    pub fn return_value(
        &mut self,
        frame: &Frame,
        source_callback: &CommandResultCallback,
        value: Option<i32>,
    ) {
        if let Some(value) = value {
            source_callback.on_success(value);
            frame.return_success(value);
        } else {
            source_callback.on_failure();
            frame.return_failure();
        }
        self.discard(frame);
    }
}
/// Port of BuildContexts' top-level/continuation and ReturnCommand's custom executors.
/// World mutations remain in the host, after this entry emits its owned command.
pub fn queue_command(
    context: &mut ExecutionContext<Option<super::PreparedCommand>>,
    frame: Frame,
    mut command: super::PreparedCommand,
    returning: bool,
) {
    context.queue_next(CommandQueueEntry::new(
        frame,
        move |context, frame, _pending| {
            if command.command == "return" {
                if let Some(continuation) = command.continuation.take() {
                    context.discard(frame);
                    let mut continuation = *continuation;
                    continuation.source = command.source;
                    queue_command(context, frame.clone(), continuation, true);
                } else {
                    context.return_value(frame, &command.source.callback, command.integer("value"));
                }
            } else {
                if returning {
                    command.source = command
                        .source
                        .with_combined_callback(frame.return_value_consumer.clone());
                }
                context.queue_next(CommandQueueEntry::new(
                    frame.clone(),
                    move |context, _, pending| {
                        context.increment_cost();
                        *pending = Some(command);
                    },
                ));
            }
        },
    ));
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[test]
    fn next_commands_run_before_siblings_in_insertion_order() {
        let mut context = ExecutionContext::new(10, 10);
        let frame = context.top_frame(CommandResultCallback::default());
        let child_frame = frame.clone();
        context.queue_next(CommandQueueEntry::new(
            frame.clone(),
            move |context, _, host: &mut Vec<i32>| {
                host.push(1);
                context.increment_cost();
                context.queue_next(CommandQueueEntry::new(
                    child_frame.clone(),
                    |context, _, host| {
                        host.push(2);
                        context.increment_cost();
                    },
                ));
                context.queue_next(CommandQueueEntry::new(child_frame, |context, _, host| {
                    host.push(3);
                    context.increment_cost();
                }));
            },
        ));
        context.queue_next(CommandQueueEntry::new(frame, |context, _, host| {
            host.push(4);
            context.increment_cost();
        }));
        let mut host = Vec::new();
        assert_eq!(context.run_command_queue(&mut host), QueueResult::Complete);
        assert_eq!(host, [1, 2, 3, 4]);
        assert_eq!(context.remaining_quota(), 6);
    }
    #[test]
    fn return_callbacks_run_inline_and_discard_pending_siblings() {
        let results = Arc::new(Mutex::new(Vec::new()));
        let callback_results = results.clone();
        let callback = CommandResultCallback::new(move |success, value| {
            callback_results.lock().unwrap().push((success, value))
        });
        let mut context = ExecutionContext::new(1, 2);
        let frame = context.top_frame(callback.clone());
        context.queue_next(CommandQueueEntry::new(
            frame.clone(),
            move |context, frame, host: &mut Vec<i32>| {
                context.increment_cost();
                context.return_value(frame, &callback, Some(7));
                host.push(7);
            },
        ));
        context.queue_next(CommandQueueEntry::new(frame, |_, _, host| host.push(99)));
        let mut host = Vec::new();
        assert_eq!(
            context.run_command_queue(&mut host),
            QueueResult::CommandLimit
        );
        assert_eq!(host, [7]);
        assert_eq!(*results.lock().unwrap(), [(true, 7), (true, 7)]);
        assert!(context.append_forks(&mut vec![0], vec![1]).is_err());
    }
}

#[cfg(test)]
mod command_tests {
    use super::*;
    use crate::commands::{CommandSourceStack, Commands};
    use ferraloom_util::Vec3;
    use std::sync::{Arc, Mutex};
    #[test]
    fn return_commands_share_the_live_queue_and_compose_callbacks() {
        let commands = Commands::default();
        for (input, value, emitted, cost) in [
            ("return 7", Some(7), false, 0),
            ("return fail", None, false, 0),
            ("return run seed", Some(41), true, 1),
            ("return run return 9", Some(9), false, 0),
        ] {
            let results = Arc::new(Mutex::new(Vec::new()));
            let source_results = results.clone();
            let frame_results = results.clone();
            let source = CommandSourceStack::server(4, Vec3::ZERO).with_callback(
                CommandResultCallback::new(move |success, value| {
                    source_results
                        .lock()
                        .unwrap()
                        .push(("source", success, value))
                }),
            );
            let command = commands.prepare(input, source).unwrap();
            let mut context = ExecutionContext::new(10, 10);
            let frame = context.top_frame(CommandResultCallback::new(move |success, value| {
                frame_results
                    .lock()
                    .unwrap()
                    .push(("frame", success, value))
            }));
            queue_command(&mut context, frame, command, false);
            let mut pending = None;
            let mut did_emit = false;
            while context.run_next(&mut pending) == Ok(true) {
                if let Some(command) = pending.take() {
                    did_emit = true;
                    assert_eq!(command.command, "seed");
                    command.source.callback.on_success(41);
                }
            }
            assert_eq!(did_emit, emitted, "{input}");
            assert_eq!(context.remaining_quota(), 10 - cost, "{input}");
            assert_eq!(
                *results.lock().unwrap(),
                [
                    ("source", value.is_some(), value.unwrap_or(0)),
                    ("frame", value.is_some(), value.unwrap_or(0))
                ],
                "{input}"
            );
        }
    }
}
