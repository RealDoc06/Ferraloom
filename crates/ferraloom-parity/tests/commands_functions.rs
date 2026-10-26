//! One Strict disposable boot per backend for world-pack functions and live frames.
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, WorldPreset, ensure,
    protocol::{
        codec::VarInt,
        common::Utf8,
        play::{clientbound as cb, serverbound as sb},
    },
};
use serde_json::{Value, json};
use std::path::Path;

fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, text)?;
    Ok(())
}
fn fixture(root: &Path) -> Result<()> {
    let low = root.join("datapacks/a_low");
    let high = root.join("datapacks/z_high");
    for pack in [&low, &high] {
        write(
            &pack.join("pack.mcmeta"),
            r#"{"pack":{"description":"function parity fixture","min_format":121,"max_format":121}}"#,
        )?;
    }
    for (id, body) in [
        ("ordered", "return 11\n"),
        (
            "leaf",
            "data modify storage test:state value set value 2\nreturn 13\n",
        ),
        (
            "outer",
            "data modify storage test:state value set value 1\nfunction test:leaf\nreturn 7\ndata modify storage test:state value set value 99\n",
        ),
        (
            "outer_return",
            "return run function test:leaf\ndata modify storage test:state value set value 99\n",
        ),
        (
            "first",
            "data modify storage test:state value set value 1\nreturn 11\n",
        ),
        (
            "second",
            "data modify storage test:state value set value 2\nreturn fail\n",
        ),
        (
            "third",
            "data modify storage test:state value set value 3\n",
        ),
        ("empty", ""),
        ("bad", "unknown_function_fixture_command\n"),
        ("permission", "op ParityBot\n"),
    ] {
        write(
            &low.join(format!("data/test/function/{id}.mcfunction")),
            body,
        )?;
    }
    write(
        &high.join("data/test/function/ordered.mcfunction"),
        "return 22\n",
    )?;
    write(
        &low.join("data/test/tags/function/group.json"),
        r#"{"values":["test:first"]}"#,
    )?;
    write(
        &high.join("data/test/tags/function/group.json"),
        r#"{"values":["test:second","test:third"]}"#,
    )?;
    write(
        &low.join("data/test/tags/function/empty.json"),
        r#"{"values":[]}"#,
    )?;
    Ok(())
}
fn translation<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    if value.get("translate").and_then(Value::as_str) == Some(key) {
        return Some(value);
    }
    match value {
        Value::Array(values) => values.iter().find_map(|value| translation(value, key)),
        Value::Object(values) => values.values().find_map(|value| translation(value, key)),
        _ => None,
    }
}
fn key(value: &Value) -> Option<&str> {
    value
        .get("translate")
        .and_then(Value::as_str)
        .or_else(|| match value {
            Value::Array(values) => values.iter().find_map(key),
            Value::Object(values) => values.values().find_map(key),
            _ => None,
        })
}
async fn barrier(bot: &mut Bot, id: i32) -> Result<()> {
    bot.send(sb::Packet::CommandSuggestion(sb::CommandSuggestion {
        id: VarInt(id),
        command: Utf8("/function_parity_barrier_".into()),
    }))
    .await?;
    bot.wait_for(
        "function command barrier",
        Ticks::DEFAULT,
        |_, packet| matches!(packet,cb::Packet::CommandSuggestions(value) if value.id.0==id),
    )
    .await?;
    Ok(())
}
async fn command(bot: &mut Bot, serial: &mut i32, text: &str) -> Result<Vec<Value>> {
    *serial += 1;
    barrier(bot, *serial).await?;
    bot.command(text).await?;
    *serial += 1;
    let id = *serial;
    bot.send(sb::Packet::CommandSuggestion(sb::CommandSuggestion {
        id: VarInt(id),
        command: Utf8("/function_parity_barrier_".into()),
    }))
    .await?;
    let mut messages = Vec::new();
    let mut error = None;
    bot.wait_for(text, Ticks::DEFAULT, |_, packet| {
        if let cb::Packet::SystemChat(value) = packet {
            match value.content.to_json().and_then(|text| {
                serde_json::from_str::<Value>(&text).map_err(|error| {
                    ferraloom_parity::protocol::Error::invalid("component JSON", error.to_string())
                })
            }) {
                Ok(content) => messages.push(json!({"content":content,"overlay":value.overlay})),
                Err(cause) => error = Some(cause),
            }
        }
        matches!(packet,cb::Packet::CommandSuggestions(value) if value.id.0==id)
    })
    .await?;
    if let Some(error) = error {
        return Err(error.into());
    }
    eprintln!("FUNCTION {text} -> {}", serde_json::to_string(&messages)?);
    Ok(messages)
}
fn check_keys(messages: &[Value], expected: &[&str]) -> Result<()> {
    let actual = messages
        .iter()
        .map(|message| key(&message["content"]).unwrap_or("<literal>"))
        .collect::<Vec<_>>();
    ensure!(
        actual == expected,
        "function feedback order: expected {expected:?}, observed {actual:?}"
    );
    Ok(())
}
async fn state(bot: &mut Bot, serial: &mut i32, expected: i32) -> Result<Vec<Value>> {
    let messages = command(bot, serial, "data get storage test:state value 1").await?;
    check_keys(&messages, &["commands.data.storage.get"])?;
    let value = translation(&messages[0], "commands.data.storage.get")
        .and_then(|message| message["with"].as_array()?.last())
        .and_then(Value::as_i64);
    ensure!(
        value == Some(i64::from(expected)),
        "function state expected {expected}, observed {messages:?}"
    );
    Ok(messages)
}
async fn scenario(session: &mut Session) -> Result<ObservationLog> {
    let actor = session.join("ParityBot").await?;
    let mut log = ObservationLog::default();
    let mut serial = 600_000;
    let bot = session.bot(actor)?;
    for (text, wanted) in [
        ("tick freeze", "commands.tick.status.frozen"),
        ("gamemode spectator", "commands.gamemode.success.self"),
    ] {
        check_keys(&command(bot, &mut serial, text).await?, &[wanted])?;
    }
    for (label, text, results, expected_state) in [
        ("priority", "function test:ordered", vec![22], 0),
        ("nested", "function test:outer", vec![7], 2),
        ("return_parent", "function test:outer_return", vec![13], 2),
        ("tag", "function #test:group", vec![11, 0], 3),
        ("return_tag", "return run function #test:group", vec![11], 1),
        ("empty_function", "function test:empty", vec![], 0),
        (
            "compound_argument",
            "function test:outer {unused:1}",
            vec![7],
            2,
        ),
    ] {
        let reset = command(bot, &mut serial, "data merge storage test:state {value:0}").await?;
        log.record(format!("{label}.reset"), reset)?;
        let messages = command(bot, &mut serial, text).await?;
        let scheduled = if label == "tag" || label == "return_tag" {
            "commands.function.scheduled.multiple"
        } else {
            "commands.function.scheduled.single"
        };
        let mut keys = vec![scheduled];
        keys.extend(std::iter::repeat_n(
            "commands.function.result",
            results.len(),
        ));
        check_keys(&messages, &keys)?;
        for (message, expected) in messages[1..].iter().zip(results) {
            let value = translation(message, "commands.function.result")
                .and_then(|message| message["with"][1].as_i64());
            ensure!(
                value == Some(expected),
                "wrong function result: {message:?}"
            );
        }
        log.record(label, messages)?;
        log.record(
            format!("{label}.state"),
            state(bot, &mut serial, expected_state).await?,
        )?;
    }
    let args = command(
        bot,
        &mut serial,
        "data merge storage test:args {p:{unused:2},scalar:1,objects:[{},{}]}",
    )
    .await?;
    log.record("arguments.setup", args)?;
    for (label, text, wanted, expected_state) in [
        (
            "storage_arguments",
            "function test:outer with storage test:args p",
            "commands.function.scheduled.single",
            2,
        ),
        (
            "noncompound",
            "function test:outer with storage test:args scalar",
            "commands.function.error.argument_not_compound",
            2,
        ),
        (
            "missing_path",
            "function test:outer with storage test:args missing",
            "arguments.nbtpath.nothing_found",
            2,
        ),
        (
            "multiple_paths",
            "function test:outer with storage test:args objects[]",
            "commands.data.get.multiple",
            2,
        ),
        (
            "unknown",
            "function test:missing",
            "arguments.function.unknown",
            2,
        ),
        (
            "empty_tag",
            "function #test:empty",
            "commands.function.scheduled.no_functions",
            2,
        ),
        (
            "unknown_tag",
            "function #test:missing",
            "commands.function.scheduled.no_functions",
            2,
        ),
        (
            "bad_definition",
            "function test:bad",
            "arguments.function.unknown",
            2,
        ),
        (
            "compile_permission",
            "function test:permission",
            "arguments.function.unknown",
            2,
        ),
    ] {
        let messages = command(bot, &mut serial, text).await?;
        if label == "storage_arguments" {
            check_keys(&messages, &[wanted, "commands.function.result"])?;
        } else {
            check_keys(&messages, &[wanted])?;
        }
        log.record(label, messages)?;
        log.record(
            format!("{label}.state"),
            state(bot, &mut serial, expected_state).await?,
        )?;
    }
    for (limit, expected) in [(1, 0), (3, 1)] {
        log.record(
            format!("quota{limit}.reset"),
            command(bot, &mut serial, "data merge storage test:state {value:0}").await?,
        )?;
        log.record(
            format!("quota{limit}.limit"),
            command(
                bot,
                &mut serial,
                &format!("gamerule max_command_sequence_length {limit}"),
            )
            .await?,
        )?;
        let messages = command(bot, &mut serial, "function test:outer").await?;
        check_keys(&messages, &["commands.function.scheduled.single"])?;
        log.record(format!("quota{limit}.call"), messages)?;
        log.record(
            format!("quota{limit}.state"),
            state(bot, &mut serial, expected).await?,
        )?;
        log.record(
            format!("quota{limit}.restore"),
            command(
                bot,
                &mut serial,
                "gamerule max_command_sequence_length 65536",
            )
            .await?,
        )?;
    }
    session.stop().await?;
    let saved = session.saved_world()?.level()?.data;
    let packs = saved
        .get_compound("DataPacks")
        .ok_or_else(|| anyhow::anyhow!("normalized packs were not saved"))?;
    ensure!(
        packs
            .get_list("Enabled")
            .is_some_and(|values| values.len() == 3),
        "both world packs must remain selected after save"
    );
    ensure!(
        !saved.contains_key("enabled_features"),
        "native default features must be codec-omitted"
    );
    log.record("saved.pack_configuration", packs.to_string())?;
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both Strict disposable servers and world-pack fixtures"]
async fn parity_world_datapacks_plain_functions_frames_and_live_callbacks() -> Result<()> {
    let directory = tempfile::tempdir()?;
    fixture(directory.path())?;
    let options = LaunchOptions {
        fixture: Some(directory.path().to_owned()),
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
            biome: "minecraft:plains".into(),
        },
        ..Default::default()
    };
    ferraloom_parity::run(options, RunMode::Both, scenario).await
}
