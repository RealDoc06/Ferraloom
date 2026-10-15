use super::*;
use pumpkin_nbt::nbt_compress::write_gzip_compound_tag;
use pumpkin_util::text::{TextComponentBase, TextContent, color::Color};

fn text(value: &str) -> NbtTag {
    NbtTag::String(value.into())
}

fn objective(name: &str) -> NbtCompound {
    let mut record = NbtCompound::new();
    record.put_string("Name", name.into());
    record.put("DisplayName", text(name));
    record
}

fn root(data: NbtCompound) -> NbtCompound {
    let mut root = NbtCompound::new();
    root.put_int("DataVersion", 5023);
    root.put_compound("data", data);
    root
}

/// Hand-authored data shaped like 26.3's actual save, not bytes from the JAR
/// or output of our encoder. Java omits default criteria, render type and zeros.
fn target_fixture() -> NbtCompound {
    let mut parity = objective("parity");
    parity.put_bool("display_auto_update", true);
    parity.put_int("future_objective", 19);
    let mut trigger = objective("action");
    trigger.put_string("CriteriaName", "trigger".into());
    let mut data = NbtCompound::new();
    data.put_list(
        "Objectives",
        vec![NbtTag::Compound(parity), NbtTag::Compound(trigger)],
    );
    let scores = [("A", 12), ("B", 5), ("Query", -7), ("Zero", 0)]
        .into_iter()
        .map(|(name, value)| {
            let mut record = NbtCompound::new();
            record.put_string("Name", name.into());
            record.put_string("Objective", "parity".into());
            record.put_bool("Locked", true);
            if value != 0 {
                record.put_int("Score", value);
            }
            record.put_string("future_score", "keep".into());
            NbtTag::Compound(record)
        })
        .chain([{
            let mut record = NbtCompound::new();
            record.put_string("Name", "Ready".into());
            record.put_string("Objective", "action".into());
            // Omitted Locked and Score mean unlocked and zero in Score.Packed.
            NbtTag::Compound(record)
        }])
        .collect();
    data.put_list("PlayerScores", scores);
    let mut slots = NbtCompound::new();
    slots.put_string("below_name", "parity".into());
    slots.put_string("sidebar.team.red", "action".into());
    slots.put_string("future_slot", "parity".into());
    data.put_compound("DisplaySlots", slots);
    data.put_string("future_data", "keep".into());
    let mut root = root(data);
    root.put_string("future_root", "keep".into());
    root
}

#[test]
fn reads_actual_target_structure_and_defaults_without_using_our_encoder() {
    let board = Scoreboard::from_saved_data(target_fixture()).unwrap();
    assert_eq!(board.get_objective("parity").unwrap().criterion, "dummy");
    assert_eq!(
        board.get_objective("parity").unwrap().render_type,
        RenderType::Integer
    );
    for (name, value) in [("A", 12), ("B", 5), ("Query", -7), ("Zero", 0)] {
        assert_eq!(board.get_score_value(name, "parity"), Some(value));
        assert!(board.get_score(name, "parity").unwrap().locked);
    }
    assert_eq!(board.get_objective("action").unwrap().criterion, "trigger");
    assert_eq!(board.get_score_value("Ready", "action"), Some(0));
    assert!(!board.get_score("Ready", "action").unwrap().locked);
    assert_eq!(
        board.get_display_objective(ScoreboardDisplaySlot::BelowName),
        Some("parity")
    );
    assert_eq!(
        board.get_display_objective(ScoreboardDisplaySlot::TeamRed),
        Some("action")
    );
}

