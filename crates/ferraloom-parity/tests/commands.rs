//! Exact command feedback and packet variants, using disposable single-region servers.
use ferraloom_parity::{
    Bot, ObservationLog, Result, Session, Ticks, ensure,
    protocol::{chat::Component, play::clientbound as cb},
};
use serde_json::Value;

mod command_support;
use command_support::command_test;

fn json(component: &Component) -> Result<Value> {
    Ok(serde_json::from_str(&component.to_json()?)?)
}
fn has_translation(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(fields) => {
            fields.get("translate").and_then(Value::as_str) == Some(key)
                || fields.values().any(|value| has_translation(value, key))
        }
        Value::Array(values) => values.iter().any(|value| has_translation(value, key)),
        _ => false,
    }
}
async fn feedback(bot: &mut Bot, key: &str) -> Result<Value> {
    let packet = bot.wait_for(key, Ticks::DEFAULT, |_, packet| {
        matches!(packet, cb::Packet::SystemChat(packet) if json(&packet.content).is_ok_and(|value| has_translation(&value,key)))
    }).await?;
    let cb::Packet::SystemChat(packet) = packet.as_ref() else {
        unreachable!()
    };
    ensure!(!packet.overlay, "command feedback must use chat");
    json(&packet.content)
}
async fn command(bot: &mut Bot, text: &str, key: &str) -> Result<Value> {
    bot.command(text).await?;
    feedback(bot, key).await
}

async fn game_rules(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    let mut log = ObservationLog::default();
    for (label, text, key) in [
        (
            "set",
            "gamerule keep_inventory true",
            "commands.gamerule.set",
        ),
        (
            "query",
            "gamerule minecraft:keep_inventory",
            "commands.gamerule.query",
        ),
        (
            "no_op",
            "gamerule keep_inventory true",
            "commands.gamerule.not_set",
        ),
        (
            "bound",
            "gamerule max_snow_accumulation_height 9",
            "argument.integer.big",
        ),
    ] {
        log.record(label, command(bot, text, key).await?)?;
        if label == "bound" {
            log.record(
                "bound.context",
                feedback(bot, "command.context.here").await?,
            )?;
        }
    }
    for (label, text, event) in [
        ("debug", "gamerule reduced_debug_info true", 22),
        ("respawn", "gamerule immediate_respawn true", 11),
        ("crafting", "gamerule limited_crafting true", 12),
    ] {
        bot.command(text).await?;
        let packet = bot.wait_for(label,Ticks::DEFAULT,|_,packet| {
            if event == 22 { matches!(packet, cb::Packet::EntityEvent(packet) if packet.event_id==event) }
            else { matches!(packet, cb::Packet::GameEvent(packet) if packet.event==event as u8 && packet.param==1.) }
        }).await?;
        log.record(
            format!("{label}.packet"),
            match packet.as_ref() {
                cb::Packet::EntityEvent(packet) => format!("event {}", packet.event_id),
                cb::Packet::GameEvent(packet) => {
                    format!("game event {} {}", packet.event, packet.param)
                }
                _ => unreachable!(),
            },
        )?;
        log.record(
            format!("{label}.feedback"),
            feedback(bot, "commands.gamerule.set").await?,
        )?;
    }
    // The changed rule suppresses its own feedback; a packet-order barrier proves the absence.
    bot.synchronize().await?;
    let before = bot.snapshot().messages.len();
    bot.command("gamerule send_command_feedback false").await?;
    bot.synchronize().await?;
    ensure!(
        bot.snapshot().messages.len() == before,
        "disabled feedback produced a source message"
    );
    log.record("disabled.feedback_count", 0)?;
    log.record(
        "enabled",
        command(
            bot,
            "gamerule send_command_feedback true",
            "commands.gamerule.set",
        )
        .await?,
    )?;
    Ok(log)
}

