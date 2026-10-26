use super::*;
use crate::commands::{CommandSourceStack, Commands};
use serde::Deserialize;

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
