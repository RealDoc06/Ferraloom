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
    /// Drain manager-generated client packets and sprint reports before command feedback.
    fn flush_events(&mut self);
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
    access.flush_events();
    Ok(result)
}