#[test]
fn saves_all_modeled_fields_and_retains_unmodeled_nested_data() {
    let mut fixture = target_fixture();
    let mut data = fixture.get_compound("data").unwrap().clone();
    let mut rich_text = NbtCompound::new();
    rich_text.put_string("translate", "example.score".into());
    rich_text.put_bool("bold", true);
    rich_text.put_string("color", "gold".into());
    rich_text.put_list("with", vec![text("argument")]);
    rich_text.put_string("future_text", "keep".into());
    let mut hearts = objective("health");
    hearts.put_string("CriteriaName", "health".into());
    hearts.put_string("RenderType", "hearts".into());
    hearts.put_compound("DisplayName", rich_text.clone());
    let mut fixed = NbtCompound::new();
    fixed.put_string("type", "minecraft:fixed".into());
    fixed.put_compound("value", rich_text.clone());
    fixed.put_string("future_format", "keep".into());
    hearts.put_compound("format", fixed.clone());
    let mut objectives = data.get_list("Objectives").unwrap().to_vec();
    objectives.push(NbtTag::Compound(hearts));
    data.put_list("Objectives", objectives);

    let mut team = NbtCompound::new();
    team.put_string("Name", "builders".into());
    team.put_compound("DisplayName", rich_text.clone());
    team.put("MemberNamePrefix", text("["));
    team.put("MemberNameSuffix", text("]"));
    team.put_string("TeamColor", "red".into());
    team.put_bool("AllowFriendlyFire", false);
    team.put_bool("SeeFriendlyInvisibles", true);
    team.put_string("NameTagVisibility", "hideForOtherTeams".into());
    team.put_string("DeathMessageVisibility", "hideForOwnTeam".into());
    team.put_string("CollisionRule", "pushOwnTeam".into());
    team.put_list("Players", vec![text("Alice"), text("FakeHolder")]);
    team.put_string("future_team", "keep".into());
    let mut no_color = NbtCompound::new();
    no_color.put_string("Name", "uncolored".into());
    data.put_list(
        "Teams",
        vec![NbtTag::Compound(team.clone()), NbtTag::Compound(no_color)],
    );
    fixture.put_compound("data", data);
    let mut board = Scoreboard::from_saved_data(fixture).unwrap();
    assert_eq!(
        board.get_objective("health").unwrap().render_type,
        RenderType::Hearts
    );
    assert_eq!(
        board
            .get_objective("health")
            .unwrap()
            .display_name
            .0
            .style
            .bold,
        Some(true)
    );
    assert_eq!(board.get_team("builders").unwrap().options, 2);
    assert_eq!(board.get_team("builders").unwrap().color, NamedColor::Red);
    assert_eq!(
        board.get_team("builders").unwrap().collision_rule,
        CollisionRule::PushOwnTeam
    );
    assert_eq!(
        board.get_team("builders").unwrap().nametag_visibility,
        NameTagVisibility::HideForOtherTeams
    );
    board.set_score_value(&NoTarget, "A", "parity", 41);
    board.add_player_to_team(&NoTarget, "builders", "Bob".into());
    for (slot, _) in SLOTS {
        board.set_display_objective(&NoTarget, slot, Some("health"));
    }
    let mut styled = Style::default();
    styled.color = Some(Color::Named(NamedColor::Blue));
    styled.italic = Some(false);
    let mut score = ScoreboardScore::new(
        "Styled",
        "parity",
        VarInt(-42),
        Some(TextComponent::text("Alias")),
        Some(NumberFormat::Styled(styled)),
    );
    score.locked = false;
    board.update_score(&NoTarget, score);
    board.update_score(
        &NoTarget,
        ScoreboardScore::new(
            "Blank",
            "parity",
            VarInt(0),
            None,
            Some(NumberFormat::Blank),
        ),
    );
    let folder = tempfile::tempdir().unwrap();
    board.save(folder.path()).unwrap();
    let loaded = Scoreboard::load(folder.path()).unwrap();
    assert_eq!(loaded.objectives, board.objectives);
    assert_eq!(loaded.scores, board.scores);
    assert_eq!(loaded.display_slots, board.display_slots);
    assert_eq!(
        loaded.teams["builders"].players,
        vec!["Alice", "FakeHolder", "Bob"]
    );
    assert_eq!(
        loaded.teams["builders"].player_prefix,
        TextComponent::text("[")
    );
    assert_eq!(
        loaded.teams["builders"].player_suffix,
        TextComponent::text("]")
    );
    let encoded = read_gzip_compound_tag(
        File::open(folder.path().join("data/minecraft/scoreboard.dat")).unwrap(),
    )
    .unwrap();
    assert_eq!(encoded.get_int("DataVersion"), Some(5023));
    assert_eq!(encoded.get_string("future_root"), Some("keep"));
    let data = encoded.get_compound("data").unwrap();
    assert_eq!(data.get_string("future_data"), Some("keep"));
    assert!(data.get("DataVersion").is_none());
    let objectives = data.get_list("Objectives").unwrap();
    let health = find_record(objectives, "health");
    assert_eq!(health.get_compound("DisplayName"), Some(&rich_text));
    assert_eq!(health.get_compound("format"), Some(&fixed));
    let parity = find_record(objectives, "parity");
    assert_eq!(parity.get_bool("display_auto_update"), Some(true));
    assert_eq!(parity.get_int("future_objective"), Some(19));
    assert_eq!(
        find_record(data.get_list("PlayerScores").unwrap(), "A").get_string("future_score"),
        Some("keep")
    );
    let encoded_team = find_record(data.get_list("Teams").unwrap(), "builders");
    assert_eq!(
        encoded_team.get_string("DeathMessageVisibility"),
        Some("hideForOwnTeam")
    );
    assert_eq!(encoded_team.get_string("future_team"), Some("keep"));
    assert!(
        find_record(data.get_list("Teams").unwrap(), "uncolored")
            .get("TeamColor")
            .is_none()
    );
    let slots = data.get_compound("DisplaySlots").unwrap();
    for (_, name) in SLOTS {
        assert_eq!(slots.get_string(name), Some("health"));
    }
    assert_eq!(slots.get_string("future_slot"), Some("parity"));
}

