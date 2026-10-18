use super::*;
use ferraloom_nbt::{Compression, NbtAccounter, NbtIo, Tag, parse_compound};
use ferraloom_storage::{SavedData, SavedDataCodec};

const VECTORS: &str = include_str!("../../tests/java/scoreboard/vectors.tsv");
fn objective() -> Objective {
    Objective::new(
        ObjectiveCriteria::by_name("dummy").unwrap(),
        ferraloom_protocol::chat::Component::literal("Points"),
    )
}
#[test]
fn vanilla_membership_and_alliance() {
    let mut board = Scoreboard::default();
    board.add_player_team("a");
    board.add_player_team("b");
    let mut actual = Vec::new();
    actual.push(format!(
        "membership\tfirst\t{}\t{}",
        board.add_player_to_team("Doc", "a").unwrap(),
        board.get_player_team("a").unwrap().members().len()
    ));
    actual.push(format!(
        "membership\tsame\t{}\t{}",
        board.add_player_to_team("Doc", "a").unwrap(),
        board.get_player_team("a").unwrap().members().len()
    ));
    actual.push(format!(
        "membership\tmove\t{}\t{}\t{}",
        board.add_player_to_team("Doc", "b").unwrap(),
        board.get_player_team("a").unwrap().members().len(),
        board.get_player_team("b").unwrap().members().len()
    ));
    actual.push(format!(
        "membership\twrong-team\t{}",
        board.remove_player_from_specific_team("Doc", "a").is_err()
    ));
    actual.push(format!(
        "membership\tremove\t{}\t{}",
        board.remove_player_from_team("Doc"),
        board.remove_player_from_team("Doc")
    ));
    board.add_player_team("a");
    actual.push(format!(
        "membership\texisting\t{}",
        board.teams().count() == 2
    ));
    board.add_player_to_team("Doc", "a").unwrap();
    board.remove_player_team("a").unwrap();
    actual.push(format!(
        "membership\tremove-team\t{}",
        board.get_players_team("Doc").is_none()
    ));
    board.add_player_team("a");
    let a = board.get_player_team("a").unwrap();
    let b = board.get_player_team("b").unwrap();
    let mut foreign = Scoreboard::default();
    foreign.add_player_team("a");
    for (name, team) in [
        ("none", None),
        ("self", Some(a)),
        ("other", Some(b)),
        ("foreign", foreign.get_player_team("a")),
    ] {
        actual.push(format!("allied\t{name}\t{}", a.is_allied_to(team)));
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|v| v.starts_with("membership\t") || v.starts_with("allied\t"))
        .collect();
    assert_eq!(actual, expected);
}
#[test]
fn vanilla_collision_matrix() {
    let mut count = 0;
    for row in VECTORS.lines().filter(|v| v.starts_with("collision\t")) {
        let fields: Vec<_> = row.split('\t').collect();
        let mut board = Scoreboard::default();
        board.add_player_team("a").collision_rule =
            save::enum_value(fields[2], save::COLLISIONS).unwrap();
        board.add_player_team("b").collision_rule =
            save::enum_value(fields[3], save::COLLISIONS).unwrap();
        let a = board.get_player_team("a");
        let b = board.get_player_team("b");
        let (own, other) = match fields[1] {
            "0" => (a, a),
            "1" => (a, b),
            "2" => (None, b),
            "3" => (None, None),
            _ => unreachable!(),
        };
        assert_eq!(can_collide(own, other).to_string(), fields[4], "{row}");
        count += 1;
    }
    assert_eq!(count, 64);
}
#[test]
fn vanilla_saved_file_and_codec_defaults() {
    let mut count = 0;
    for row in VECTORS.lines() {
        let fields: Vec<_> = row.split('\t').collect();
        match fields[0] {
            "saved" => {
                // SNBT was read back from a real NbtIo.writeCompressed vanilla scoreboard.dat.
                let root = parse_compound(fields[1]).unwrap();
                let saved = SavedData::<ScoreboardSaveData>::from_nbt(root.clone()).unwrap();
                assert_eq!(
                    Tag::Compound(saved.to_nbt().unwrap()).to_string(),
                    Tag::Compound(root).to_string()
                );
                let mut bytes = Vec::new();
                NbtIo::write_compound(&mut bytes, Compression::Gzip, &saved.to_nbt().unwrap())
                    .unwrap();
                let loaded = NbtIo::read_compound(
                    bytes.as_slice(),
                    Compression::Gzip,
                    &mut NbtAccounter::unlimited(),
                )
                .unwrap();
                let roundtrip = SavedData::<ScoreboardSaveData>::from_nbt(loaded).unwrap();
                let team = roundtrip.value().0.get_players_team("Doc").unwrap();
                assert_eq!(team.name(), "b");
                assert_eq!(team.color, Some(TeamColor::Red));
                assert_eq!(team.collision_rule, CollisionRule::PushOwnTeam);
                assert_eq!(
                    roundtrip
                        .value()
                        .0
                        .get_player_score("Doc", "points")
                        .unwrap()
                        .value,
                    42
                );
                count += 1;
            }
            "codec" => {
                let input = Tag::Compound(parse_compound(fields[1]).unwrap());
                let expected = Tag::Compound(parse_compound(fields[2]).unwrap());
                // The packed codec preserves absent team DisplayName; live loading supplies the name.
                let mut expected = expected;
                if let Some(teams) = expected.as_compound_mut().unwrap().get_list_mut("Teams") {
                    for team in &mut teams.0 {
                        let team = team.as_compound_mut().unwrap();
                        if team.get("DisplayName").is_none() {
                            let name = team.get("Name").unwrap().clone();
                            team.insert("DisplayName", name);
                        }
                    }
                }
                assert_eq!(
                    ScoreboardSaveData::decode(&input)
                        .unwrap()
                        .encode()
                        .unwrap(),
                    expected,
                    "{row}"
                );
                count += 1;
            }
            _ => {}
        }
    }
    assert_eq!(count, 5);
}
#[test]
fn entity_names_snapshots_and_score_lifecycle() {
    use ferraloom_util::uuid::Uuid;
    let uuid = Uuid::new(0, 1);
    let mut server = ServerScoreboard::default();
    let board = server.scoreboard_mut();
    board.add_player_team("red");
    board.add_player_to_team("Doc", "red").unwrap();
    board.add_player_to_team(&uuid.to_string(), "red").unwrap();
    assert_eq!(scoreboard_name(Some("Doc"), uuid), "Doc");
    assert_eq!(
        scoreboard_name(None, uuid),
        "00000000-0000-0000-0000-000000000001"
    );
    assert_eq!(entity_team(board, None, uuid).unwrap().name(), "red");
    assert!(entity_is_allied_to(
        board,
        "Doc",
        Some(&uuid.to_string()),
        false
    ));
    assert!(entity_is_allied_to(
        board,
        "unteamed",
        Some("unteamed"),
        true
    ));
    assert!(!entity_is_allied_to(
        board,
        "unteamed",
        Some("unteamed"),
        false
    ));
    assert!(!entity_is_allied_to(board, "Doc", None, true));
    let snapshot = server.snapshot();
    server.scoreboard_mut().remove_player_from_team("Doc");
    assert!(snapshot.get_players_team("Doc").is_some());
    assert!(server.scoreboard().get_players_team("Doc").is_none());
    let board = server.scoreboard_mut();
    board.add_objective("points".into(), objective()).unwrap();
    assert!(board.add_objective("points".into(), objective()).is_err());
    assert!(board.get_or_create_score("Doc", "points").unwrap().locked);
    board.set_score("Doc", "points", 7, None, false).unwrap();
    board
        .set_display_objective(DisplaySlot::Sidebar, Some("points".into()))
        .unwrap();
    board.remove_objective("points").unwrap();
    assert!(board.get_player_score("Doc", "points").is_none());
    assert!(board.get_display_objective(DisplaySlot::Sidebar).is_none());
    // Vanilla retains tracked holders when removeObjective leaves their score map empty.
    assert_eq!(board.tracked_players().collect::<Vec<_>>(), vec!["Doc"]);
    assert!(
        !ObjectiveCriteria::by_name("minecraft.mined:minecraft.stone")
            .unwrap()
            .read_only
    );
    let health = Objective::new(
        ObjectiveCriteria::by_name("health").unwrap(),
        Component::literal("Health"),
    );
    board.add_objective("health".into(), health).unwrap();
    assert!(board.set_score("Doc", "health", 20, None, false).is_err());
    board.set_score("Doc", "health", 20, None, true).unwrap();
}
#[test]
fn invalid_saved_scoreboard_is_not_silently_dropped() {
    for input in [
        "{Teams:1}",
        "{Teams:[{Name:'a',CollisionRule:'bad'}]}",
        "{Objectives:[{Name:'a'}]}",
        "{Objectives:[{Name:'a',DisplayName:'A',CriteriaName:'bad'}]}",
    ] {
        assert!(
            ScoreboardSaveData::decode(&Tag::Compound(parse_compound(input).unwrap())).is_err(),
            "{input}"
        );
    }
}