async fn clocks_and_admin(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    let mut log = ObservationLog::default();
    for (index, text, key) in [
        (0, "gamerule advance_time false", "commands.gamerule.set"),
        (1, "time set 1000", "commands.time.set.absolute"),
        (20, "time set 1000", "commands.time.set.already_at_time"),
        (2, "time query time", "commands.time.query.absolute"),
        (3, "time add 20", "commands.time.set.absolute"),
        (4, "time pause", "commands.time.pause"),
        (5, "time pause", "commands.time.pause.already_paused"),
        (6, "time rate 2", "commands.time.rate"),
        (7, "time rate 2", "commands.time.rate.already_same"),
        (8, "time resume", "commands.time.resume"),
        (9, "seed", "commands.seed.success"),
        (
            10,
            "setidletimeout 0",
            "commands.setidletimeout.success.disabled",
        ),
        (
            11,
            "defaultgamemode creative",
            "commands.defaultgamemode.success",
        ),
        (12, "save-off", "commands.save.disabled"),
        (13, "save-off", "commands.save.alreadyOff"),
        (14, "save-on", "commands.save.enabled"),
        (15, "save-on", "commands.save.alreadyOn"),
        (16, "difficulty", "commands.difficulty.query"),
        (17, "difficulty hard", "commands.difficulty.success"),
        (18, "difficulty hard", "commands.difficulty.failure"),
        (19, "save-all flush", "commands.save.saving"),
    ] {
        log.record(format!("step.{index}"), command(bot, text, key).await?)?;
    }
    log.record(
        "save.complete",
        feedback(bot, "commands.save.success").await?,
    )?;
    log.record(
        "help.error",
        command(bot, "help no_such_command", "commands.help.failed").await?,
    )?;
    log.record(
        "permission.error",
        command(
            bot,
            "op @a[limit=0]",
            "argument.entity.options.limit.toosmall",
        )
        .await?,
    )?;
    log.record(
        "permission.context",
        feedback(bot, "command.context.here").await?,
    )?;
    // `day` is a timeline resource in 26.3, so its short identifier remains valid.
    log.record(
        "day.timeline",
        command(bot, "time query day", "commands.time.query.timeline").await?,
    )?;
    log.record(
        "legacy.daytime",
        command(bot, "time query daytime", "argument.resource.not_found").await?,
    )?;
    log.record(
        "legacy.daytime.context",
        feedback(bot, "command.context.here").await?,
    )?;
    let gametime = command(bot, "time query gametime", "commands.time.query.gametime").await?;
    ensure!(
        gametime["with"][0].as_i64().is_some_and(|time| time >= 0),
        "gametime feedback requires the full numeric long value"
    );
    log.record("gametime.translation", &gametime["translate"])?;
    let newcomer = session.join("DifficultyGuest").await?;
    let difficulty = session
        .bot(newcomer)?
        .snapshot()
        .difficulty
        .expect("joining player's difficulty packet");
    ensure!(
        difficulty.difficulty == ferraloom_parity::protocol::play::Difficulty::Hard,
        "joining player's packet lost changed world difficulty"
    );
    ensure!(
        !difficulty.locked,
        "normal world difficulty must remain unlocked"
    );
    log.record("difficulty.newcomer_packet", format!("{difficulty:?}"))?;
    let difficulty = command(session.bot(id)?, "difficulty", "commands.difficulty.query").await?;
    ensure!(
        has_translation(&difficulty, "options.difficulty.hard"),
        "joining a player reset the changed world difficulty"
    );
    log.record("difficulty.after_join", difficulty)?;
    Ok(log)
}

