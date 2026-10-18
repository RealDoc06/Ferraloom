mod level_support;
use ferraloom_data::Block;
use ferraloom_parity::{ObservationLog, Result, RunMode, Session, ensure, run};
use ferraloom_util::BlockPos;
use level_support::*;

async fn update_modes(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    let initial_time = game_time(bot).await?;
    let mut log = ObservationLog::default();
    for (i, mode) in ["replace", "destroy", "keep", "strict"]
        .into_iter()
        .enumerate()
    {
        let pos = BlockPos::new(8 + i as i32 * 2, -59, 8);
        if mode != "keep" {
            set(bot, pos, Block::STONE).await?;
        }
        command(
            bot,
            &format!(
                "setblock {} {} {} minecraft:dirt {mode}",
                pos.x, pos.y, pos.z
            ),
            "commands.setblock.success",
        )
        .await?;
        expect_blocks(bot, &[(pos, Block::DIRT.default_state())]).await?;
        log.record(format!("setblock.{mode}"), block(bot, pos)?.id())?;
    }
    for (i, mode) in ["replace", "destroy", "keep", "outline", "hollow", "strict"]
        .into_iter()
        .enumerate()
    {
        let origin = BlockPos::new(8, -59, 8 + i as i32 * 4);
        if mode != "keep" {
            command(
                bot,
                &format!(
                    "fill {} {} {} {} {} {} minecraft:stone strict",
                    origin.x,
                    origin.y,
                    origin.z,
                    origin.x + 2,
                    origin.y + 2,
                    origin.z + 2
                ),
                "commands.fill.success",
            )
            .await?;
        }
        command(
            bot,
            &format!(
                "fill {} {} {} {} {} {} minecraft:dirt {mode}",
                origin.x,
                origin.y,
                origin.z,
                origin.x + 2,
                origin.y + 2,
                origin.z + 2
            ),
            "commands.fill.success",
        )
        .await?;
        let mut expected = Vec::new();
        for dx in 0..3 {
            for dy in 0..3 {
                for dz in 0..3 {
                    let boundary = dx == 0 || dx == 2 || dy == 0 || dy == 2 || dz == 0 || dz == 2;
                    let block = match (mode, boundary) {
                        ("outline", false) => Block::STONE,
                        ("hollow", false) => Block::AIR,
                        _ => Block::DIRT,
                    };
                    expected.push((origin.offset(dx, dy, dz), block.default_state()));
                }
            }
        }
        expect_blocks(bot, &expected).await?;
        log.record(
            format!("fill.{mode}"),
            expected
                .iter()
                .map(|(pos, _)| {
                    Ok((
                        pos.x - origin.x,
                        pos.y - origin.y,
                        pos.z - origin.z,
                        block(bot, *pos)?.id(),
                    ))
                })
                .collect::<Result<Vec<_>>>()?,
        )?;
    }
    ensure!(
        game_time(bot).await? == initial_time,
        "frozen commands advanced game time"
    );
    step(bot, 1).await?;
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla/JDK and integrated flat-world, commands and live phases"]
async fn parity_setblock_and_fill_update_modes() -> Result<()> {
    run(options(), RunMode::from_env()?, update_modes).await
}