#[test]
fn vanilla_fastutil_iterator_and_stream_orders() {
    let row = VECTORS
        .lines()
        .find(|row| row.starts_with("saved\t"))
        .unwrap();
    let root = parse_compound(row.split_once('\t').unwrap().1).unwrap();
    let saved = SavedData::<ScoreboardSaveData>::from_nbt(root).unwrap();
    let board = &saved.value().0;
    for row in VECTORS.lines().filter(|row| row.starts_with("ordering\t")) {
        let fields: Vec<_> = row.split('\t').collect();
        let actual = match fields[1] {
            "teams" => {
                assert_eq!(
                    board
                        .teams()
                        .map(PlayerTeam::name)
                        .collect::<Vec<_>>()
                        .join(","),
                    fields[3]
                );
                board
                    .teams()
                    .rev()
                    .map(PlayerTeam::name)
                    .collect::<Vec<_>>()
                    .join(",")
            }
            "objectives" => board
                .objectives()
                .rev()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
                .join(","),
            "holders" => board.tracked_players().collect::<Vec<_>>().join(","),
            "scores" => board
                .scores_for_objective("points")
                .map(|(holder, _)| holder)
                .collect::<Vec<_>>()
                .join(","),
            _ => unreachable!(),
        };
        assert_eq!(actual, fields[2], "{row}");
    }
}
