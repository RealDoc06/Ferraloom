//! Exact command feedback and packet variants, using disposable single-region servers.
use ferraloom_parity::{
    Bot, ObservationLog, Result, Session, Ticks, ensure, parity_test,
    protocol::{chat::Component, play::clientbound as cb},
};
use serde_json::Value;

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
    let announced = session
        .bot(newcomer)?
        .sequence()
        .join
        .into_iter()
        .any(|packet| {
            packet == ferraloom_data::packets::play::clientbound::Packet::ChangeDifficulty
        });
    ensure!(announced, "joining player received no difficulty packet");
    log.record("difficulty.newcomer_packet", announced)?;
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
    let mut options = ferraloom_parity::LaunchOptions::default();
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

parity_test!(parity_command_game_rules, game_rules);
parity_test!(parity_command_clocks_and_admin, clocks_and_admin);
parity_test!(parity_command_message_arguments, message_arguments);
parity_test!(parity_command_spawn_and_return, spawn_and_return);

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
parity_test!(parity_command_access_lists, access_commands);
