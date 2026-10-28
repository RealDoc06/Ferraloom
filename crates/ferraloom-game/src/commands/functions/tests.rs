use super::*;
use crate::commands::{CommandSourceStack, Commands};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
struct Vector {
    lines: Vec<String>,
    permission: i32,
    entries: Option<Vec<String>>,
    error: Option<String>,
}
#[test]
fn native_plain_function_compilation_lines_permissions_and_errors() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../tests/java/commands/functions/plain.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 23);
    let commands = Commands::default();
    for vector in vectors {
        let source = CommandSourceStack::server(vector.permission, ferraloom_util::Vec3::ZERO);
        let result = CompiledFunction::from_lines(
            "test:plain".parse().unwrap(),
            &commands,
            source,
            &vector.lines,
        );
        match result {
            Ok(CompiledFunction::Plain(function)) => assert_eq!(
                Some(
                    function
                        .entries
                        .iter()
                        .map(|entry| entry.input.clone())
                        .collect::<Vec<_>>()
                ),
                vector.entries,
                "{:?}",
                vector.lines
            ),
            Ok(CompiledFunction::MacroPending(_)) => panic!("plain vector became a macro"),
            Err(error) => assert_eq!(Some(error.message), vector.error, "{:?}", vector.lines),
        }
    }
}
#[test]
fn native_function_resource_utf8_replacement_and_line_terminators() {
    #[derive(Deserialize)]
    struct ReaderVector {
        bytes: Vec<u8>,
        lines: Vec<String>,
    }
    let vectors: Vec<ReaderVector> = serde_json::from_str(include_str!(
        "../../../tests/java/commands/functions/readers.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 13);
    for vector in vectors {
        assert_eq!(
            read_lines(&vector.bytes),
            vector.lines,
            "{:?}",
            vector.bytes
        );
    }
}

#[test]
fn native_function_calls_continuations_returns_and_quotas() {
    use crate::commands::{
        execution::{CommandExecutionHost, CommandQueueEntry, ExecutionContext, PendingCommand},
        source::{CommandCallbackHost, CommandResultCallback},
    };
    use ferraloom_protocol::chat::Component;
    use ferraloom_util::Vec3;
    #[derive(Default)]
    struct Host {
        events: Vec<String>,
        pending: Option<PendingCommand>,
    }
    impl CommandCallbackHost for Host {
        fn add_function_result(&mut self, _: crate::commands::functions::CollectorId, _: i32) {
            panic!("this vector does not accumulate function results");
        }
        fn send_success(&mut self, _: &CommandSourceStack, message: Component, _: bool) {
            self.events.push(crate::language::plain_text(&message));
        }
    }
    impl CommandExecutionHost for Host {
        fn emit_command(&mut self, command: PendingCommand) {
            self.pending = Some(command);
        }
    }
    #[derive(Deserialize)]
    struct CallVector {
        body: Vec<String>,
        parent: bool,
        quota: i32,
        events: Vec<String>,
        remaining: i32,
    }
    let vectors: Vec<CallVector> = serde_json::from_str(include_str!(
        "../../../tests/java/commands/functions/calls.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 42);
    let commands = Commands::default();
    for vector in vectors {
        let source = CommandSourceStack::server(4, Vec3::ZERO);
        let CompiledFunction::Plain(function) = CompiledFunction::from_lines(
            "test:call".parse().unwrap(),
            &commands,
            source.clone(),
            &vector.body,
        )
        .unwrap() else {
            panic!("plain call vector");
        };
        let mut context = ExecutionContext::<Host>::new(vector.quota, 10);
        let root = context.top_frame(CommandResultCallback::default());
        let callback_source = source.clone();
        let callback = CommandResultCallback::new(move |host, success, value| {
            host.send_success(
                &callback_source,
                Component::literal(format!("result:{success}:{value}")),
                false,
            );
        });
        queue_function(
            &mut context,
            root.clone(),
            function,
            source,
            callback,
            vector.parent,
        );
        context.queue_next(CommandQueueEntry::new(
            root,
            |context, _, host: &mut Host| {
                host.events.push("tail".into());
                context.increment_cost();
            },
        ));
        let mut host = Host::default();
        while context.run_next(&mut host) == Ok(true) {
            if let Some(command) = host.pending.take() {
                assert_eq!(command.command.command, "seed");
                host.events.push(format!("seed:{}", command.frame.depth));
            }
        }
        assert_eq!(
            host.events, vector.events,
            "{:?} parent={} quota={}",
            vector.body, vector.parent, vector.quota
        );
        assert_eq!(context.remaining_quota(), vector.remaining);
    }
}

#[test]
fn native_function_accumulators_and_execution_scope_cleanup() {
    #[derive(Deserialize)]
    struct Vector {
        values: Vec<i32>,
        any: bool,
        sum: i32,
    }
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../tests/java/commands/functions/accumulators.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 7);
    let mut manager = ServerFunctionManager::default();
    for vector in vectors {
        let outer = manager.begin_execution();
        let collector = manager.create_collector();
        for value in vector.values {
            manager.add_result(collector, value);
        }
        let inner = manager.begin_execution();
        let discarded = manager.create_collector();
        manager.add_result(discarded, 123);
        manager.end_execution(inner);
        assert_eq!(
            manager.take_result(collector),
            vector.any.then_some(vector.sum)
        );
        manager.end_execution(outer);
        let next = manager.begin_execution();
        assert_eq!(manager.create_collector(), collector);
        manager.end_execution(next);
    }
}

#[test]
fn native_function_command_results_aggregation_and_return_chains() {
    use crate::commands::{
        execution::{CommandExecutionHost, CommandQueueEntry, ExecutionContext, PendingCommand},
        source::{CommandCallbackHost, CommandResultCallback},
    };
    use ferraloom_protocol::chat::Component;
    use ferraloom_util::Vec3;
    #[derive(Default)]
    struct Host {
        manager: ServerFunctionManager,
        events: Vec<String>,
        pending: Option<PendingCommand>,
    }
    impl CommandCallbackHost for Host {
        fn add_function_result(&mut self, collector: CollectorId, value: i32) {
            self.manager.add_result(collector, value);
        }
        fn send_success(&mut self, _: &CommandSourceStack, message: Component, _: bool) {
            self.events.push(crate::language::plain_text(&message));
        }
    }
    impl CommandExecutionHost for Host {
        fn emit_command(&mut self, command: PendingCommand) {
            self.pending = Some(command);
        }
    }
    impl FunctionExecutionHost for Host {
        fn create_function_collector(&mut self) -> CollectorId {
            self.manager.create_collector()
        }
        fn take_function_result(&mut self, collector: CollectorId) -> Option<i32> {
            self.manager.take_result(collector)
        }
    }
    fn callback(name: &'static str) -> CommandResultCallback {
        CommandResultCallback::new(move |host, success, value| {
            host.send_success(
                &CommandSourceStack::server(4, Vec3::ZERO),
                Component::literal(format!("{name}:{success}:{value}")),
                false,
            );
        })
    }
    #[derive(Deserialize)]
    struct Vector {
        bodies: Vec<Vec<String>>,
        silent: bool,
        callback: bool,
        returning: bool,
        quota: i32,
        events: Vec<String>,
        remaining: i32,
    }
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../tests/java/commands/functions/functions.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 128);
    let commands = Commands::default();
    for vector in vectors {
        let mut source = CommandSourceStack::server(4, Vec3::ZERO);
        source.silent = vector.silent;
        if vector.callback {
            source.callback = callback("original");
        }
        let functions = vector
            .bodies
            .iter()
            .enumerate()
            .map(|(index, body)| {
                Arc::new(
                    CompiledFunction::from_lines(
                        format!("test:f{index}").parse().unwrap(),
                        &commands,
                        CommandSourceStack::server(4, Vec3::ZERO),
                        body,
                    )
                    .unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let mut context = ExecutionContext::<Host>::new(vector.quota, 10);
        let frame = context.top_frame(callback("frame"));
        let mut host = Host::default();
        let scope = host.manager.begin_execution();
        queue_functions(
            &mut context,
            frame.clone(),
            &functions,
            source,
            vector.returning,
            &mut host,
        )
        .unwrap();
        context.queue_next(CommandQueueEntry::new(
            frame,
            |context, _, host: &mut Host| {
                host.events.push("tail".into());
                context.increment_cost();
            },
        ));
        while context.run_next(&mut host) == Ok(true) {
            if let Some(command) = host.pending.take() {
                assert_eq!(command.command.command, "seed");
                assert!(command.command.source.silent);
                assert!(command.command.source.callback.is_empty());
                host.events.push(format!("seed:{}", command.frame.depth));
            }
        }
        host.manager.end_execution(scope);
        assert_eq!(
            host.events, vector.events,
            "{:?}, silent={}, callback={}, returning={}, quota={}",
            vector.bodies, vector.silent, vector.callback, vector.returning, vector.quota
        );
        assert_eq!(context.remaining_quota(), vector.remaining);
    }
}
