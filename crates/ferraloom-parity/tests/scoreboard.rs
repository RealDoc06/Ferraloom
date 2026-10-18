//! Scoreboard/team command feedback, packet changes and join-time sync on bedrock-only flat worlds.
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, WorldPreset, ensure,
    protocol::{chat::Component, play::clientbound as cb},
    run,
};
use serde_json::Value;
fn options() -> LaunchOptions {
    LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
            biome: "minecraft:plains".into(),
        },
        ferraloom_binary: std::env::var_os("FERRALOOM_PARITY_BINARY").map(Into::into),
        ..Default::default()
    }
}
async fn command(bot: &mut Bot, text: &str, key: &str) -> Result<Value> {
    bot.command(text).await?;
    let packet = bot.wait_for(text,Ticks::DEFAULT,|_,packet| matches!(packet,cb::Packet::SystemChat(packet) if packet.content.to_json().is_ok_and(|value|value.contains(key)))).await?;
    let cb::Packet::SystemChat(packet) = packet.as_ref() else {
        unreachable!()
    };
    Ok(serde_json::from_str(&packet.content.to_json()?)?)
}
fn json(component: &Component) -> Result<Value> {
    Ok(serde_json::from_str(&component.to_json()?)?)
}
async fn teams(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    let mut log = ObservationLog::default();
    for (index, text, key) in [
        (0, "team add red", "commands.team.add.success"),
        (1, "team add blue", "commands.team.add.success"),
        (2, "team join red #one", "commands.team.join.success.single"),
        (3, "team join red #one", "commands.team.join.success.single"),
        (
            4,
            "team join blue #one",
            "commands.team.join.success.single",
        ),
        (5, "team list red", "commands.team.list.members.empty"),
        (
            6,
            "team modify blue collisionRule never",
            "commands.team.option.collisionRule.success",
        ),
        (
            7,
            "team modify blue color red",
            "commands.team.option.color.success",
        ),
        (
            8,
            "team modify blue prefix \"[\"",
            "commands.team.option.prefix.success",
        ),
        (
            9,
            "team modify blue suffix \"]\"",
            "commands.team.option.suffix.success",
        ),
        (10, "team list blue", "commands.team.list.members.success"),
        (11, "team empty blue", "commands.team.empty.success"),
        (12, "team empty blue", "commands.team.empty.unchanged"),
        (13, "team remove red", "commands.team.remove.success"),
        (14, "team remove blue", "commands.team.remove.success"),
    ] {
        if index == 10 {
            for (step, text, key) in [
                (
                    "join",
                    "team join blue ParityBot",
                    "commands.team.join.success.single",
                ),
                (
                    "rejoin",
                    "team join blue ParityBot",
                    "commands.team.join.success.single",
                ),
                (
                    "transfer",
                    "team join red ParityBot",
                    "commands.team.join.success.single",
                ),
                (
                    "leave",
                    "team leave ParityBot",
                    "commands.team.leave.success.single",
                ),
                (
                    "dynamic",
                    "team modify red displayName {selector:'@s'}",
                    "commands.team.option.name.success",
                ),
            ] {
                log.record(format!("entity.{step}"), command(bot, text, key).await?)?;
            }
        }
        log.record(index.to_string(), command(bot, text, key).await?)?;
    }
    Ok(log)
}
async fn scores(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    let mut log = ObservationLog::default();
    for (index, text, key) in [
        (
            0,
            "scoreboard objectives add points dummy",
            "commands.scoreboard.objectives.add.success",
        ),
        (
            1,
            "scoreboard players set #one points -5",
            "commands.scoreboard.players.set.success.single",
        ),
        (
            2,
            "scoreboard objectives add divisor dummy",
            "commands.scoreboard.objectives.add.success",
        ),
        (
            3,
            "scoreboard players set #two divisor 2",
            "commands.scoreboard.players.set.success.single",
        ),
        (
            4,
            "scoreboard players operation #one points /= #two divisor",
            "commands.scoreboard.players.operation.success.single",
        ),
        (
            5,
            "scoreboard players get #one points",
            "commands.scoreboard.players.get.success",
        ),
        (
            6,
            "scoreboard objectives setdisplay sidebar points",
            "commands.scoreboard.objectives.display.set",
        ),
        (
            7,
            "scoreboard objectives modify points numberformat blank",
            "commands.scoreboard.objectives.modify.objectiveFormat.set",
        ),
        (
            8,
            "scoreboard objectives add toggle trigger",
            "commands.scoreboard.objectives.add.success",
        ),
        (
            9,
            "scoreboard players enable ParityBot toggle",
            "commands.scoreboard.players.enable.success.single",
        ),
        (10, "trigger toggle add 3", "commands.trigger.add.success"),
        (11, "trigger toggle", "commands.trigger.failed.unprimed"),
    ] {
        log.record(index.to_string(), command(bot, text, key).await?)?;
    }
    let guest = session.join("ScoreGuest").await?;
    // Bot::connect already consumes the initial packet batch; inspect its retained client state.
    let state = session.bot(guest)?.snapshot();
    let Some(ferraloom_parity::protocol::play::ObjectiveOperation::Add {
        display_name,
        number_format,
        ..
    }) = state.objectives.get("points")
    else {
        ferraloom_parity::bail!("join omitted displayed objective")
    };
    ensure!(
        matches!(
            number_format,
            Some(ferraloom_parity::protocol::play::NumberFormat::Blank {})
        ),
        "join omitted objective blank number format"
    );
    ensure!(
        state
            .display_objectives
            .iter()
            .any(|slot| slot.objective_name == "points"),
        "join omitted objective display slot"
    );
    ensure!(
        state.scores.get(&("points".into(), "#one".into())) == Some(&-3),
        "join omitted current score"
    );
    log.record("join.objective", json(display_name)?)?;
    let bot = session.bot(id)?;
    for (step, text, key) in [
        (
            "add",
            "scoreboard objectives add rich dummy {score:{name:'#two',objective:'divisor'}}",
            "commands.scoreboard.objectives.add.success",
        ),
        (
            "object",
            "scoreboard objectives modify rich displayname {atlas:'minecraft:blocks',sprite:'minecraft:stone',fallback:{score:{name:'#two',objective:'divisor'}}}",
            "commands.scoreboard.objectives.modify.displayname",
        ),
        (
            "entity_nbt",
            "scoreboard objectives modify rich displayname {nbt:'Air',entity:'@s',plain:true}",
            "commands.scoreboard.objectives.modify.displayname",
        ),
        (
            "name",
            "scoreboard objectives modify rich displayname {selector:'@s'}",
            "commands.scoreboard.objectives.modify.displayname",
        ),
        (
            "holder",
            "scoreboard players display name #one points {score:{name:'#two',objective:'divisor'}}",
            "commands.scoreboard.players.display.name.set.success.single",
        ),
        (
            "format",
            "scoreboard objectives modify rich numberformat fixed {score:{name:'#two',objective:'divisor'}}",
            "commands.scoreboard.objectives.modify.objectiveFormat.set",
        ),
        (
            "remove",
            "scoreboard objectives remove rich",
            "commands.scoreboard.objectives.remove.success",
        ),
        ("team", "team add existing", "commands.team.add.success"),
    ] {
        log.record(format!("dynamic.{step}"), command(bot, text, key).await?)?;
    }
    let vectors: Vec<Value> = serde_json::from_str(include_str!(
        "../../ferraloom-game/tests/java/scoreboard/dynamic-commands.json"
    ))?;
    ensure!(vectors.len() == 7, "native getter-order cases");
    for (index, vector) in vectors.iter().enumerate() {
        let text = vector["command"].as_str().expect("native command");
        let key = vector["error"]["translate"]
            .as_str()
            .expect("native error key");
        log.record(format!("getter.{index}"), command(bot, text, key).await?)?;
    }
    log.record(
        "order.cleanup",
        command(bot, "team remove existing", "commands.team.remove.success").await?,
    )?;
    log.record(
        "reset",
        command(
            bot,
            "scoreboard players reset #one points",
            "commands.scoreboard.players.reset.specific.single",
        )
        .await?,
    )?;
    log.record(
        "remove",
        command(
            bot,
            "scoreboard objectives remove points",
            "commands.scoreboard.objectives.remove.success",
        )
        .await?,
    )?;
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla jar/JDK; run with --ignored"]
async fn parity_team_commands() -> Result<()> {
    run(options(), RunMode::from_env()?, teams).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla jar/JDK; run with --ignored"]
async fn parity_scoreboard_commands() -> Result<()> {
    run(options(), RunMode::from_env()?, scores).await
}
