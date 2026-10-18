mod command_support;

use ferraloom_parity::{
    ObservationLog, Result, Session, Ticks, ensure, parity_test,
    protocol::{
        codec::Holder,
        play::{self, clientbound as cb},
    },
};

async fn join_and_spawn(s: &mut Session) -> Result<ObservationLog> {
    let id = s.join("ParityBot").await?;
    let bot = s.bot(id)?;
    let state = bot.snapshot();
    ensure!(
        state.teleports > 0 && state.position.into_iter().all(f64::is_finite),
        "missing spawn"
    );
    let seq = bot.sequence();
    let mut log = ObservationLog::default();
    log.record(
        "login",
        seq.login
            .iter()
            .map(|p| format!("{p:?}"))
            .collect::<Vec<_>>(),
    )?;
    log.record(
        "configuration",
        seq.configuration
            .iter()
            .map(|(p, r)| (format!("{p:?}"), r))
            .collect::<Vec<_>>(),
    )?;
    log.record("registries", seq.registries)?;
    log.record(
        "join",
        seq.join
            .iter()
            .map(|p| format!("{p:?}"))
            .collect::<Vec<_>>(),
    )?;
    log.record(
        "spawn.position_finite",
        state.position.into_iter().all(f64::is_finite),
    )?;
    log.record("spawn.teleported", state.teleports > 0)?;
    log.record("game_mode", format!("{:?}", state.game_mode))?;
    Ok(log)
}
async fn commands(s: &mut Session) -> Result<ObservationLog> {
    let id = s.join("ParityBot").await?;
    let bot = s.bot(id)?;
    let mut log = ObservationLog::default();
    bot.command("gamemode spectator").await?;
    bot.wait_for(
        "spectator mode",
        Ticks::DEFAULT,
        |_, p| matches!(p,cb::Packet::GameEvent(p) if p.event==3&&p.param==3.),
    )
    .await?;
    bot.teleport([8.5, 150., 8.5]).await?;
    let state = bot.snapshot();
    ensure!(state.position == [8.5, 150., 8.5], "tp position differs");
    ensure!(
        state.game_mode == Some(play::GameType::Spectator),
        "gamemode did not apply"
    );
    log.record("position", state.position)?;
    log.record("mode", format!("{:?}", state.game_mode))?;
    for (signed, message) in [
        (true, "parity signed command"),
        (false, "parity unsigned command"),
    ] {
        let command = format!("say {message}");
        if signed {
            bot.signed_command(&command).await?;
        } else {
            bot.command(&command).await?;
        }
        let packet = bot
            .wait_for("say broadcast", Ticks::DEFAULT, |_, packet| match packet {
                cb::Packet::PlayerChat(p) => p.body.content.0 == message,
                cb::Packet::DisguisedChat(p) => {
                    p.message.to_json().is_ok_and(|s| s.contains(message))
                }
                _ => false,
            })
            .await?;
        let (packet_kind, name, chat_type) = match packet.as_ref() {
            cb::Packet::PlayerChat(p) => {
                ensure!(
                    signed,
                    "unsigned ChatCommand /say must produce DisguisedChat, got PlayerChat; known server mismatch fixed by e52040c, which requires commands foundation b120a6f"
                );
                log.record("signed.content", &p.body.content.0)?;
                ("PlayerChat", &p.chat_type.name, &p.chat_type.chat_type)
            }
            cb::Packet::DisguisedChat(p) => {
                ensure!(
                    !signed,
                    "ChatCommandSigned /say must produce PlayerChat, got DisguisedChat"
                );
                ensure!(
                    p.message == ferraloom_parity::protocol::chat::Component::literal(message),
                    "unexpected /say component"
                );
                log.record(
                    "unsigned.content",
                    serde_json::from_str::<serde_json::Value>(&p.message.to_json()?)?,
                )?;
                ("DisguisedChat", &p.chat_type.name, &p.chat_type.chat_type)
            }
            _ => unreachable!(),
        };
        let prefix = if signed { "signed" } else { "unsigned" };
        log.record(format!("{prefix}.packet"), packet_kind)?;
        log.record(
            format!("{prefix}.name"),
            serde_json::from_str::<serde_json::Value>(&name.to_json()?)?,
        )?;
        log.record(format!("{prefix}.type"), format!("{chat_type:?}"))?;
    }
    Ok(log)
}
async fn chunk_loading_while_walking(s: &mut Session) -> Result<ObservationLog> {
    let id = s.join("ParityBot").await?;
    let bot = s.bot(id)?;
    bot.command("gamemode spectator").await?;
    bot.wait_for(
        "spectator",
        Ticks::DEFAULT,
        |_, p| matches!(p,cb::Packet::GameEvent(p) if p.event==3&&p.param==3.),
    )
    .await?;
    bot.teleport([0.5, 150., 0.5]).await?;
    let start = bot.snapshot().position;
    let initial_chunk = (start[0].floor() as i32 >> 4, start[2].floor() as i32 >> 4);
    if !bot.snapshot().chunks.contains(&initial_chunk) {
        bot.wait_for("walking start chunk", Ticks(1200), |state, _| {
            state.chunks.contains(&initial_chunk)
        })
        .await?;
    }
    let unloads_before = bot.snapshot().unloads;
    let target = [start[0] + 100., start[1], start[2]];
    bot.move_to(target, false).await?;
    let chunk = (target[0].floor() as i32 >> 4, target[2].floor() as i32 >> 4);
    let state = bot.snapshot();
    if !state.chunks.contains(&chunk)
        || state.chunks.contains(&initial_chunk)
        || state.unloads <= unloads_before
    {
        bot.wait_for(
            "destination chunk and walking start unload",
            Ticks(1200),
            |state, _| {
                state.chunks.contains(&chunk)
                    && !state.chunks.contains(&initial_chunk)
                    && state.unloads > unloads_before
            },
        )
        .await?;
    }
    bot.synchronize().await?;
    let state = bot.snapshot();
    ensure!(
        state.position == target,
        "movement was corrected: {:?}",
        state.position
    );
    let mut log = ObservationLog::default();
    log.record("position", state.position)?;
    log.record("destination_loaded", state.chunks.contains(&chunk))?;
    log.record(
        "walking_start_unloaded",
        !state.chunks.contains(&initial_chunk) && state.unloads > unloads_before,
    )?;
    Ok(log)
}
async fn chat_formatting(s: &mut Session) -> Result<ObservationLog> {
    let sender = s.join("ParityBot").await?;
    let witness = s.join("ChatWitness").await?;
    s.bot(sender)?.chat("hello parity").await?;
    let bot = s.bot(witness)?;
    let packet = bot
        .wait_for(
            "player chat",
            Ticks::DEFAULT,
            |_, p| matches!(p,cb::Packet::PlayerChat(p) if p.body.content.0=="hello parity"),
        )
        .await?;
    let cb::Packet::PlayerChat(p) = packet.as_ref() else {
        unreachable!()
    };
    ensure!(
        p.sender == ferraloom_parity::protocol::Uuid::offline_player("ParityBot"),
        "wrong chat sender"
    );
    ensure!(
        matches!(p.chat_type.chat_type.0, Holder::Reference(_)),
        "chat type must resolve through negotiated registry"
    );
    let mut log = ObservationLog::default();
    log.record("content", &p.body.content.0)?;
    log.record("sender", p.sender.to_string())?;
    log.record(
        "name",
        serde_json::from_str::<serde_json::Value>(&p.chat_type.name.to_json()?)?,
    )?;
    log.record("chat_type", format!("{:?}", p.chat_type.chat_type))?;
    log.record(
        "unsigned_content",
        p.unsigned_content
            .as_ref()
            .map(|c| c.to_json())
            .transpose()?,
    )?;
    log.record("filter", format!("{:?}", p.filter_mask))?;
    Ok(log)
}
async fn save_and_reload_player_position(s: &mut Session) -> Result<ObservationLog> {
    let id = s.join("ParityBot").await?;
    let bot = s.bot(id)?;
    bot.command("gamemode spectator").await?;
    bot.wait_for(
        "spectator",
        Ticks::DEFAULT,
        |_, p| matches!(p,cb::Packet::GameEvent(p) if p.event==3&&p.param==3.),
    )
    .await?;
    let target = [23.5, 150., -12.5];
    bot.teleport(target).await?;
    s.stop().await?;
    let saved = s.saved_world()?.player_position("ParityBot")?;
    ensure!(saved == target, "saved position differs: {saved:?}");
    let mut log = ObservationLog::default();
    log.record("saved_position", saved)?;
    s.restart().await?;
    let id = s.join("ParityBot").await?;
    let reloaded = s.bot(id)?.snapshot().position;
    ensure!(
        reloaded == target,
        "reloaded position differs: {reloaded:?}"
    );
    log.record("reloaded_position", reloaded)?;
    Ok(log)
}
parity_test!(parity_join_and_spawn, join_and_spawn);
// Strict default-noise generation still requires the unported sculk_spread callback.
command_support::command_test!(parity_commands, commands);
parity_test!(
    parity_chunk_loading_while_walking,
    chunk_loading_while_walking
);
parity_test!(parity_chat_formatting, chat_formatting);
parity_test!(
    parity_save_and_reload_player_position,
    save_and_reload_player_position
);