fn find_record<'a>(records: &'a [NbtTag], name: &str) -> &'a NbtCompound {
    records
        .iter()
        .map(|tag| compound(tag).unwrap())
        .find(|record| record.get_string("Name") == Some(name))
        .unwrap()
}

#[test]
fn deleted_records_and_cleared_overrides_do_not_reappear_on_name_reuse() {
    let mut board = Scoreboard::from_saved_data(target_fixture()).unwrap();
    board.remove_score(&NoTarget, "A", "parity");
    board.set_score_value(&NoTarget, "A", "parity", 3);
    let data = board.to_saved_data().unwrap();
    assert!(
        find_record(
            data.get_compound("data")
                .unwrap()
                .get_list("PlayerScores")
                .unwrap(),
            "A"
        )
        .get("future_score")
        .is_none()
    );
    board.remove_objective(&NoTarget, "parity");
    board.add_objective(
        &NoTarget,
        ScoreboardObjective::new(
            "parity",
            TextComponent::text("New"),
            RenderType::Integer,
            None,
            "dummy",
        ),
    );
    board.clear_display_objective(&NoTarget, ScoreboardDisplaySlot::TeamRed);
    let data = board.to_saved_data().unwrap();
    let data = data.get_compound("data").unwrap();
    assert!(
        find_record(data.get_list("Objectives").unwrap(), "parity")
            .get("future_objective")
            .is_none()
    );
    assert!(data.get_compound("DisplaySlots").unwrap().is_empty());
    assert_eq!(data.get_list("PlayerScores").unwrap().len(), 1);
}

#[test]
fn newly_created_text_retains_translation_and_boolean_style_without_server_translation() {
    let translated = TextComponent(TextComponentBase {
        content: Box::new(TextContent::Translate {
            translate: "example.key".into(),
            bedrock_translate: None,
            with: vec![TextComponent::text("argument").0],
        }),
        style: Box::new(Style {
            bold: Some(true),
            italic: Some(false),
            ..Style::default()
        }),
        extra: vec![TextComponent::text("tail").0],
    });
    let mut board = Scoreboard::new();
    board.add_objective(
        &NoTarget,
        ScoreboardObjective::new(
            "test",
            translated.clone(),
            RenderType::Hearts,
            Some(NumberFormat::Fixed(translated)),
            "dummy",
        ),
    );
    let saved = board.to_saved_data().unwrap();
    let objective = find_record(
        saved
            .get_compound("data")
            .unwrap()
            .get_list("Objectives")
            .unwrap(),
        "test",
    );
    let display = objective.get_compound("DisplayName").unwrap();
    assert_eq!(display.get_string("translate"), Some("example.key"));
    assert_eq!(display.get_byte("bold"), Some(1));
    assert_eq!(display.get_byte("italic"), Some(0));
    assert_eq!(
        Scoreboard::from_saved_data(saved).unwrap().objectives,
        board.objectives
    );
}

