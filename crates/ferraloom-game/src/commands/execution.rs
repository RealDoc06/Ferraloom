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
type EntryAction<H> = Box<dyn FnOnce(&mut ExecutionContext<H>, &Frame, &mut H)>;
/// Port of CommandQueueEntry. The action owns its source and arguments.
pub struct CommandQueueEntry<H> {
    pub frame: Frame,
    action: EntryAction<H>,
}
impl<H> CommandQueueEntry<H> {
    pub fn new(
        frame: Frame,
        action: impl FnOnce(&mut ExecutionContext<H>, &Frame, &mut H) + 'static,
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
    pub fn run_command_queue(&mut self, host: &mut H) -> QueueResult {
        self.push_new_commands();
        loop {
            if self.command_quota <= 0 {
                self.current_frame_depth = 0;
                return QueueResult::CommandLimit;
            }
            let Some(entry) = self.queue.pop_front() else {
                return QueueResult::Complete;
            };
            self.current_frame_depth = entry.frame.depth;
            (entry.action)(self, &entry.frame, host);
            if self.queue_overflow {
                self.current_frame_depth = 0;
                return QueueResult::QueueOverflow;
            }
            self.push_new_commands();
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
