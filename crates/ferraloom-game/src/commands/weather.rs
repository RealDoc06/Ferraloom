//! Port of WeatherCommand over the canonical server-owned weather.
use super::{PreparedCommand, builtins::CommandResult};
use crate::level::{Level, LevelError};
use ferraloom_protocol::chat::Component;
use ferraloom_util::random::RandomSource;

pub fn run(command: &PreparedCommand, level: &mut Level) -> Result<CommandResult, LevelError> {
    let action = command.literals[1].as_str();
    let requested = command.integer("duration").unwrap_or(-1);
    let duration = if requested == -1 {
        let (min, max) = match action {
            "clear" => (12000, 180000),
            "rain" => (12000, 24000),
            "thunder" => (3600, 15600),
            _ => unreachable!("registered weather variant"),
        };
        min + level.random().next_int_bound(max - min + 1)
    } else {
        requested
    };
    match action {
        "clear" => level.set_weather(duration, 0, false, false),
        "rain" => level.set_weather(0, duration, true, false),
        "thunder" => level.set_weather(0, duration, true, true),
        _ => unreachable!("registered weather variant"),
    }?;
    // Vanilla returns the supplied duration, including the -1 default sentinel.
    Ok(CommandResult::success(
        requested,
        Component::translatable(format!("commands.weather.set.{action}"), vec![]),
        true,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        commands::{CommandSourceStack, Commands},
        level::Region,
    };
    use ferraloom_util::{
        Vec3,
        random::{LegacyRandomSource, Random},
    };
    #[derive(serde::Deserialize)]
    struct Vector {
        seed: i64,
        action: String,
        duration: i32,
        parameters: (i32, i32, bool, bool),
        feedback: serde_json::Value,
        broadcast: bool,
        value: i32,
        next_random: i64,
    }
    #[test]
    fn weather_parameters_feedback_and_canonical_rng_consumption_match_java() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let commands = Commands::default();
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("vanilla/weather-vectors.json")).unwrap();
        for vector in vectors {
            let mut level = Level::new(
                Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            level.install_shared(Box::default());
            level.set_random_streams(Random::Legacy(LegacyRandomSource::new(vector.seed)), 0);
            let input = format!(
                "weather {}{}",
                vector.action,
                if vector.duration == -1 {
                    String::new()
                } else {
                    format!(" {}t", vector.duration)
                }
            );
            let command = commands
                .prepare(&input, CommandSourceStack::server(4, Vec3::ZERO))
                .unwrap();
            let result = run(&command, &mut level).unwrap();
            let weather = level.weather().unwrap();
            assert_eq!(
                (
                    weather.clear_weather_time,
                    weather.rain_time,
                    weather.raining,
                    weather.thundering
                ),
                vector.parameters,
                "{input}, seed {}",
                vector.seed
            );
            assert_eq!(weather.thunder_time, weather.rain_time);
            assert_eq!(result.value, vector.value, "{input}");
            assert_eq!(result.feedback[0].broadcast, vector.broadcast, "{input}");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(
                    &result.feedback[0].message.to_json().unwrap()
                )
                .unwrap(),
                vector.feedback,
                "{input}"
            );
            assert_eq!(
                level.random().next_long(),
                vector.next_random,
                "{input}, seed {}",
                vector.seed
            );
        }
    }
}