async fn forced_default_game_mode(session: &mut Session) -> Result<ObservationLog> {
    let operator = session.join("ParityBot").await?;
    let guest = session.join("CommandGuest").await?;
    session
        .bot(operator)?
        .command("defaultgamemode creative")
        .await?;
    let mut log = ObservationLog::default();
    for (name, id) in [("operator", operator), ("guest", guest)] {
        let packet = session.bot(id)?.wait_for("forced default game mode",Ticks::DEFAULT,|_,packet| {
            matches!(packet,cb::Packet::GameEvent(packet) if packet.event==3 && packet.param==1.)
        }).await?;
        log.record(format!("{name}.mode"), format!("{packet:?}"))?;
    }
    log.record(
        "feedback",
        feedback(session.bot(operator)?, "commands.defaultgamemode.success").await?,
    )?;
    log.record(
        "no_op",
        command(
            session.bot(operator)?,
            "defaultgamemode creative",
            "commands.defaultgamemode.success",
        )
        .await?,
    )?;
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla jar/JDK; run with --ignored"]
async fn parity_command_forced_default_game_mode() -> Result<()> {
    let mut options = command_support::options();
    options
        .properties
        .insert("force-gamemode".into(), "true".into());
    ferraloom_parity::run(
        options,
        ferraloom_parity::RunMode::from_env()?,
        forced_default_game_mode,
    )
    .await
}

async fn message_arguments(session: &mut Session) -> Result<ObservationLog> {
    let sender = session.join("ParityBot").await?;
    let witness = session.join("CommandGuest").await?;
    let mut log = ObservationLog::default();
    for (index, signed, text) in [
        (0, false, "say hello @s"),
        (1, true, "say signed @s"),
        (2, false, "me waves at @a"),
        (3, true, "me signs a wave"),
        (4, false, "msg CommandGuest hello @s"),
        (5, true, "msg cOmMaNdGuEsT signed hello @s"),
    ] {
        let bot = session.bot(sender)?;
        if signed {
            bot.signed_command(text).await?;
        } else {
            bot.command(text).await?;
        }
        let packet = session
            .bot(witness)?
            .wait_for("message argument delivery", Ticks::DEFAULT, |_, packet| {
                matches!(
                    packet,
                    cb::Packet::PlayerChat(_) | cb::Packet::DisguisedChat(_)
                )
            })
            .await?;
        let value = match packet.as_ref() {
            cb::Packet::DisguisedChat(packet) => {
                ensure!(!signed, "plain signing context distinction lost");
                serde_json::json!({"packet":"DisguisedChat","message":json(&packet.message)?,"sender":json(&packet.chat_type.name)?,"target":packet.chat_type.target_name.as_ref().map(json).transpose()?,"type":format!("{:?}",packet.chat_type.chat_type)})
            }
            cb::Packet::PlayerChat(packet) => {
                ensure!(signed, "absent signing context produced PlayerChat");
                serde_json::json!({"packet":"PlayerChat","message":packet.body.content.0,"unsigned":packet.unsigned_content.as_ref().map(json).transpose()?,"sender":json(&packet.chat_type.name)?,"target":packet.chat_type.target_name.as_ref().map(json).transpose()?,"type":format!("{:?}",packet.chat_type.chat_type)})
            }
            _ => unreachable!(),
        };
        log.record(format!("message.{index}"), value)?;
    }
    log.record(
        "team.no_team",
        command(
            session.bot(sender)?,
            "teammsg before joining",
            "commands.teammsg.failed.noteam",
        )
        .await?,
    )?;
    for (text, key) in [
        (
            "team add parity \"Parity Team\"",
            "commands.team.add.success",
        ),
        (
            "team modify parity color gold",
            "commands.team.option.color.success",
        ),
        ("team join parity @a", "commands.team.join.success.multiple"),
        (
            "scoreboard objectives add selection dummy",
            "commands.scoreboard.objectives.add.success",
        ),
        (
            "scoreboard players set ParityBot selection 7",
            "commands.scoreboard.players.set.success.single",
        ),
    ] {
        command(session.bot(sender)?, text, key).await?;
    }
    for (index, text) in [
        "say selected @a[team=parity,scores={selection=7..}]",
        "say missing @a[team=!parity,scores={absent=1..}]",
    ]
    .into_iter()
    .enumerate()
    {
        session.bot(sender)?.command(text).await?;
        let packet = session
            .bot(witness)?
            .wait_for(
                "score/team message selector",
                Ticks::DEFAULT,
                |_, packet| matches!(packet, cb::Packet::DisguisedChat(_)),
            )
            .await?;
        let cb::Packet::DisguisedChat(packet) = packet.as_ref() else {
            unreachable!()
        };
        log.record(format!("selector.{index}"), json(&packet.message)?)?;
    }
    let outsider = session.join("OutsideTeam").await?;
    session.bot(outsider)?.synchronize().await?;
    let outside_messages = session.bot(outsider)?.snapshot().messages.len();
    for (index, signed, text) in [
        (0, false, "teammsg plain team"),
        (1, true, "tm signed team"),
    ] {
        let bot = session.bot(sender)?;
        if signed {
            bot.signed_command(text).await?;
        } else {
            bot.command(text).await?;
        }
        for (recipient, expected_type) in [
            (sender, "minecraft:team_msg_command_outgoing"),
            (witness, "minecraft:team_msg_command_incoming"),
        ] {
            let packet=session.bot(recipient)?.wait_for("team message variant and direction",Ticks::DEFAULT,|_,packet| {
                let binding=match packet { cb::Packet::DisguisedChat(packet)=>&packet.chat_type,cb::Packet::PlayerChat(packet)=>&packet.chat_type,_=>return false };
                matches!(&binding.chat_type.0,ferraloom_parity::protocol::codec::Holder::Reference(id) if id.as_str()==expected_type)
            }).await?;
            let (kind, message, binding) = match packet.as_ref() {
                cb::Packet::DisguisedChat(packet) => {
                    ensure!(!signed, "unsigned team message used wrong packet variant");
                    ("DisguisedChat", json(&packet.message)?, &packet.chat_type)
                }
                cb::Packet::PlayerChat(packet) => {
                    ensure!(signed, "signed team message used wrong packet variant");
                    (
                        "PlayerChat",
                        serde_json::json!({"content":packet.body.content.0,"unsigned":packet.unsigned_content.as_ref().map(json).transpose()?}),
                        &packet.chat_type,
                    )
                }
                _ => unreachable!(),
            };
            log.record(format!("team.{index}.{expected_type}"),serde_json::json!({"kind":kind,"message":message,"name":json(&binding.name)?,"target":binding.target_name.as_ref().map(json).transpose()?}))?;
        }
    }
    session.bot(outsider)?.synchronize().await?;
    ensure!(
        session.bot(outsider)?.snapshot().messages.len() == outside_messages,
        "team message reached a nonmember"
    );
    log.record("team.outsider_messages", 0)?;
    for (label, text, key) in [
        (
            "leave",
            "team leave @s",
            "commands.team.leave.success.single",
        ),
        (
            "join",
            "team join parity @s",
            "commands.team.join.success.single",
        ),
        (
            "rejoin",
            "team join parity @s",
            "commands.team.join.success.single",
        ),
        (
            "leave_again",
            "team leave @s",
            "commands.team.leave.success.single",
        ),
    ] {
        log.record(
            format!("team.single.{label}"),
            command(session.bot(sender)?, text, key).await?,
        )?;
    }
    Ok(log)
}

async fn spawn_and_return(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    let mut log = ObservationLog::default();
    log.record(
        "world.feedback",
        command(
            bot,
            "setworldspawn 1 80 -2 450 100",
            "commands.setworldspawn.success",
        )
        .await?,
    )?;
    log.record(
        "player.feedback",
        command(
            bot,
            "spawnpoint @s 3 90 -4 ~30 ~-20",
            "commands.spawnpoint.success.single",
        )
        .await?,
    )?;
    log.record(
        "return.feedback",
        command(
            bot,
            "return run gamerule keep_inventory true",
            "commands.gamerule.set",
        )
        .await?,
    )?;
    log.record(
        "return.value",
        command(bot, "gamerule keep_inventory", "commands.gamerule.query").await?,
    )?;
    session.stop().await?;
    let world = session.saved_world()?;
    let level = world.level()?;
    let player = world.player("ParityBot")?;
    let spawn = level.data.get_compound("spawn").expect("world spawn");
    let respawn = player
        .get_compound("respawn")
        .expect("forced player respawn");
    ensure!(
        spawn.get_float_or("yaw", 0.) == 90. && spawn.get_float_or("pitch", 0.) == 90.,
        "world spawn rotation lost"
    );
    ensure!(
        respawn.get_boolean_or("forced", false),
        "player spawn must be forced"
    );
    log.record("world.spawn", spawn.to_string())?;
    log.record("player.spawn", respawn.to_string())?;
    Ok(log)
}

command_test!(parity_command_game_rules, game_rules);
command_test!(parity_command_clocks_and_admin, clocks_and_admin);
command_test!(parity_command_message_arguments, message_arguments);
command_test!(parity_command_spawn_and_return, spawn_and_return);

async fn access_commands(session: &mut Session) -> Result<ObservationLog> {
    let operator = session.join("ParityBot").await?;
    let guest = session.join("CommandGuest").await?;
    let mut log = ObservationLog::default();
    log.record(
        "op",
        command(
            session.bot(operator)?,
            "op CommandGuest",
            "commands.op.success",
        )
        .await?,
    )?;
    session
        .bot(guest)?
        .wait_for(
            "operator permission",
            Ticks::DEFAULT,
            |_, packet| matches!(packet,cb::Packet::EntityEvent(packet) if packet.event_id==28),
        )
        .await?;
    log.record("op.permission", 28)?;
    log.record(
        "op.noop",
        command(
            session.bot(operator)?,
            "op CommandGuest",
            "commands.op.failed",
        )
        .await?,
    )?;
    log.record(
        "deop",
        command(
            session.bot(operator)?,
            "deop CommandGuest",
            "commands.deop.success",
        )
        .await?,
    )?;
    session
        .bot(guest)?
        .wait_for(
            "removed permission",
            Ticks::DEFAULT,
            |_, packet| matches!(packet,cb::Packet::EntityEvent(packet) if packet.event_id==24),
        )
        .await?;
    log.record("deop.permission", 24)?;
    log.record(
        "nonop.seed",
        command(session.bot(guest)?, "seed", "command.unknown.command").await?,
    )?;
    log.record(
        "nonop.context",
        feedback(session.bot(guest)?, "command.context.here").await?,
    )?;
    // The uncached profile below exceeds 16 characters, so vanilla takes its deterministic
    // offline fallback without consulting the external account repository.
    for (index, text, key) in [
        (
            0,
            "whitelist add CommandGuest",
            "commands.whitelist.add.success",
        ),
        (
            1,
            "whitelist add CommandGuest",
            "commands.whitelist.add.failed",
        ),
        (2, "whitelist list", "commands.whitelist.list"),
        (
            3,
            "whitelist remove CommandGuest",
            "commands.whitelist.remove.success",
        ),
        (
            4,
            "whitelist remove CommandGuest",
            "commands.whitelist.remove.failed",
        ),
        (5, "whitelist on", "commands.whitelist.enabled"),
        (6, "whitelist off", "commands.whitelist.disabled"),
        (
            7,
            "ban AbsentCommandProfile reason @s",
            "commands.ban.success",
        ),
        (8, "ban AbsentCommandProfile", "commands.ban.failed"),
        (9, "banlist players", "commands.banlist.list"),
    ] {
        log.record(
            format!("access.{index}"),
            command(session.bot(operator)?, text, key).await?,
        )?;
    }
    log.record(
        "ban.entry",
        feedback(session.bot(operator)?, "commands.banlist.entry").await?,
    )?;
    for (index, text, key) in [
        (10, "pardon AbsentCommandProfile", "commands.pardon.success"),
        (11, "pardon AbsentCommandProfile", "commands.pardon.failed"),
        (12, "banlist players", "commands.banlist.none"),
        (13, "ban-ip 198.51.100.9", "commands.banip.success"),
        (14, "banlist ips", "commands.banlist.list"),
    ] {
        log.record(
            format!("access.{index}"),
            command(session.bot(operator)?, text, key).await?,
        )?;
    }
    log.record(
        "ban_ip.entry",
        feedback(session.bot(operator)?, "commands.banlist.entry").await?,
    )?;
    for (index, text, key) in [
        (15, "pardon-ip 198.51.100.9", "commands.pardonip.success"),
        (16, "pardon-ip 198.51.100.9", "commands.pardonip.failed"),
        (17, "pardon-ip invalid", "commands.pardonip.invalid"),
        (18, "list uuids", "commands.list.players"),
        (19, "kick cOmMaNdGuEsT", "commands.kick.success"),
    ] {
        log.record(
            format!("access.{index}"),
            command(session.bot(operator)?, text, key).await?,
        )?;
    }
    Ok(log)
}
command_test!(parity_command_access_lists, access_commands);

async fn force_load_and_weather(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let mut log = ObservationLog::default();
    for (index, text, key) in [
        (0, "forceload query", "commands.forceload.added.none"),
        (
            1,
            "forceload add 1024 1024",
            "commands.forceload.added.single",
        ),
        (
            2,
            "forceload add 1024 1024",
            "commands.forceload.added.failure",
        ),
        (
            3,
            "forceload query 1024 1024",
            "commands.forceload.query.success",
        ),
        (
            4,
            "forceload add -16 0 15 15",
            "commands.forceload.added.multiple",
        ),
        (5, "forceload query", "commands.forceload.list.multiple"),
        (
            6,
            "forceload add 0 0 4096 4096",
            "commands.forceload.toobig",
        ),
        (7, "forceload remove all", "commands.forceload.removed.all"),
        (8, "forceload query", "commands.forceload.added.none"),
        (
            9,
            "forceload query 1024 1024",
            "commands.forceload.query.failure",
        ),
        (
            10,
            "forceload add 1024 1024",
            "commands.forceload.added.single",
        ),
        (11, "weather clear", "commands.weather.set.clear"),
        (12, "weather rain 100t", "commands.weather.set.rain"),
        (13, "weather thunder 2s", "commands.weather.set.thunder"),
        (14, "weather clear 1d", "commands.weather.set.clear"),
        (15, "weather rain 0t", "argument.time.tick_count_too_low"),
    ] {
        log.record(
            format!("level.{index}"),
            command(session.bot(id)?, text, key).await?,
        )?;
    }
    log.record(
        "weather.error_context",
        feedback(session.bot(id)?, "command.context.here").await?,
    )?;
    session.restart().await?;
    let id = session.join("ParityBot").await?;
    log.record(
        "forced.persisted",
        command(
            session.bot(id)?,
            "forceload query 1024 1024",
            "commands.forceload.query.success",
        )
        .await?,
    )?;
    log.record(
        "forced.persisted_list",
        command(
            session.bot(id)?,
            "forceload query",
            "commands.forceload.list.single",
        )
        .await?,
    )?;
    Ok(log)
}
command_test!(
    parity_command_force_load_and_weather,
    force_load_and_weather,
    "pending canonical center-chunk FULL acquisition; current ready wait also requires neighbor light/postProcess"
);

async fn player_state_commands(session: &mut Session) -> Result<ObservationLog> {
    let operator = session.join("ParityBot").await?;
    let guest = session.join("CommandGuest").await?;
    let mut log = ObservationLog::default();
    log.record(
        "mode.self",
        command(
            session.bot(operator)?,
            "gamemode creative",
            "commands.gamemode.success.self",
        )
        .await?,
    )?;
    let bot = session.bot(operator)?;
    bot.synchronize().await?;
    let before = bot.snapshot().messages.len();
    bot.command("gamemode creative").await?;
    bot.synchronize().await?;
    ensure!(
        bot.snapshot().messages.len() == before,
        "unchanged game mode emitted feedback"
    );
    log.record("mode.unchanged_feedback", 0)?;
    log.record(
        "mode.other",
        command(
            session.bot(operator)?,
            "gamemode creative CommandGuest",
            "commands.gamemode.success.other",
        )
        .await?,
    )?;
    log.record(
        "mode.target",
        feedback(session.bot(guest)?, "gameMode.changed").await?,
    )?;
    ensure!(
        session.bot(guest)?.snapshot().game_mode
            == Some(ferraloom_parity::protocol::play::GameType::Creative),
        "target game-mode packet missing"
    );
    for (index, text, position, rotation) in [
        (
            0,
            "tp @s 10.0 90.0 -10.0 450 30",
            [10., 90., -10.],
            [90., 30.],
        ),
        (
            1,
            "teleport @s ~1 ~-2 ~3 ~30 ~-20",
            [11., 88., -7.],
            [120., 10.],
        ),
    ] {
        let bot = session.bot(operator)?;
        bot.command(text).await?;
        let packet = bot
            .wait_for("teleport movement", Ticks::DEFAULT, |_, packet| {
                matches!(packet, cb::Packet::PlayerPosition(_))
            })
            .await?;
        let cb::Packet::PlayerPosition(packet) = packet.as_ref() else {
            unreachable!()
        };
        log.record(format!("teleport.{index}.packet"), serde_json::json!({
            "position": [packet.change.position.x,packet.change.position.y,packet.change.position.z],
            "rotation": [packet.change.y_rot,packet.change.x_rot],
            "movement": [packet.change.delta_movement.x,packet.change.delta_movement.y,packet.change.delta_movement.z],
            "relatives": packet.relatives,
        }))?;
        log.record(
            format!("teleport.{index}.feedback"),
            feedback(bot, "commands.teleport.success.location.single").await?,
        )?;
        ensure!(
            bot.snapshot().position == position && bot.snapshot().rotation == rotation,
            "teleport destination mismatch: {:?}",
            bot.snapshot().position
        );
    }
    let before = session.bot(guest)?.snapshot().teleports;
    log.record(
        "teleport.entity.feedback",
        command(
            session.bot(operator)?,
            "tp CommandGuest ParityBot",
            "commands.teleport.success.entity.single",
        )
        .await?,
    )?;
    let guest = session.bot(guest)?;
    guest
        .wait_for("target teleport", Ticks::DEFAULT, |state, packet| {
            state.teleports > before && matches!(packet, cb::Packet::PlayerPosition(_))
        })
        .await?;
    ensure!(
        guest.snapshot().position == [11., 88., -7.] && guest.snapshot().rotation == [120., 10.],
        "entity destination mismatch"
    );
    log.record("teleport.entity.position", guest.snapshot().position)?;
    log.record(
        "teleport.bounds",
        command(
            session.bot(operator)?,
            "tp @s 30000001 90 0",
            "commands.teleport.invalidPosition",
        )
        .await?,
    )?;
    Ok(log)
}
command_test!(parity_command_player_state, player_state_commands);

async fn limited_crafting(session: &mut Session) -> Result<ObservationLog> {
    use ferraloom_data::Item;
    use ferraloom_parity::protocol::{
        codec::VarInt,
        component::{HashedStack, ItemStack},
        play::{self, serverbound as sb},
    };
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    command(bot, "gamemode creative", "commands.gamemode.success.self").await?;
    let mut log = ObservationLog::default();
    for (index, limited, count) in [(0, false, 1), (1, true, 2), (2, false, 3)] {
        if index != 0 {
            log.record(
                format!("rule.{index}"),
                command(
                    bot,
                    &format!("gamerule limited_crafting {limited}"),
                    "commands.gamerule.set",
                )
                .await?,
            )?;
        }
        // Change the crafting grid without taking its result, so the recipe
        // remains unknown to the recipe book throughout this scenario.
        bot.send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
            slot_num: 1,
            item_stack: play::UntrustedItemStack(ItemStack::new(
                Item::OAK_LOG,
                count,
                Default::default(),
            )),
        }))
        .await?;
        bot.synchronize().await?;
        // Native stale-state handling returns an authoritative full menu.
        bot.send(sb::Packet::ContainerClick(sb::ContainerClick {
            container_id: VarInt(0),
            state_id: VarInt(-1),
            slot_num: -1,
            button_num: 0,
            container_input: play::ContainerInput::Pickup,
            changed_slots: play::ChangedSlots(Vec::new()),
            carried_item: HashedStack::default(),
        }))
        .await?;
        let packet = bot.wait_for("crafting result resync", Ticks::DEFAULT, |_,packet| matches!(packet, cb::Packet::ContainerSetContent(packet) if packet.container_id.0==0)).await?;
        let cb::Packet::ContainerSetContent(packet) = packet.as_ref() else {
            unreachable!()
        };
        let result = &packet.items[0];
        if limited {
            ensure!(result.is_empty(), "unknown recipe bypassed LimitedCrafting");
        } else {
            ensure!(
                result
                    .0
                    .as_ref()
                    .is_some_and(|stack| stack.item == Item::OAK_PLANKS && stack.count == 4),
                "unrestricted crafting result missing: {result:?}"
            );
        }
        log.record(format!("result.{index}"), format!("{result:?}"))?;
    }
    Ok(log)
}
command_test!(parity_command_limited_crafting, limited_crafting);

