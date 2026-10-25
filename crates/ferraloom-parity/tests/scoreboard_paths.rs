//! Live Strict command-tree paths and scoreboard/team state changes, batched per server.
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, WorldPreset, ensure,
    protocol::{Wire, play::clientbound as cb},
};
use serde_json::{Value, json};

fn options() -> LaunchOptions {
    LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
            biome: "minecraft:plains".into(),
        },
        game_rules: [
            ("minecraft:random_tick_speed".into(), "0".into()),
            ("minecraft:spawn_mobs".into(), "false".into()),
        ]
        .into(),
        ferraloom_binary: std::env::var_os("FERRALOOM_PARITY_BINARY").map(Into::into),
        operators: vec!["ParityBot".into(), "ScoreGuest".into()],
        ..Default::default()
    }
}

/// Capture only this command's feedback and scoreboard packets, in emission order.
/// A second command ends the capture on the server thread. Vanilla answers
/// PingRequest on the network thread, so a pong cannot fence command execution.
async fn command(bot: &mut Bot, text: &str) -> Result<Vec<Value>> {
    bot.command(text).await?;
    bot.command("scoreboard players get #capture_barrier capture_barrier")
        .await?;
    let mut packets = Vec::new();
    bot.wait_for(text, Ticks::DEFAULT, |_, packet| {
        if matches!(packet, cb::Packet::SystemChat(p) if p.content.to_json().is_ok_and(|text|
            text.contains("commands.scoreboard.players.get.success") && text.contains("#capture_barrier"))) {
            return true;
        }
        if matches!(
            packet,
            cb::Packet::SystemChat(_)
                | cb::Packet::SetObjective(_)
                | cb::Packet::SetPlayerTeam(_)
                | cb::Packet::SetScore(_)
                | cb::Packet::ResetScore(_)
                | cb::Packet::SetDisplayObjective(_)
        ) {
            packets.push(packet.clone());
        }
        false
    })
    .await?;
    packets
        .into_iter()
        .map(|packet| {
            Ok(match packet {
                cb::Packet::SystemChat(p) => {
                    let content: Value = serde_json::from_str(&p.content.to_json()?)?;
                    ensure!(
                        !content.to_string().contains("ferraloom."),
                        "{text}: unsupported feedback {content}"
                    );
                    json!({"chat":content,"overlay":p.overlay})
                }
                packet => json!({"packet": packet.to_bytes()?}),
            })
        })
        .collect()
}

fn paths() -> Vec<(&'static str, &'static str, bool)> {
    include_str!("../../../features/commands.md")
        .lines()
        .filter_map(|line| {
            let mut columns = line.split('|');
            columns.next()?;
            let id = columns.next()?.trim();
            let root = id.strip_prefix("commands.path.")?.split('.').next()?;
            if !["scoreboard", "team", "trigger"].contains(&root) {
                return None;
            }
            let feature = columns.next()?;
            let input = feature.split('`').nth(1)?.strip_prefix('/')?;
            Some((id, input, feature.contains("executable")))
        })
        .collect()
}

fn instantiate(input: &str) -> String {
    let mut result = input.to_owned();
    let objective =
        if input.starts_with("trigger") || input.starts_with("scoreboard players enable") {
            "toggle"
        } else if input.starts_with("scoreboard objectives add") {
            "added"
        } else {
            "points"
        };
    let team = if input.starts_with("team add") {
        "added"
    } else {
        "sweep"
    };
    let value = if input.starts_with("trigger") {
        "3"
    } else if input.ends_with("color <value>") {
        "blue"
    } else {
        "true"
    };
    for (name, replacement) in [
        ("objective", objective),
        ("criteria", "dummy"),
        ("slot", "sidebar"),
        ("target", "#holder"),
        ("targets", "#holder"),
        ("targetObjective", "points"),
        ("source", "#source"),
        ("sourceObjective", "source"),
        ("operation", "+="),
        ("score", "3"),
        ("team", team),
        ("members", "ParityBot"),
        ("displayName", "\"Changed\""),
        ("name", "\"Holder\""),
        ("contents", "\"Fixed\""),
        ("style", "{color:'gold'}"),
        ("prefix", "\"[\""),
        ("suffix", "\"]\""),
        ("allowed", "false"),
        ("value", value),
    ] {
        result = result.replace(&format!("<{name}>"), replacement);
    }
    result
}

