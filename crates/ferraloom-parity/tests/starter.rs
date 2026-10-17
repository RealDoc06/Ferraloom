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
    bot.command("say parity command").await?;
    let packet=bot.wait_for("say broadcast",Ticks::DEFAULT,|_,p|matches!(p,cb::Packet::PlayerChat(p) if p.body.content.0=="parity command")||matches!(p,cb::Packet::DisguisedChat(p) if p.message.to_json().is_ok_and(|s|s.contains("parity command")))).await?;
    let mut log = ObservationLog::default();
    match packet.as_ref() {
        cb::Packet::PlayerChat(p) => {
            log.record("say.message", &p.body.content.0)?;
            log.record(
                "say.name",
                serde_json::from_str::<serde_json::Value>(&p.chat_type.name.to_json()?)?,
            )?;
            log.record("say.type", format!("{:?}", p.chat_type.chat_type))?;
        }
        cb::Packet::DisguisedChat(p) => {
            log.record("say.message", "parity command")?;
            log.record(
                "say.name",
                serde_json::from_str::<serde_json::Value>(&p.chat_type.name.to_json()?)?,
            )?;
            log.record("say.type", format!("{:?}", p.chat_type.chat_type))?;
        }
        _ => unreachable!(),
    }
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
    let target = [start[0] + 100., start[1], start[2]];
    bot.move_to(target, false).await?;
    let chunk = (target[0].floor() as i32 >> 4, target[2].floor() as i32 >> 4);
    if !bot.snapshot().chunks.contains(&chunk) || bot.snapshot().unloads == 0 {
        bot.wait_for("destination chunk and unload", Ticks(1200), |state, _| {
            state.chunks.contains(&chunk) && state.unloads > 0
        })
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
    log.record("old_chunks_unloaded", state.unloads > 0)?;
    Ok(log)
}
async fn chat_formatting(s: &mut Session) -> Result<ObservationLog> {
    let id = s.join("ParityBot").await?;
    let bot = s.bot(id)?;
    bot.chat("hello parity").await?;
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
parity_test!(parity_commands, commands);
parity_test!(
    parity_chunk_loading_while_walking,
    chunk_loading_while_walking
);
parity_test!(parity_chat_formatting, chat_formatting);
parity_test!(
    parity_save_and_reload_player_position,
    save_and_reload_player_position
);