#[test]
fn malformed_existing_files_fail_loading_without_modifying_the_original() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("data/minecraft/scoreboard.dat");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    for bytes in [vec![], b"not gzip".to_vec(), vec![31, 139, 8, 0]] {
        fs::write(&path, &bytes).unwrap();
        assert!(Scoreboard::load(folder.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    let mut invalid_objective = objective("broken");
    invalid_objective.put_int("DisplayName", 12);
    let mut invalid_score = NbtCompound::new();
    invalid_score.put_string("Name", "A".into());
    invalid_score.put_string("Objective", "parity".into());
    invalid_score.put_string("Score", "wrong type".into());
    for malformed in [
        NbtCompound::new(),
        root(NbtCompound::from_iter([(
            "Objectives".to_owned(),
            NbtTag::Int(1),
        )])),
        root(NbtCompound::from_iter([(
            "Objectives".to_owned(),
            NbtTag::List(vec![NbtTag::Compound(invalid_objective)]),
        )])),
        root(NbtCompound::from_iter([
            (
                "Objectives".to_owned(),
                NbtTag::List(vec![NbtTag::Compound(objective("parity"))]),
            ),
            (
                "PlayerScores".to_owned(),
                NbtTag::List(vec![NbtTag::Compound(invalid_score)]),
            ),
        ])),
    ] {
        write_gzip_compound_tag(malformed, File::create(&path).unwrap()).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(Scoreboard::load(folder.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::remove_file(&path).unwrap();
    assert!(
        Scoreboard::load(folder.path())
            .unwrap()
            .objectives
            .is_empty()
    );
    // Existing but unreadable-as-a-file must not be treated as missing either.
    fs::create_dir(&path).unwrap();
    assert!(Scoreboard::load(folder.path()).is_err());
}

#[test]
fn failed_scoreboard_replacement_preserves_destination_and_cleans_stage() {
    let folder = tempfile::tempdir().unwrap();
    let board = Scoreboard::from_saved_data(target_fixture()).unwrap();
    let path = folder.path().join("data/minecraft/scoreboard.dat");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("sentinel"), b"preserve").unwrap();
    assert!(board.save(folder.path()).is_err());
    assert_eq!(fs::read(path.join("sentinel")).unwrap(), b"preserve");
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn legacy_scoreboard_is_loaded_and_saved_in_current_layout_without_losing_original() {
    let folder = tempfile::tempdir().unwrap();
    let legacy = folder.path().join("data/scoreboard.dat");
    let modern = folder.path().join("data/minecraft/scoreboard.dat");
    fs::create_dir(legacy.parent().unwrap()).unwrap();
    let mut fixture = target_fixture();
    fixture.put_int("DataVersion", 4765);
    write_gzip_compound_tag(fixture, File::create(&legacy).unwrap()).unwrap();
    let previous = fs::read(&legacy).unwrap();
    let mut board = Scoreboard::load(folder.path()).unwrap();
    assert_eq!(board.get_score_value("Query", "parity"), Some(-7));
    assert!(
        !modern.exists(),
        "loading does not rewrite the source world"
    );
    board.set_score_value(&NoTarget, "Query", "parity", -19);
    board.save(folder.path()).unwrap();
    assert_eq!(fs::read(&legacy).unwrap(), previous);
    assert_eq!(
        Scoreboard::load(folder.path())
            .unwrap()
            .get_score_value("Query", "parity"),
        Some(-19),
        "the committed current-layout file takes precedence over the old copy"
    );
    fs::write(&modern, b"unreadable current save").unwrap();
    assert!(Scoreboard::load(folder.path()).is_err());
    assert_eq!(fs::read(&modern).unwrap(), b"unreadable current save");
    assert_eq!(fs::read(&legacy).unwrap(), previous);
}

#[test]
fn unreadable_legacy_scoreboard_does_not_become_an_empty_board() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("data/scoreboard.dat");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"invalid legacy save").unwrap();
    assert!(Scoreboard::load(folder.path()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"invalid legacy save");
    assert!(!folder.path().join("data/minecraft/scoreboard.dat").exists());
}

#[test]
fn sparse_target_team_and_unlocked_trigger_keep_codec_defaults() {
    let mut data = NbtCompound::new();
    let mut trigger = objective("unlock");
    trigger.put_string("CriteriaName", "trigger".into());
    data.put_list("Objectives", vec![NbtTag::Compound(trigger)]);
    let mut score = NbtCompound::new();
    score.put_string("Name", "Candidate".into());
    score.put_string("Objective", "unlock".into());
    score.put_int("Score", 13);
    data.put_list("PlayerScores", vec![NbtTag::Compound(score)]);
    let mut team = NbtCompound::new();
    team.put_string("Name", "test_team".into());
    team.put_string("DisplayName", "Test".into());
    team.put_string("TeamColor", "gold".into());
    team.put_bool("AllowFriendlyFire", false);
    team.put_string("NameTagVisibility", "never".into());
    team.put_string("CollisionRule", "pushOtherTeams".into());
    team.put_list(
        "Players",
        vec![text("0000006f-0000-00de-0000-014d000001bc")],
    );
    data.put_list("Teams", vec![NbtTag::Compound(team)]);
    let mut slots = NbtCompound::new();
    slots.put_string("sidebar", "unlock".into());
    data.put_compound("DisplaySlots", slots);
    let board = Scoreboard::from_saved_data(root(data)).unwrap();
    assert_eq!(board.get_score_value("Candidate", "unlock"), Some(13));
    assert!(!board.get_score("Candidate", "unlock").unwrap().locked);
    let team = board.get_team("test_team").unwrap();
    assert_eq!(team.options, 2);
    assert_eq!(team.nametag_visibility, NameTagVisibility::Never);
    assert_eq!(team.collision_rule, CollisionRule::PushOtherTeams);
    assert_eq!(team.color, NamedColor::Gold);
    assert_eq!(team.player_prefix, TextComponent::empty());
    assert_eq!(team.player_suffix, TextComponent::empty());
    assert_eq!(
        board.get_display_objective(ScoreboardDisplaySlot::Sidebar),
        Some("unlock")
    );
    let loaded = Scoreboard::from_saved_data(board.to_saved_data().unwrap()).unwrap();
    assert!(!loaded.get_score("Candidate", "unlock").unwrap().locked);
    assert_eq!(loaded.get_team("test_team").unwrap().players, team.players);
}

#[test]
fn shadow_color_and_nested_player_sprite_use_target_nbt_without_losing_model_fields() {
    let mut profile = NbtCompound::new();
    profile.put_string("name", "Example".into());
    let sprite = TextComponent::player_sprite(profile.clone(), false);
    let mut display = TextComponent::text("Player: ");
    display.0.extra.push(sprite.0.clone());
    display.0.style.shadow_color = Some(ARGBColor::new(128, 12, 34, 56));
    display.0.style.font = Some("minecraft:uniform".into());
    display.0.style.insertion = Some("example".into());
    display.0.style.underlined = Some(false);
    let encoded = encode_text(&display);
    assert_eq!(read_text(&encoded).unwrap(), display);
    let data = compound(&encoded).unwrap();
    assert_eq!(
        data.get_int("shadow_color"),
        Some(ARGBColor::new(128, 12, 34, 56).to_argb_int())
    );
    let child = compound(&data.get_list("extra").unwrap()[0]).unwrap();
    assert_eq!(child.get_string("type"), Some("object"));
    assert_eq!(child.get_string("object"), Some("player"));
    assert_eq!(child.get_compound("player"), Some(&profile));
    assert_eq!(child.get_bool("hat"), Some(false));
    let mut format = NbtCompound::new();
    let styled = NumberFormat::Styled((*display.0.style).clone());
    put_format(&mut format, Some(&styled)).unwrap();
    assert_eq!(read_format(format.get("format")).unwrap(), Some(styled));
}

#[test]
fn serialization_failure_does_not_truncate_a_previous_valid_scoreboard() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("data/minecraft/scoreboard.dat");
    let mut board = Scoreboard::from_saved_data(target_fixture()).unwrap();
    board.save(folder.path()).unwrap();
    let before = fs::read(&path).unwrap();
    // Java NBT strings have a u16 byte length. Exercise the real encoder failure
    // after staging starts, rather than only an injected write error.
    board.get_objective_mut("parity").unwrap().display_name =
        TextComponent::text("x".repeat(65_536));
    assert!(board.save(folder.path()).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    assert_eq!(
        Scoreboard::load(folder.path())
            .unwrap()
            .get_score_value("Query", "parity"),
        Some(-7)
    );
}
