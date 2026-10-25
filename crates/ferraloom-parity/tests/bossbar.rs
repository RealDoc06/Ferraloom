//! Live Strict BossBarCommands, initial saved bossbars and persistent viewer membership.
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_parity::{
    Bot, BotId, LaunchOptions, Normalization, ObservationLog, Result, RunMode, Session, Ticks,
    WorldPreset, ensure,
    protocol::{Wire, play::clientbound as cb},
};
use ferraloom_storage::{SavedData, SavedDataStorage};
use serde_json::{Value, json};

fn options() -> LaunchOptions {
    LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
            biome: "minecraft:plains".into(),
        },
        operators: vec!["ParityBot".into(), "ScoreGuest".into()],
        game_rules: [
            ("minecraft:random_tick_speed".into(), "0".into()),
            ("minecraft:spawn_mobs".into(), "false".into()),
        ]
        .into(),
        ferraloom_binary: std::env::var_os("FERRALOOM_PARITY_BINARY").map(Into::into),
        ..Default::default()
    }
}
async fn command(bot: &mut Bot, text: &str) -> Result<Vec<Value>> {
    bot.command(text).await?;
    // PingRequest is handled on vanilla's network thread; fence with an actual command instead.
    bot.command("scoreboard players get #boss_capture boss_capture")
        .await?;
    let mut packets = Vec::new();
    bot.wait_for(text,Ticks::DEFAULT,|_,packet| {
        if matches!(packet,cb::Packet::SystemChat(p) if p.content.to_json().is_ok_and(|text|text.contains("commands.scoreboard.players.get.success") && text.contains("#boss_capture"))) { return true; }
        if matches!(packet,cb::Packet::SystemChat(_) | cb::Packet::BossEvent(_)) {packets.push(packet.clone());}
        false
    }).await?;
    packets
        .into_iter()
        .map(|packet| {
            Ok(match packet {
                cb::Packet::BossEvent(p) => {
                    json!({"id":p.id.to_string(),"operation":p.operation.to_bytes()?})
                }
                cb::Packet::SystemChat(p) => {
                    let content: Value = serde_json::from_str(&p.content.to_json()?)?;
                    ensure!(
                        !content.to_string().contains("ferraloom."),
                        "{text}: unsupported feedback {content}"
                    );
                    json!({"chat":content,"overlay":p.overlay})
                }
                _ => unreachable!(),
            })
        })
        .collect()
}
fn record(
    log: &mut ObservationLog,
    session: &mut Session,
    label: &str,
    events: Vec<Value>,
) -> Result<()> {
    for (index, event) in events.iter().enumerate() {
        if event.get("id").is_some() {
            session.normalize(Normalization::RandomUuid(format!("/{label}/{index}/id")));
        }
    }
    log.record(label, events)
}
fn snapshot(session: &mut Session, player: BotId) -> Result<Vec<Value>> {
    let mut events=session.bot(player)?.snapshot().boss_events.into_iter().map(|(id,event)| {
        let name=serde_json::from_str::<Value>(&event.name.to_json()?)?;
        Ok(json!({"id":id.to_string(),"name":name,"progress_bits":event.progress.to_bits(),"color":event.color as u8,
            "overlay":event.overlay as u8,"darken":event.properties.darken_screen,"music":event.properties.play_music,"fog":event.properties.create_world_fog}))
    }).collect::<Result<Vec<_>>>()?;
    events.sort_by_key(|event| event["name"].to_string());
    Ok(events)
}
fn instantiate(input: &str) -> String {
    let id = if input.starts_with("bossbar add") {
        "test:added"
    } else {
        "test:bar"
    };
    [
        ("id", id),
        ("name", "\"Added\""),
        ("max", "5"),
        ("value", "3"),
        ("visible", "false"),
        ("targets", "@s"),
    ]
    .into_iter()
    .fold(input.to_owned(), |text, (name, value)| {
        text.replace(&format!("<{name}>"), value)
    })
}
async fn sweep(session: &mut Session) -> Result<ObservationLog> {
    let player = session.join("ParityBot").await?;
    let mut log = ObservationLog::default();
    let initial = snapshot(session, player)?;
    ensure!(
        initial.len() == 1 && initial[0]["progress_bits"] == 0.25_f32.to_bits(),
        "saved bossbar not restored: {initial:?}"
    );
    record(&mut log, session, "join.fixture", initial)?;
    let bot = session.bot(player)?;
    for (text, key) in [
        (
            "scoreboard objectives add boss_capture dummy",
            "commands.scoreboard.objectives.add.success",
        ),
        (
            "scoreboard players set #boss_capture boss_capture 1",
            "commands.scoreboard.players.set.success.single",
        ),
    ] {
        bot.command(text).await?;
        bot.wait_for(text,Ticks::DEFAULT,|_,packet|matches!(packet,cb::Packet::SystemChat(p) if p.content.to_json().is_ok_and(|text|text.contains(key)))).await?;
    }
    let paths: Vec<_> = include_str!("../../../features/commands.md")
        .lines()
        .filter_map(|line| {
            let columns: Vec<_> = line.split('|').collect();
            let id = columns.get(1)?.trim();
            if id != "commands.path.bossbar" && !id.starts_with("commands.path.bossbar.") {
                return None;
            }
            Some((
                id,
                columns[2].split('`').nth(1)?.strip_prefix('/')?,
                columns[2].contains("executable"),
            ))
        })
        .collect();
    ensure!(paths.len() == 39, "bossbar command path inventory changed");
    for (id, input, executable) in paths {
        let bot = session.bot(player)?;
        command(bot, "bossbar add test:bar \"Bar\"").await?;
        command(bot, "bossbar set test:bar players @s").await?;
        if input.starts_with("bossbar add") {
            command(bot, "bossbar remove test:added").await?;
        }
        let text = instantiate(input);
        ensure!(
            !text.contains(['<', '>']),
            "unbound command argument {text}"
        );
        let events = command(bot, &text).await?;
        ensure!(
            events.iter().any(|event| event.get("chat").is_some()),
            "{id}: missing feedback"
        );
        if executable {
            ensure!(
                !events
                    .iter()
                    .any(|event| event.to_string().contains("command.unknown")),
                "{id}: executable rejected"
            );
        }
        record(&mut log, session, id, events)?;
    }
    for (label, text) in [
        ("hide", "bossbar set test:bar visible false"),
        ("hidden-value", "bossbar set test:bar value 4"),
        ("hidden-name", "bossbar set test:bar name \"Hidden\""),
        ("show", "bossbar set test:bar visible true"),
        ("clamp-value", "bossbar set test:bar value 2147483647"),
        ("clamp-max", "bossbar set test:bar max 1"),
        ("dynamic-name", "bossbar set test:bar name {selector:'@s'}"),
    ] {
        let events = command(session.bot(player)?, text).await?;
        record(&mut log, session, label, events)?;
    }
    // Sixteen colliding Identifier hashes exercise native HashMap tree-bin list order.
    for index in 0..16 {
        let path: String = (0..4)
            .map(|bit| if index & (1 << bit) == 0 { "an" } else { "c0" })
            .collect();
        let events = command(
            session.bot(player)?,
            &format!("bossbar add test:{path} \"{path}\""),
        )
        .await?;
        record(&mut log, session, &format!("collision.add.{index}"), events)?;
    }
    let events = command(session.bot(player)?, "bossbar list").await?;
    record(&mut log, session, "collision.list", events)?;
    let guest = session.join("ScoreGuest").await?;
    let initial = snapshot(session, guest)?;
    ensure!(
        initial.len() == 1,
        "late join must receive only its saved bossbar membership: {initial:?}"
    );
    record(&mut log, session, "late.fixture", initial)?;
    for (label, text) in [
        ("players-all", "bossbar set test:bar players @a"),
        ("players-get", "bossbar get test:bar players"),
        ("players-none", "bossbar set test:bar players"),
        ("players-unchanged", "bossbar set test:bar players"),
        ("players-saved", "bossbar set test:bar players @s"),
        ("fixture-saved", "bossbar set test:loaded players @s"),
    ] {
        let events = command(session.bot(guest)?, text).await?;
        record(&mut log, session, label, events)?;
    }
    session.stop().await?;
    let saved = session
        .saved_world()?
        .saved_data("minecraft:custom_boss_events")?
        .ok_or_else(|| anyhow::anyhow!("custom boss events missing after save"))?;
    // Sort compound keys only; preserve every NBT value/type and every membership list entry.
    log.record("saved", ferraloom_nbt::to_text(&saved, "", true))?;
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla jar/JDK; run with --ignored"]
async fn parity_bossbar_commands() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let mut event = CompoundTag::new();
    event.insert("Name", Tag::String("Loaded".into()));
    event.insert("Visible", Tag::Byte(1));
    event.insert("Value", Tag::Int(25));
    event.insert("Color", Tag::String("purple".into()));
    event.insert("Overlay", Tag::String("notched_12".into()));
    for flag in ["DarkenScreen", "PlayBossMusic", "CreateWorldFog"] {
        event.insert(flag, Tag::Byte(1));
    }
    event.insert(
        "Players",
        Tag::List(ListTag(
            ["ParityBot", "ScoreGuest"]
                .into_iter()
                .map(|name| {
                    Tag::IntArray(
                        ferraloom_parity::protocol::Uuid::offline_player(name)
                            .to_int_array()
                            .to_vec(),
                    )
                })
                .collect(),
        )),
    );
    let mut data = CompoundTag::new();
    data.insert("test:loaded", Tag::Compound(event));
    SavedDataStorage::new(fixture.path().join("data")).save(
        "minecraft:custom_boss_events",
        &mut SavedData::new(Tag::Compound(data)),
    )?;
    let mut options = options();
    options.fixture = Some(fixture.path().to_path_buf());
    ferraloom_parity::run(options, RunMode::from_env()?, sweep).await
}
