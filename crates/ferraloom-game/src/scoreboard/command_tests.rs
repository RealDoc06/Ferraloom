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
            entity: None,
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
fn comparable_feedback(command: &str, mut feedback: Vec<Value>) -> Vec<Value> {
    // PlayerScores#listScores keys by Objective identity hash, so only these
    // independent list entries have JVM-allocation-dependent ordering.
    if command.starts_with("scoreboard players list ") && !feedback.is_empty() {
        let entries = &mut feedback[1..];
        if entries.iter().all(|entry| {
            entry["message"]["translate"] == "commands.scoreboard.players.list.entity.entry"
        }) {
            entries.sort_by_key(Value::to_string);
        }
    }
    feedback
}
#[test]
fn vanilla_zero_score_feedback_and_displayed_packets() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../../tests/java/scoreboard/zero.json")).unwrap();
    assert_eq!(cases.len(), 8);
    let commands = commands::Commands::default();
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
    board.set_score("#one", "points", 4, None, false).unwrap();
    board.set_score("#two", "points", 0, None, false).unwrap();
    board
        .set_display_objective(DisplaySlot::Sidebar, Some("points".into()))
        .unwrap();
    board.drain_packets();
    for case in cases {
        let command = commands
            .prepare(
                &case.command,
                CommandSourceStack::server(4, ferraloom_util::Vec3::ZERO),
            )
            .unwrap();
        let targets = if command.string("targets") == Some("*") {
            board
                .scoreboard()
                .tracked_players()
                .map(Holder::for_name)
                .collect()
        } else {
            holder(&command, "targets")
        };
        let result = commands::scoreboard::execute(&mut board, &command, &targets, &[]).unwrap();
        let feedback: Vec<_> = result.feedback.iter().map(|feedback|json!({"message":serde_json::from_str::<Value>(&feedback.message.to_json().unwrap()).unwrap(),"broadcast":feedback.broadcast})).collect();
        let packets: Vec<_> = board.drain_packets().iter().map(packet_json).collect();
        assert_eq!(Some(result.value), case.value, "{} result", case.command);
        assert_eq!(feedback, case.feedback, "{} feedback", case.command);
        assert_eq!(packets, case.packets, "{} packets", case.command);
    }
}
#[test]
fn vanilla_registered_commands_feedback_and_packet_bytes() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../../tests/java/scoreboard/commands.json")).unwrap();
    assert_eq!(cases.len(), 120);
    let commands = commands::Commands::default();
    let mut board = ServerScoreboard::default();
    let mut failures = Vec::new();
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
                    entity: None,
                    name: "Doc".into(),
                    display: Some(Component::literal("Doc")),
                },
            ),
            _ => unreachable!(),
        };
        let (value, error, feedback) = match result {
            Ok(result) => {
                let feedback: Vec<_> = result.feedback.iter().map(|feedback|json!({"message":serde_json::from_str::<Value>(&feedback.message.to_json().unwrap()).unwrap(),"broadcast":feedback.broadcast})).collect();
                (Some(result.value), None, feedback)
            }
            Err(error) => {
                let component = commands::exceptions::error_component(&error);
                let actual: Value = serde_json::from_str(&component.to_json().unwrap()).unwrap();
                (None, Some(actual), Vec::new())
            }
        };
        let packets: Vec<_> = board.drain_packets().iter().map(packet_json).collect();
        for (field, actual, expected) in [
            ("result", json!(value), json!(case.value)),
            ("error", json!(error), json!(case.error)),
            (
                "feedback",
                json!(comparable_feedback(&case.command, feedback)),
                json!(comparable_feedback(&case.command, case.feedback)),
            ),
            ("packets", json!(packets), json!(case.packets)),
        ] {
            if actual != expected {
                failures.push(format!(
                    "{} {field}: actual={actual}, expected={expected}",
                    case.command
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn vanilla_team_entity_count_feedback_and_packet_bytes() {
    #[derive(Deserialize)]
    struct TeamCase {
        command: String,
        members: Vec<String>,
        value: i32,
        feedback: Vec<Value>,
        packets: Vec<Value>,
    }
    let cases: Vec<TeamCase> = serde_json::from_str(include_str!(
        "../../tests/java/scoreboard/team-feedback.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 9);
    let commands = commands::Commands::default();
    let mut board = ServerScoreboard::default();
    board.add_player_team("count");
    board.add_player_team("other");
    board.drain_packets();
    for (index, case) in cases.into_iter().enumerate() {
        let command = commands
            .prepare(
                &case.command,
                CommandSourceStack::server(4, ferraloom_util::Vec3::ZERO),
            )
            .unwrap();
        let members: Vec<_> = case
            .members
            .into_iter()
            .map(|name| Holder {
                entity: None,
                display: (!name.starts_with('#'))
                    .then(|| Component::literal("pre-mutation entity display")),
                name,
            })
            .collect();
        let result = commands::team::execute(&mut board, &command, &members)
            .unwrap_or_else(|error| panic!("case {index}: {error}"));
        assert_eq!(result.value, case.value, "case {index} result");
        let feedback: Vec<_> = result
            .feedback
            .iter()
            .map(|feedback| {
                json!({
                    "message": serde_json::from_str::<Value>(&feedback.message.to_json().unwrap()).unwrap(),
                    "broadcast": feedback.broadcast,
                })
            })
            .collect();
        assert_eq!(feedback, case.feedback, "case {index} feedback");
        let packets: Vec<_> = board.drain_packets().iter().map(packet_json).collect();
        assert_eq!(packets, case.packets, "case {index} packets");
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

#[test]
fn readonly_swap_keeps_target_write_and_native_failure_feedback() {
    let mut board = ServerScoreboard::default();
    for (name, criterion, display) in [
        ("points", "dummy", "Points"),
        ("health", "health", "Health"),
    ] {
        board
            .add_objective(
                name.into(),
                Objective::new(
                    ObjectiveCriteria::by_name(criterion).unwrap(),
                    Component::literal(display),
                ),
            )
            .unwrap();
    }
    board.set_score("#one", "points", 5, None, false).unwrap();
    board.set_score("#two", "health", 3, None, true).unwrap();
    board
        .set_display_objective(DisplaySlot::Sidebar, Some("points".into()))
        .unwrap();
    board.drain_packets();
    let command = commands::Commands::default()
        .prepare(
            "scoreboard players operation #one points >< #two health",
            CommandSourceStack::server(4, ferraloom_util::Vec3::ZERO),
        )
        .unwrap();
    let error = commands::scoreboard::execute(
        &mut board,
        &command,
        &[Holder::for_name("#one")],
        &[Holder::for_name("#two")],
    )
    .unwrap_err();
    let failure: Value = serde_json::from_str(
        &commands::exceptions::error_component(&error)
            .to_json()
            .unwrap(),
    )
    .unwrap();
    let actual = json!({"failures":[failure],"packets":board.drain_packets().iter().map(packet_json).collect::<Vec<_>>(),"target":board.scoreboard().get_player_score("#one","points").unwrap().value,"source":board.scoreboard().get_player_score("#two","health").unwrap().value});
    let expected: Value =
        serde_json::from_str(include_str!("../../tests/java/scoreboard/readonly.json")).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn vanilla_single_entity_team_feedback_reads_completed_membership() {
    use crate::{
        Behaviors,
        entity::{
            EntityRef, EntityState,
            player::{GameType, ServerPlayer},
            synched_data,
        },
        level::{Level, LevelContext, Region},
    };
    use ferraloom_util::{Uuid, Vec3};
    #[derive(Deserialize)]
    struct SingleCase {
        kind: String,
        command: String,
        members: Vec<String>,
        value: i32,
        feedback: Vec<Value>,
        packets: Vec<Value>,
        display_changed: Option<bool>,
    }
    let cases: Vec<SingleCase> = serde_json::from_str(include_str!(
        "../../tests/java/scoreboard/team-single-feedback.json"
    ))
    .unwrap();
    assert_eq!(cases.len(), 16);
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let commands = commands::Commands::default();
    for kind in ["player", "cow"] {
        let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
        let entity = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        let uuid = Uuid::new(0, 1);
        let mut state = if kind == "player" {
            EntityState::Player(Box::new(ServerPlayer::new(
                uuid,
                "Doc".into(),
                1,
                Vec3::ZERO,
                GameType::Survival,
            )))
        } else {
            let mut state = EntityState::Cow(Box::default());
            let mut name = Component::literal("Named Cow");
            name.style.bold = Some(true);
            state
                .as_entity_mut()
                .entity_data
                .set(synched_data::CUSTOM_NAME, Some(Box::new(name)));
            state
        };
        state.as_entity_mut().uuid = uuid;
        level.add_entity(entity, state).unwrap();
        level.install_shared(Box::default());
        for (name, color) in [("red", TeamColor::Red), ("blue", TeamColor::Blue)] {
            let board = &mut level.shared_mut().unwrap().scoreboard;
            board.add_player_team(name);
            board
                .update_team(name, |team| {
                    team.color = Some(color);
                    team.prefix = Component::literal(format!("<{name}>"));
                    team.suffix = Component::literal(format!("</{name}>"));
                })
                .unwrap();
        }
        level.shared_mut().unwrap().scoreboard.drain_packets();
        for (index, case) in cases.iter().filter(|case| case.kind == kind).enumerate() {
            let command = commands
                .prepare(&case.command, CommandSourceStack::server(4, Vec3::ZERO))
                .unwrap();
            let before = level.entity_display_name(entity).unwrap();
            let members: Vec<_> = case
                .members
                .iter()
                .map(|name| {
                    if name.starts_with('#') {
                        Holder::for_name(name)
                    } else {
                        assert_eq!(name, level.entity_scoreboard_name(entity).unwrap().as_ref());
                        Holder {
                            entity: Some(entity),
                            name: name.clone(),
                            display: Some(before.clone()),
                        }
                    }
                })
                .collect();
            let pending = commands::team::execute_with_feedback(
                &mut level.shared_mut().unwrap().scoreboard,
                &command,
                &members,
            )
            .unwrap();
            // Native add/remove packets are already queued before sendSuccess's Supplier runs.
            let packets: Vec<_> = level
                .shared_mut()
                .unwrap()
                .scoreboard
                .drain_packets()
                .iter()
                .map(packet_json)
                .collect();
            assert_eq!(packets, case.packets, "{kind} case {index} packets");
            let mut reads = 0;
            let result = pending
                .resolve_feedback(|holder| {
                    assert_eq!(holder.entity, Some(entity));
                    reads += 1;
                    let display = level
                        .entity_display_name_with_scoreboard(
                            entity,
                            level.shared().unwrap().scoreboard.scoreboard(),
                        )
                        .unwrap();
                    assert_eq!(Some(before != display), case.display_changed);
                    Ok(holder.feedback_with_display(display))
                })
                .unwrap();
            assert_eq!(reads, usize::from(case.value == 1));
            assert!(
                level
                    .shared_mut()
                    .unwrap()
                    .scoreboard
                    .drain_packets()
                    .is_empty(),
                "feedback only reads"
            );
            assert_eq!(result.value, case.value);
            let feedback: Vec<_> = result.feedback.iter().map(|entry| json!({
                "message": serde_json::from_str::<Value>(&entry.message.to_json().unwrap()).unwrap(),
                "broadcast": entry.broadcast,
            })).collect();
            assert_eq!(feedback, case.feedback, "{kind} case {index} feedback");
        }
    }
}