async fn tick_control(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    let mut log = ObservationLog::default();
    for (label, text, frozen, rate, key) in [
        (
            "freeze",
            "tick freeze",
            true,
            20.,
            "commands.tick.status.frozen",
        ),
        (
            "rate",
            "tick rate 10",
            true,
            10.,
            "commands.tick.rate.success",
        ),
        (
            "restore_rate",
            "tick rate 20",
            true,
            20.,
            "commands.tick.rate.success",
        ),
    ] {
        bot.command(text).await?;
        let packet = bot.wait_for("tick state before feedback", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::TickingState(_))
                || matches!(packet,cb::Packet::SystemChat(packet) if json(&packet.content).is_ok_and(|v|has_translation(&v,key)))
        }).await?;
        let cb::Packet::TickingState(packet) = packet.as_ref() else {
            anyhow::bail!("tick feedback preceded its state packet");
        };
        ensure!(
            packet.is_frozen == frozen && packet.tick_rate == rate,
            "wrong live tick state"
        );
        log.record(format!("{label}.state"), format!("{packet:?}"))?;
        log.record(format!("{label}.feedback"), feedback(bot, key).await?)?;
    }
    let baseline =
        command(bot, "time query gametime", "commands.time.query.gametime").await?["with"][0]
            .as_i64()
            .expect("numeric long gametime");
    bot.synchronize().await?;
    let frozen_time =
        command(bot, "time query gametime", "commands.time.query.gametime").await?["with"][0]
            .as_i64()
            .unwrap();
    ensure!(frozen_time == baseline, "frozen game time advanced");
    bot.command("tick step 30t").await?;
    let packet = bot.wait_for("step packet before feedback", Ticks::DEFAULT, |_,packet| {
        matches!(packet,cb::Packet::TickingStep(_))
            || matches!(packet,cb::Packet::SystemChat(packet) if json(&packet.content).is_ok_and(|v|has_translation(&v,"commands.tick.step.success")))
    }).await?;
    let cb::Packet::TickingStep(packet) = packet.as_ref() else {
        anyhow::bail!("step feedback preceded its step packet");
    };
    ensure!(packet.tick_steps.0 == 30, "step count lost");
    log.record("step.packet", packet.tick_steps.0)?;
    log.record(
        "step.feedback",
        feedback(bot, "commands.tick.step.success").await?,
    )?;
    let mut delta = 0;
    for _ in 0..80 {
        let current =
            command(bot, "time query gametime", "commands.time.query.gametime").await?["with"][0]
                .as_i64()
                .unwrap();
        delta = current - baseline;
        ensure!(delta <= 30, "stepping advanced beyond its requested count");
        if delta == 30 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    ensure!(delta == 30, "stepping never reached thirty game ticks");
    log.record("step.game_time_delta", delta)?;
    log.record(
        "step.stop.error",
        command(bot, "tick step stop", "commands.tick.step.stop.fail").await?,
    )?;
    log.record(
        "sprint.stop.error",
        command(bot, "tick sprint stop", "commands.tick.sprint.stop.fail").await?,
    )?;
    bot.command("tick sprint 1000000000t").await?;
    let packet=bot.wait_for("sprint state before feedback",Ticks::DEFAULT,|_,packet| {
        matches!(packet,cb::Packet::TickingState(_)) || matches!(packet,cb::Packet::SystemChat(packet) if json(&packet.content).is_ok_and(|v|has_translation(&v,"commands.tick.status.sprinting")))
    }).await?;
    let cb::Packet::TickingState(packet) = packet.as_ref() else {
        anyhow::bail!("sprint feedback preceded state")
    };
    ensure!(
        !packet.is_frozen && packet.tick_rate == 20.,
        "sprint must unfreeze the game"
    );
    log.record("sprint.state", format!("{packet:?}"))?;
    log.record(
        "sprint.feedback",
        feedback(bot, "commands.tick.status.sprinting").await?,
    )?;
    bot.command("tick sprint stop").await?;
    let packet=bot.wait_for("sprint report before restored state",Ticks::DEFAULT,|_,packet| {
        matches!(packet,cb::Packet::TickingState(_)) || matches!(packet,cb::Packet::SystemChat(packet) if json(&packet.content).is_ok_and(|v|has_translation(&v,"commands.tick.sprint.report")||has_translation(&v,"commands.tick.sprint.stop.success")))
    }).await?;
    let cb::Packet::SystemChat(packet) = packet.as_ref() else {
        anyhow::bail!("restored state preceded sprint report")
    };
    let mut report = json(&packet.content)?;
    // Performance values depend on the host; the native fixed-time Java vector
    // checks their calculation and formatting. Compare every other field here.
    fn normalize_timing(value: &mut Value) -> Result<bool> {
        match value {
            Value::Object(fields) => {
                if fields.get("translate").and_then(Value::as_str)
                    == Some("commands.tick.sprint.report")
                {
                    let args = fields
                        .get_mut("with")
                        .and_then(Value::as_array_mut)
                        .expect("sprint report arguments");
                    ensure!(
                        args.len() == 2 && args[0].as_i64().is_some_and(|v| v >= 0),
                        "sprint rate must be a numeric integer"
                    );
                    let milliseconds = args[1].as_str().expect("formatted milliseconds");
                    ensure!(
                        milliseconds.parse::<f64>().is_ok()
                            && milliseconds
                                .split_once('.')
                                .is_some_and(|(_, fraction)| fraction.len() == 2),
                        "sprint milliseconds must retain two decimals"
                    );
                    args[0] = serde_json::json!(0);
                    args[1] = serde_json::json!("<timing>");
                    return Ok(true);
                }
                for value in fields.values_mut() {
                    if normalize_timing(value)? {
                        return Ok(true);
                    }
                }
            }
            Value::Array(values) => {
                for value in values {
                    if normalize_timing(value)? {
                        return Ok(true);
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }
    ensure!(
        normalize_timing(&mut report)?,
        "missing sprint report before stop feedback"
    );
    log.record("sprint.report", report)?;
    let packet = bot
        .wait_for("restored frozen state", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::TickingState(_))
        })
        .await?;
    let cb::Packet::TickingState(packet) = packet.as_ref() else {
        unreachable!()
    };
    ensure!(
        packet.is_frozen && packet.tick_rate == 20.,
        "sprint stop did not restore frozen state"
    );
    log.record("sprint.restored_state", format!("{packet:?}"))?;
    log.record(
        "sprint.stop.feedback",
        feedback(bot, "commands.tick.sprint.stop.success").await?,
    )?;
    bot.command("tick unfreeze").await?;
    let packet = bot
        .wait_for("unfreeze packet", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::TickingState(_))
        })
        .await?;
    let cb::Packet::TickingState(packet) = packet.as_ref() else {
        unreachable!()
    };
    ensure!(
        !packet.is_frozen && packet.tick_rate == 20.,
        "unfreeze state lost"
    );
    log.record("unfreeze.state", format!("{packet:?}"))?;
    log.record(
        "unfreeze.feedback",
        feedback(bot, "commands.tick.status.running").await?,
    )?;
    Ok(log)
}
command_test!(parity_command_tick_control, tick_control);
