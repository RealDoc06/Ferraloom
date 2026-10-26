use ferraloom_data::{Block, BlockState};
use ferraloom_parity::{
    Bot, LaunchOptions, Result, Ticks, WorldPreset, ensure, protocol::play::clientbound as cb,
};
use ferraloom_util::BlockPos;
use std::time::Duration;

pub fn options() -> LaunchOptions {
    LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:stone".into(), 4)],
            biome: "minecraft:plains".into(),
        },
        game_rules: [
            ("minecraft:random_tick_speed".into(), "0".into()),
            ("minecraft:advance_weather".into(), "false".into()),
            ("minecraft:spawn_mobs".into(), "false".into()),
        ]
        .into(),
        ferraloom_binary: std::env::var_os("FERRALOOM_PARITY_BINARY").map(Into::into),
        ..Default::default()
    }
}
pub async fn command(bot: &mut Bot, text: &str, expected: &str) -> Result<serde_json::Value> {
    bot.command(text).await?;
    let response=bot.wait_for(text,Ticks::DEFAULT,|_,packet| {
        matches!(packet,cb::Packet::SystemChat(p) if p.content.to_json().is_ok_and(|json| json.contains(expected)||json.contains("commands.generic.failed")||json.contains("commands.generic.unknown")||json.contains("commands.generic.permission")||json.contains("ferraloom.command.pending")))
    }).await?;
    let cb::Packet::SystemChat(response) = response.as_ref() else {
        unreachable!()
    };
    let json = response.content.to_json()?;
    ensure!(json.contains(expected), "{text}: {json}");
    Ok(serde_json::from_str(&json)?)
}
pub async fn freeze(bot: &mut Bot) -> Result<()> {
    command(bot, "tick freeze", "commands.tick.status.frozen").await?;
    Ok(())
}
pub async fn game_time(bot: &mut Bot) -> Result<i64> {
    let response = command(bot, "time query gametime", "commands.time.query.gametime").await?;
    fn time(value: &serde_json::Value) -> Option<i64> {
        if value.get("translate")?.as_str()? == "commands.time.query.gametime" {
            let arg = value.get("with")?.as_array()?.first()?;
            arg.as_i64()
                .or_else(|| arg.as_str()?.parse().ok())
                .or_else(|| arg.get("text")?.as_str()?.parse().ok())
        } else {
            None
        }
    }
    time(&response).ok_or_else(|| anyhow::anyhow!("missing numeric gametime response"))
}
pub async fn step(bot: &mut Bot, ticks: i64) -> Result<()> {
    let initial = game_time(bot).await?;
    command(
        bot,
        &format!("tick step {ticks}"),
        "commands.tick.step.success",
    )
    .await?;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let now = game_time(bot).await?;
            ensure!(
                now <= initial + ticks,
                "step overshot: {initial} + {ticks} -> {now}"
            );
            if now == initial + ticks {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    bot.synchronize().await?;
    Ok(())
}
pub fn block(bot: &Bot, pos: BlockPos) -> Result<BlockState> {
    bot.snapshot()
        .block(pos)
        .ok_or_else(|| anyhow::anyhow!("unloaded block {pos:?}"))
}
pub async fn expect_blocks(bot: &mut Bot, expected: &[(BlockPos, BlockState)]) -> Result<()> {
    if !expected
        .iter()
        .all(|(pos, state)| bot.snapshot().block(*pos) == Some(*state))
    {
        bot.wait_for(
            "block changes at broadcast phase",
            Ticks::DEFAULT,
            |state, _| {
                expected
                    .iter()
                    .all(|(pos, expected)| state.block(*pos) == Some(*expected))
            },
        )
        .await?;
    }
    for (pos, expected) in expected {
        ensure!(
            block(bot, *pos)? == *expected,
            "unexpected block at {pos:?}"
        );
    }
    Ok(())
}
pub async fn set(bot: &mut Bot, pos: BlockPos, block: Block) -> Result<()> {
    command(
        bot,
        &format!("setblock {} {} {} {}", pos.x, pos.y, pos.z, block.name()),
        "commands.setblock.success",
    )
    .await?;
    Ok(())
}
