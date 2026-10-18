//! Port of TickCommand. The server adapter owns the live tick-rate manager and timing ring.
use super::{
    PreparedCommand,
    arguments::ArgumentValue,
    builtins::{self, CommandResult, Feedback},
    exceptions::error,
};
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_protocol::chat::Component;

#[derive(Clone, Debug)]
pub struct TickRateSnapshot {
    pub rate: f32,
    pub nanoseconds_per_tick: i64,
    pub average_tick_time_nanos: i64,
    pub tick_times_nanos: [i64; 100],
    pub frozen: bool,
    pub stepping: bool,
    pub sprinting: bool,
}
pub trait TickRateAccess {
    fn snapshot(&self) -> TickRateSnapshot;
    fn set_tick_rate(&mut self, rate: f32);
    fn set_frozen(&mut self, frozen: bool);
    fn step_game_if_paused(&mut self, ticks: i32) -> bool;
    fn stop_stepping(&mut self) -> bool;
    fn request_game_to_sprint(&mut self, ticks: i32) -> bool;
    fn stop_sprinting(&mut self) -> bool;
}
fn one_decimal(value: f32) -> String {
    // Formatter's fixed-point conversion rounds a tie away from zero.
    format!("{:.1}", (f64::from(value) * 10.).round() / 10.)
}
fn milliseconds(nanos: i64) -> String {
    one_decimal(nanos as f32 / 1_000_000f32)
}
fn feedback(key: &str, args: Vec<ferraloom_protocol::chat::Argument>, broadcast: bool) -> Feedback {
    Feedback {
        message: Component::translatable(key, args),
        broadcast,
    }
}
/// Port of ServerTickRateManager#finishTickSprint's feedback.
pub fn sprint_report(ticks_per_second: i32, milliseconds_per_tick: String) -> Feedback {
    feedback(
        "commands.tick.sprint.report",
        vec![
            builtins::integer(ticks_per_second),
            builtins::string(milliseconds_per_tick),
        ],
        true,
    )
}
/// Mutates the live manager and returns feedback. The runtime drains its ordered
/// events before emitting this feedback or reporting a command error.
pub fn run<A: TickRateAccess>(
    command: &PreparedCommand,
    access: &mut A,
) -> Result<CommandResult, CommandSyntaxException> {
    let action = command.literals[1].as_str();
    let stopped = command.literals.iter().any(|literal| literal == "stop");
    let mut result = CommandResult {
        value: 1,
        feedback: Vec::new(),
        rule_change: None,
    };
    match action {
        "query" => {
            let snapshot = access.snapshot();
            let rate = one_decimal(snapshot.rate);
            let busy = milliseconds(snapshot.average_tick_time_nanos);
            let status = if snapshot.sprinting {
                "sprinting"
            } else if snapshot.frozen {
                "frozen"
            } else if snapshot.nanoseconds_per_tick < snapshot.average_tick_time_nanos {
                "lagging"
            } else {
                "running"
            };
            result.feedback.push(feedback(
                &format!("commands.tick.status.{status}"),
                vec![],
                false,
            ));
            result.feedback.push(if snapshot.sprinting {
                feedback(
                    "commands.tick.query.rate.sprinting",
                    vec![builtins::string(rate), builtins::string(busy)],
                    false,
                )
            } else {
                feedback(
                    "commands.tick.query.rate.running",
                    vec![
                        builtins::string(rate),
                        builtins::string(busy),
                        builtins::string(milliseconds(snapshot.nanoseconds_per_tick)),
                    ],
                    false,
                )
            });
            let mut samples = snapshot.tick_times_nanos;
            samples.sort();
            result.feedback.push(feedback(
                "commands.tick.query.percentiles",
                vec![
                    builtins::string(milliseconds(samples[50])),
                    builtins::string(milliseconds(samples[95])),
                    builtins::string(milliseconds(samples[99])),
                    builtins::integer(100),
                ],
                false,
            ));
            result.value = snapshot.rate as i32;
        }
        "rate" => {
            let Some(ArgumentValue::Float(rate)) = command.argument("rate") else {
                unreachable!()
            };
            access.set_tick_rate(*rate);
            result.value = *rate as i32;
            result.feedback.push(feedback(
                "commands.tick.rate.success",
                vec![builtins::string(one_decimal(*rate))],
                true,
            ));
        }
        "freeze" | "unfreeze" => {
            let freeze = action == "freeze";
            if freeze {
                if access.snapshot().sprinting {
                    access.stop_sprinting();
                }
                if access.snapshot().stepping {
                    access.stop_stepping();
                }
            }
            access.set_frozen(freeze);
            result.value = i32::from(freeze);
            result.feedback.push(feedback(
                if freeze {
                    "commands.tick.status.frozen"
                } else {
                    "commands.tick.status.running"
                },
                vec![],
                true,
            ));
        }
        "step" if stopped => {
            if !access.stop_stepping() {
                return Err(error("commands.tick.step.stop.fail", &[]));
            }
            result
                .feedback
                .push(feedback("commands.tick.step.stop.success", vec![], true));
        }
        "step" => {
            let ticks = command.integer("time").unwrap_or(1);
            if !access.step_game_if_paused(ticks) {
                return Err(error("commands.tick.step.fail", &[]));
            }
            result.feedback.push(feedback(
                "commands.tick.step.success",
                vec![builtins::integer(ticks)],
                true,
            ));
        }
        "sprint" if stopped => {
            if !access.stop_sprinting() {
                return Err(error("commands.tick.sprint.stop.fail", &[]));
            }
            result
                .feedback
                .push(feedback("commands.tick.sprint.stop.success", vec![], true));
        }
        "sprint" => {
            let interrupted =
                access.request_game_to_sprint(command.integer("time").expect("sprint time"));
            if interrupted {
                result
                    .feedback
                    .push(feedback("commands.tick.sprint.stop.success", vec![], true));
            }
            result
                .feedback
                .push(feedback("commands.tick.status.sprinting", vec![], true));
        }
        _ => unreachable!(),
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{CommandSourceStack, Commands};
    use ferraloom_util::Vec3;

    #[derive(serde::Deserialize)]
    struct Vector {
        command: String,
        rate: f32,
        average: i64,
        frozen: bool,
        stepping: bool,
        sprinting: bool,
        outcome: bool,
        feedback: serde_json::Value,
        value: Option<i32>,
        error: Option<String>,
    }
    struct Host {
        state: TickRateSnapshot,
        outcome: bool,
        calls: Vec<&'static str>,
    }
    impl TickRateAccess for Host {
        fn snapshot(&self) -> TickRateSnapshot {
            self.state.clone()
        }
        fn set_tick_rate(&mut self, _rate: f32) {
            self.calls.push("rate");
        }
        fn set_frozen(&mut self, _frozen: bool) {
            self.calls.push("freeze");
        }
        fn step_game_if_paused(&mut self, _ticks: i32) -> bool {
            self.calls.push("step");
            self.outcome
        }
        fn stop_stepping(&mut self) -> bool {
            self.calls.push("stop_step");
            self.outcome
        }
        fn request_game_to_sprint(&mut self, _ticks: i32) -> bool {
            self.calls.push("sprint");
            self.outcome
        }
        fn stop_sprinting(&mut self) -> bool {
            self.calls.push("stop_sprint");
            self.outcome
        }
    }
    #[test]
    fn sprint_report_component_and_console_text_match_native_manager() {
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("vanilla/tick-report-vector.json")).unwrap();
        let report = sprint_report(100, "10.00".into());
        assert_eq!(Some(report.broadcast), expected["broadcast"].as_bool());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&report.message.to_json().unwrap()).unwrap(),
            expected["message"]
        );
        assert_eq!(
            crate::commands::exceptions::plain_text(&report.message),
            expected["text"].as_str().unwrap()
        );
    }
    #[test]
    fn tick_feedback_and_errors_match_java_command_methods() {
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("vanilla/tick-vectors.json")).unwrap();
        let commands = Commands::default();
        let source = CommandSourceStack::server(4, Vec3::ZERO);
        for vector in vectors {
            let mut host = Host {
                state: TickRateSnapshot {
                    rate: vector.rate,
                    average_tick_time_nanos: vector.average,
                    nanoseconds_per_tick: 50_000_000,
                    tick_times_nanos: std::array::from_fn(|i| (99 - i) as i64 * 123_456),
                    frozen: vector.frozen,
                    stepping: vector.stepping,
                    sprinting: vector.sprinting,
                },
                outcome: vector.outcome,
                calls: Vec::new(),
            };
            let command = commands.prepare(&vector.command, source.clone()).unwrap();
            match (run(&command, &mut host), vector.error) {
                (Err(error), Some(expected)) => {
                    assert_eq!(error.raw_message.text(), expected, "{}", vector.command);
                }
                (Ok(result), None) => {
                    assert_eq!(result.value, vector.value.unwrap(), "{}", vector.command);
                    let feedback = result.feedback.into_iter().map(|feedback| {
                        serde_json::json!({
                            "broadcast": feedback.broadcast,
                            "message": serde_json::from_str::<serde_json::Value>(&feedback.message.to_json().unwrap()).unwrap(),
                        })
                    }).collect::<Vec<_>>();
                    assert_eq!(
                        serde_json::json!(feedback),
                        vector.feedback,
                        "{}",
                        vector.command
                    );
                    if vector.command == "tick freeze" {
                        assert_eq!(host.calls, ["stop_sprint", "stop_step", "freeze"]);
                    }
                }
                (result, error) => panic!(
                    "{}: unexpected result {result:?}, expected error {error:?}",
                    vector.command
                ),
            }
        }
    }
}
