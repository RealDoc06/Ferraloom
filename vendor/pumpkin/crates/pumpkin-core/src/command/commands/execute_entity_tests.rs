use super::{ERROR_CONDITIONAL_FAILED, ERROR_CONDITIONAL_FAILED_COUNT};
use crate::block::entities::command_block::CommandBlockEntity;
use crate::command::CommandSender;
use crate::command::context::command_source::{ReturnValue, ReturnValueCallable};
use crate::command::test::CommandFixture;
use crate::entity::r#type::from_type;
use pumpkin_data::entity::EntityType;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::text::TextComponent;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn add_pigs(fixture: &CommandFixture) {
    let world = fixture.player.world();
    for uuid in [201, 202] {
        world.add_entity_silent(from_type(
            &EntityType::PIG,
            Vector3::new(0.0, 64.0, 0.0),
            &world,
            Uuid::from_u128(uuid),
        ));
    }
}

#[derive(Default)]
struct ResultLog(Mutex<Vec<ReturnValue>>);

impl ReturnValueCallable for ResultLog {
    fn call(&self, value: ReturnValue) {
        self.0.lock().unwrap().push(value);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_entity_conditions_return_counts_and_success_feedback() {
    let fixture = CommandFixture::new().await;
    add_pigs(&fixture);
    assert_eq!(
        fixture.execute(CommandSender::Dummy, "execute if entity @a"),
        1
    );

    let output = Arc::new(CommandBlockEntity::new(
        BlockPos::new(0, 64, 0),
        true,
        false,
    ));
    let source = CommandSender::Dummy
        .into_source(&fixture.server)
        .with_output(CommandSender::CommandBlock(
            output.clone(),
            fixture.player.world(),
        ));
    let dispatcher = fixture.server.command_dispatcher.load();
    for (input, result, key, arguments) in [
        (
            "execute if entity @a",
            1,
            "commands.execute.conditional.pass_count",
            vec![TextComponent::text("1")],
        ),
        (
            "execute if entity @e[type=minecraft:pig]",
            2,
            "commands.execute.conditional.pass_count",
            vec![TextComponent::text("2")],
        ),
        (
            "execute unless entity @e[type=minecraft:cow]",
            1,
            "commands.execute.conditional.pass",
            vec![],
        ),
    ] {
        output.last_output.lock().unwrap().clear();
        assert_eq!(
            dispatcher.execute_input(input, &source),
            Ok(result),
            "{input}"
        );
        let expected = TextComponent::translate(key, arguments).get_text();
        assert!(
            output
                .last_output
                .lock()
                .unwrap()
                .ends_with(&format!("] {expected}")),
            "{input}"
        );
    }
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_entity_condition_failures_report_counts_and_failure_callbacks() {
    let fixture = CommandFixture::new().await;
    add_pigs(&fixture);
    let results = Arc::new(ResultLog::default());
    let mut source = CommandSender::Dummy.into_source(&fixture.server);
    source.command_result_taker.0.push(results.clone());
    let dispatcher = fixture.server.command_dispatcher.load();

    let error = dispatcher
        .execute_input("execute if entity @e[type=minecraft:cow]", &source)
        .unwrap_err();
    assert!(error.is(&ERROR_CONDITIONAL_FAILED));
    assert!(error.context.is_none());
    for (input, count) in [
        ("execute unless entity @a", "1"),
        ("execute unless entity @e[type=minecraft:pig]", "2"),
    ] {
        let error = dispatcher.execute_input(input, &source).unwrap_err();
        assert!(error.is(&ERROR_CONDITIONAL_FAILED_COUNT), "{input}");
        assert!(error.context.is_none(), "{input}");
        let message = serde_json::to_value(error.message).unwrap();
        assert_eq!(
            message["translate"], "commands.execute.conditional.fail_count",
            "{input}"
        );
        assert_eq!(message["with"][0]["text"], count, "{input}");
    }
    assert_eq!(
        results.0.lock().unwrap().as_slice(),
        &[ReturnValue::Failure; 3]
    );
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_entity_conditions_store_result_and_success_from_completion() {
    let fixture = CommandFixture::new().await;
    add_pigs(&fixture);
    fixture.execute(
        CommandSender::Dummy,
        "scoreboard objectives add entity_probe dummy",
    );
    let world = fixture.player.world();
    let results = Arc::new(ResultLog::default());
    let mut source = CommandSender::Dummy.into_source(&fixture.server);
    source.command_result_taker.0.push(results.clone());
    let dispatcher = fixture.server.command_dispatcher.load();

    for (condition, expected, stored_result, stored_success) in [
        ("if entity @a", ReturnValue::Success(1), 1, 1),
        (
            "if entity @e[type=minecraft:pig]",
            ReturnValue::Success(2),
            2,
            1,
        ),
        (
            "if entity @e[type=minecraft:cow]",
            ReturnValue::Failure,
            0,
            0,
        ),
        (
            "unless entity @e[type=minecraft:cow]",
            ReturnValue::Success(1),
            1,
            1,
        ),
        ("unless entity @a", ReturnValue::Failure, 0, 0),
        (
            "unless entity @e[type=minecraft:pig]",
            ReturnValue::Failure,
            0,
            0,
        ),
    ] {
        {
            let mut scoreboard = world.scoreboard.lock().unwrap();
            for holder in ["#result", "#success"] {
                scoreboard.set_score_value(world.as_ref(), holder, "entity_probe", 77);
            }
        }
        results.0.lock().unwrap().clear();
        let input = format!(
            "execute store result score #result entity_probe store success score #success entity_probe {condition}"
        );
        let actual = dispatcher.execute_input(&input, &source);
        match expected {
            ReturnValue::Success(value) => assert_eq!(actual, Ok(value), "{input}"),
            ReturnValue::Failure => assert!(actual.is_err(), "{input}: {actual:?}"),
        }
        assert_eq!(results.0.lock().unwrap().as_slice(), &[expected], "{input}");
        let scoreboard = world.scoreboard.lock().unwrap();
        assert_eq!(
            scoreboard
                .get_score("#result", "entity_probe")
                .unwrap()
                .value
                .0,
            stored_result,
            "{input}"
        );
        assert_eq!(
            scoreboard
                .get_score("#success", "entity_probe")
                .unwrap()
                .value
                .0,
            stored_success,
            "{input}"
        );
    }
    fixture.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entity_run_continuations_keep_filtering_sources_and_nested_results() {
    let fixture = CommandFixture::new().await;
    add_pigs(&fixture);
    let results = Arc::new(ResultLog::default());
    let mut source = CommandSender::Dummy.into_source(&fixture.server);
    source.command_result_taker.0.push(results.clone());
    let dispatcher = fixture.server.command_dispatcher.load();

    for (condition, result, callbacks) in [
        (
            "if entity @e[type=minecraft:pig]",
            7,
            vec![ReturnValue::Success(7)],
        ),
        (
            "unless entity @e[type=minecraft:cow]",
            7,
            vec![ReturnValue::Success(7)],
        ),
        ("if entity @e[type=minecraft:cow]", 0, vec![]),
        ("unless entity @e[type=minecraft:pig]", 0, vec![]),
    ] {
        results.0.lock().unwrap().clear();
        let input = format!("execute {condition} run return 7");
        assert_eq!(
            dispatcher.execute_input(&input, &source),
            Ok(result),
            "{input}"
        );
        assert_eq!(
            results.0.lock().unwrap().as_slice(),
            callbacks.as_slice(),
            "{input}"
        );
    }
    fixture.close().await;
}
