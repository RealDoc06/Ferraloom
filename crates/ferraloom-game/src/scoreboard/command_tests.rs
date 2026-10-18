use super::*;
use crate::commands::{self, scoreboard::Holder, source::CommandSourceStack};
use ferraloom_protocol::{Wire, Writer, play::clientbound as cb};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
struct Case {
    command: String,
    value: Option<i32>,
    error: Option<Value>,
    feedback: Vec<Value>,
    packets: Vec<Value>,
}
fn holder(command: &commands::PreparedCommand, argument: &str) -> Vec<Holder> {
    command
        .string(argument)
        .map(|name| Holder {
            name: name.to_owned(),
            display: if name == "Doc" {
                Some(ferraloom_protocol::chat::Component::literal(name))
            } else {
                None
            },
        })
        .into_iter()
        .collect()
}
fn packet_json(packet: &cb::Packet) -> Value {
    let mut writer = Writer::new();
    let name = match packet {
        cb::Packet::SetObjective(p) => {
            p.encode(&mut writer).unwrap();
            "ClientboundSetObjectivePacket"
        }
        cb::Packet::SetPlayerTeam(p) => {
            p.encode(&mut writer).unwrap();
            "ClientboundSetPlayerTeamPacket"
        }
        cb::Packet::SetScore(p) => {
            p.encode(&mut writer).unwrap();
            "ClientboundSetScorePacket"
        }
        cb::Packet::ResetScore(p) => {
            p.encode(&mut writer).unwrap();
            "ClientboundResetScorePacket"
        }
        cb::Packet::SetDisplayObjective(p) => {
            p.encode(&mut writer).unwrap();
            "ClientboundSetDisplayObjectivePacket"
        }
        _ => unreachable!(),
    };
    let bytes = writer.into_inner();
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    json!({"type":name,"hex":hex})
}
#[test]
fn vanilla_registered_commands_feedback_and_packet_bytes() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../../tests/java/scoreboard/commands.json")).unwrap();
    assert_eq!(cases.len(), 115);
    let commands = commands::Commands::default();
    let mut board = ServerScoreboard::default();
    for case in cases {
        let command = commands
            .prepare(
                &case.command,
                CommandSourceStack::server(4, ferraloom_util::Vec3::ZERO),
            )
            .unwrap_or_else(|error| panic!("{}: {error}", case.command));
        let targets = if command.argument("targets").is_some() {
            holder(&command, "targets")
        } else {
            holder(&command, "target")
        };
        let result = match command.command.as_str() {
            "scoreboard" => commands::scoreboard::execute(
                &mut board,
                &command,
                &targets,
                &holder(&command, "source"),
            ),
            "team" => commands::team::execute(&mut board, &command, &holder(&command, "members")),
            "trigger" => commands::trigger::execute(
                &mut board,
                &command,
                &Holder {
                    name: "Doc".into(),
                    display: Some(Component::literal("Doc")),
                },
            ),
            _ => unreachable!(),
        };
        match result {
            Ok(result) => {
                assert!(
                    case.error.is_none(),
                    "{} expected {:?}",
                    case.command,
                    case.error
                );
                assert_eq!(Some(result.value), case.value, "{} result", case.command);
                let feedback: Vec<_> = result.feedback.iter().map(|feedback|json!({"message":serde_json::from_str::<Value>(&feedback.message.to_json().unwrap()).unwrap(),"broadcast":feedback.broadcast})).collect();
                assert_eq!(feedback, case.feedback, "{} feedback", case.command);
            }
            Err(error) => {
                let component = commands::exceptions::error_component(&error);
                let actual: Value = serde_json::from_str(&component.to_json().unwrap()).unwrap();
                assert_eq!(Some(actual), case.error, "{} error", case.command);
                assert!(case.feedback.is_empty());
            }
        }
        let packets: Vec<_> = board.drain_packets().iter().map(packet_json).collect();
        assert_eq!(packets, case.packets, "{} packets", case.command);
    }
}
#[test]
fn criterion_adapters_preserve_independent_stat_and_score_values() {
    use crate::{
        scoreboard::ObjectiveCriteria,
        stats::{ServerStatsCounter, Stat},
    };
    let stat = Stat::Mined(ferraloom_data::Block::STONE);
    let criterion = "minecraft.mined:minecraft.stone";
    let mut stats = ServerStatsCounter::default();
    let mut board = ServerScoreboard::default();
    board
        .add_objective(
            "mined".into(),
            Objective::new(
                ObjectiveCriteria::by_name(criterion).unwrap(),
                Component::literal("Mined"),
            ),
        )
        .unwrap();
    board.set_score("Doc", "mined", 100, None, false).unwrap();
    stats.set_value(stat, i32::MAX - 1);
    board.award_stat(&mut stats, "Doc", stat, 5, None).unwrap();
    let mut actual = vec![vec![
        stats.get_value(stat),
        board
            .scoreboard()
            .get_player_score("Doc", "mined")
            .unwrap()
            .value,
    ]];
    board.reset_stat(&mut stats, "Doc", stat, None).unwrap();
    actual.push(vec![
        stats.get_value(stat),
        board
            .scoreboard()
            .get_player_score("Doc", "mined")
            .unwrap()
            .value,
    ]);
    let expected: Vec<Vec<i32>> =
        serde_json::from_str(include_str!("../../tests/java/scoreboard/producers.json")).unwrap();
    assert_eq!(actual, expected);
    assert!(
        board
            .scoreboard()
            .get_player_score("Doc", "mined")
            .unwrap()
            .locked
    );
}
#[test]
fn join_sends_teams_then_each_displayed_objective_once() {
    let mut board = ServerScoreboard::default();
    board.add_player_team("red");
    board.add_player_to_team("Doc", "red").unwrap();
    board
        .add_objective(
            "points".into(),
            Objective::new(
                ObjectiveCriteria::by_name("dummy").unwrap(),
                Component::literal("Points"),
            ),
        )
        .unwrap();
    board
        .add_objective(
            "hidden".into(),
            Objective::new(
                ObjectiveCriteria::by_name("dummy").unwrap(),
                Component::literal("Hidden"),
            ),
        )
        .unwrap();
    board.set_score("Doc", "points", 3, None, false).unwrap();
    board
        .set_display_objective(DisplaySlot::List, Some("points".into()))
        .unwrap();
    board
        .set_display_objective(DisplaySlot::Sidebar, Some("points".into()))
        .unwrap();
    let packets = board.join_packets();
    let actual: Vec<_> = packets.iter().map(packet_json).collect();
    let expected: Vec<Value> =
        serde_json::from_str(include_str!("../../tests/java/scoreboard/join.json")).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn vanilla_score_contents_formatting_and_absent_scores() {
    let mut board = ServerScoreboard::default();
    board
        .add_objective(
            "points".into(),
            Objective::new(
                ObjectiveCriteria::by_name("dummy").unwrap(),
                Component::literal("Points"),
            ),
        )
        .unwrap();
    assert_eq!(
        board.scoreboard().formatted_score("Doc", "missing"),
        Component::default()
    );
    assert_eq!(
        board.scoreboard().formatted_score("Doc", "points"),
        Component::default()
    );
    board.set_score("Doc", "points", 3, None, false).unwrap();
    let mut actual = Vec::new();
    actual.push(board.scoreboard().formatted_score("Doc", "points"));
    board
        .update_objective("points", |obj| {
            obj.number_format = Some(NumberFormat::Blank {})
        })
        .unwrap();
    actual.push(board.scoreboard().formatted_score("Doc", "points"));
    board
        .update_objective("points", |obj| {
            obj.number_format = Some(NumberFormat::Fixed {
                value: Component::literal("fixed"),
            })
        })
        .unwrap();
    actual.push(board.scoreboard().formatted_score("Doc", "points"));
    board
        .update_score(
            "Doc",
            "points",
            |score| {
                score.number_format = Some(NumberFormat::Styled {
                    style: ferraloom_protocol::chat::Style {
                        bold: Some(true),
                        ..Default::default()
                    },
                })
            },
            true,
        )
        .unwrap();
    actual.push(board.scoreboard().formatted_score("Doc", "points"));
    let actual: Vec<Value> = actual
        .into_iter()
        .map(|component| serde_json::from_str(&component.to_json().unwrap()).unwrap())
        .collect();
    let expected: Vec<Value> =
        serde_json::from_str(include_str!("../../tests/java/scoreboard/formatting.json")).unwrap();
    assert_eq!(actual, expected);
}