async fn sweep(session: &mut Session) -> Result<ObservationLog> {
    let player = session.join("ParityBot").await?;
    let bot = session.bot(player)?;
    let mut log = ObservationLog::default();
    for (text, key) in [
        (
            "scoreboard objectives add capture_barrier dummy",
            "commands.scoreboard.objectives.add.success",
        ),
        (
            "scoreboard players set #capture_barrier capture_barrier 1",
            "commands.scoreboard.players.set.success.single",
        ),
    ] {
        bot.command(text).await?;
        bot.wait_for(text, Ticks::DEFAULT, |_, packet| {
            matches!(packet,
            cb::Packet::SystemChat(p) if p.content.to_json().is_ok_and(|text| text.contains(key)))
        })
        .await?;
    }
    for setup in [
        "scoreboard objectives add points dummy",
        "scoreboard objectives add source dummy",
        "scoreboard objectives add toggle trigger",
        "scoreboard players set #source source 3",
        "scoreboard objectives setdisplay sidebar points",
        "team add sweep",
    ] {
        command(bot, setup).await?;
    }
    let paths = paths();
    ensure!(paths.len() == 121, "owned command path inventory changed");
    for (id, input, executable) in paths {
        // Restore only native command state that another covered path can consume.
        for setup in [
            "scoreboard objectives add points dummy",
            "scoreboard players set #holder points 7",
            "scoreboard objectives setdisplay sidebar points",
            "team add sweep",
            "team join sweep #holder",
            "scoreboard players enable ParityBot toggle",
        ] {
            command(bot, setup).await?;
        }
        if input.starts_with("scoreboard objectives add") {
            command(bot, "scoreboard objectives remove added").await?;
        }
        if input.starts_with("team add") {
            command(bot, "team remove added").await?;
        }
        if input.starts_with("team leave") {
            command(bot, "team join sweep ParityBot").await?;
        }
        let text = instantiate(input);
        ensure!(
            !text.contains(['<', '>']),
            "unbound command argument: {text}"
        );
        let mut observed = command(bot, &text).await?;
        ensure!(
            observed.iter().any(|event| event.get("chat").is_some()),
            "{id}: missing command feedback"
        );
        if executable {
            ensure!(
                !observed
                    .iter()
                    .any(|event| event.to_string().contains("command.unknown")),
                "{id}: executable path rejected: {observed:?}"
            );
        }
        if id == "commands.path.scoreboard.players.list.arg-target" {
            // PlayerScores keys this independent entry list by Objective identity hash.
            // Only these entry lines vary with JVM allocation; the header and all other
            // commands and packets retain their original emission order.
            ensure!(
                observed
                    .first()
                    .is_some_and(|event| event["chat"]["translate"]
                        == "commands.scoreboard.players.list.entity.success"),
                "{id}: missing list header"
            );
            ensure!(
                observed[1..].iter().all(|event| event["chat"]["translate"]
                    == "commands.scoreboard.players.list.entity.entry"),
                "{id}: unexpected entry/packet shape"
            );
            observed[1..].sort_by_key(Value::to_string);
        }
        log.record(id, observed)?;
    }

    // These inventory rows describe criterion registration, not automatic producers.
    let criteria: Vec<_> = include_str!("../../../features/scoreboard-teams-bossbars.md")
        .lines()
        .filter(|line| {
            line.starts_with("| scoreboard-teams-bossbars.criterion.")
                && line.contains("Scoreboard criterion `")
        })
        .map(|line| {
            (
                line.split('|').nth(1).unwrap().trim(),
                line.split('`').nth(1).unwrap(),
            )
        })
        .collect();
    ensure!(criteria.len() == 43, "custom criterion inventory changed");
    for (id, criterion) in criteria {
        let mut observed = command(
            bot,
            &format!("scoreboard objectives add criterion {criterion}"),
        )
        .await?;
        ensure!(
            observed.iter().any(|event| event
                .to_string()
                .contains("commands.scoreboard.objectives.add.success")),
            "{criterion}: objective creation failed"
        );
        observed.extend(command(bot, "scoreboard objectives setdisplay list criterion").await?);
        observed.extend(command(bot, "scoreboard players set #criterion criterion 5").await?);
        observed.extend(command(bot, "scoreboard objectives remove criterion").await?);
        log.record(id, observed)?;
    }
    for slot in [
        "list",
        "sidebar",
        "below_name",
        "sidebar.team.black",
        "sidebar.team.dark_blue",
        "sidebar.team.dark_green",
        "sidebar.team.dark_aqua",
        "sidebar.team.dark_red",
        "sidebar.team.dark_purple",
        "sidebar.team.gold",
        "sidebar.team.gray",
        "sidebar.team.dark_gray",
        "sidebar.team.blue",
        "sidebar.team.green",
        "sidebar.team.aqua",
        "sidebar.team.red",
        "sidebar.team.light_purple",
        "sidebar.team.yellow",
        "sidebar.team.white",
    ] {
        log.record(
            format!("slot.{slot}"),
            command(
                bot,
                &format!("scoreboard objectives setdisplay {slot} points"),
            )
            .await?,
        )?;
    }
    for criterion in [
        "minecraft.mined:minecraft.stone",
        "minecraft.crafted:minecraft.stick",
        "minecraft.used:minecraft.stick",
        "minecraft.broken:minecraft.iron_pickaxe",
        "minecraft.picked_up:minecraft.stick",
        "minecraft.dropped:minecraft.stick",
        "minecraft.killed:minecraft.cow",
        "minecraft.killed_by:minecraft.cow",
        "minecraft.custom:minecraft.jump",
    ] {
        let mut observed =
            command(bot, &format!("scoreboard objectives add stat {criterion}")).await?;
        observed.extend(command(bot, "scoreboard players set #stat stat 4").await?);
        ensure!(
            observed
                .iter()
                .filter(|event| event.get("chat").is_some())
                .all(|event| event.to_string().contains("success")),
            "stat criterion must be writable: {criterion}"
        );
        observed.extend(command(bot, "scoreboard objectives remove stat").await?);
        log.record(format!("stat-writable.{criterion}"), observed)?;
    }
    // Exercise all operation bodies, signed floor division/remainder and partial failure.
    for operation in ["=", "+=", "-=", "*=", "/=", "%=", "<", ">", "><"] {
        command(bot, "scoreboard players set #holder points -7").await?;
        log.record(
            format!("operation.{operation}"),
            command(
                bot,
                &format!("scoreboard players operation #holder points {operation} #source source"),
            )
            .await?,
        )?;
    }
    command(bot, "scoreboard players set #source source 0").await?;
    log.record(
        "operation.zero",
        command(
            bot,
            "scoreboard players operation #holder points /= #source source",
        )
        .await?,
    )?;
    command(bot, "scoreboard objectives add readonly health").await?;
    command(bot, "scoreboard players set #holder points 5").await?;
    log.record(
        "operation.readonly-swap",
        command(
            bot,
            "scoreboard players operation #holder points >< #readonly readonly",
        )
        .await?,
    )?;
    log.record(
        "operation.readonly-target",
        command(bot, "scoreboard players get #holder points").await?,
    )?;
    for (label, text) in [
        ("zero.set", "scoreboard players set #holder points 0"),
        ("zero.repeat", "scoreboard players set #holder points 0"),
        ("zero.add", "scoreboard players add #holder points 0"),
        ("zero.remove", "scoreboard players remove #holder points 0"),
    ] {
        log.record(label, command(bot, text).await?)?;
    }
    command(bot, "scoreboard players enable ParityBot toggle").await?;
    log.record(
        "trigger.primed",
        command(bot, "trigger toggle set 8").await?,
    )?;
    log.record("trigger.locked", command(bot, "trigger toggle").await?)?;
    // Late join must receive current objective formats, slots and scores.
    command(bot, "team add synced").await?;
    command(bot, "team join synced ParityBot").await?;
    command(bot, "team modify synced color red").await?;
    command(bot, "team modify synced prefix \"[\"").await?;
    command(bot, "team modify synced suffix \"]\"").await?;
    log.record(
        "team.single-rejoin",
        command(bot, "team join synced ParityBot").await?,
    )?;
    command(bot, "scoreboard players set #holder points 42").await?;
    let guest = session.join("ScoreGuest").await?;
    let bot = session.bot(guest)?;
    bot.synchronize().await?;
    let state = bot.snapshot();
    ensure!(
        state.scores.get(&("points".into(), "#holder".into())) == Some(&42),
        "initial scoreboard sync omitted current score"
    );
    ensure!(
        state.objectives.contains_key("points"),
        "initial scoreboard sync omitted objective"
    );
    log.record(
        "join.objectives",
        state
            .objectives
            .into_iter()
            .map(|(name, value)| Ok((name, value.to_bytes()?)))
            .collect::<Result<Vec<_>>>()?,
    )?;
    log.record(
        "join.slots",
        state
            .display_objectives
            .iter()
            .map(Wire::to_bytes)
            .collect::<std::result::Result<Vec<_>, _>>()?,
    )?;
    log.record("join.scores", state.scores.into_iter().collect::<Vec<_>>())?;
    for (label, text) in [
        ("join.multiple", "team join synced @a"),
        ("join.repeated", "team join synced @a"),
        ("leave.multiple", "team leave @a"),
        ("leave.unchanged", "team leave @a"),
        ("remove.tracked", "scoreboard objectives remove points"),
    ] {
        log.record(label, command(bot, text).await?)?;
    }
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla jar/JDK; run with --ignored"]
async fn parity_scoreboard_team_trigger_paths() -> Result<()> {
    ferraloom_parity::run(options(), RunMode::from_env()?, sweep).await
}
