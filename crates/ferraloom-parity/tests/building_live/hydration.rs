use super::{POS, expect, put, redstone_support, state, use_block};
use ferraloom_data::{Block, BlockState, EntityType};
use ferraloom_parity::{Bot, ObservationLog, Result, Session, Ticks};
use ferraloom_util::Direction;

const COLORS: [(Block, Block); 16] = [
    (Block::WHITE_CONCRETE_POWDER, Block::WHITE_CONCRETE),
    (Block::ORANGE_CONCRETE_POWDER, Block::ORANGE_CONCRETE),
    (Block::MAGENTA_CONCRETE_POWDER, Block::MAGENTA_CONCRETE),
    (
        Block::LIGHT_BLUE_CONCRETE_POWDER,
        Block::LIGHT_BLUE_CONCRETE,
    ),
    (Block::YELLOW_CONCRETE_POWDER, Block::YELLOW_CONCRETE),
    (Block::LIME_CONCRETE_POWDER, Block::LIME_CONCRETE),
    (Block::PINK_CONCRETE_POWDER, Block::PINK_CONCRETE),
    (Block::GRAY_CONCRETE_POWDER, Block::GRAY_CONCRETE),
    (
        Block::LIGHT_GRAY_CONCRETE_POWDER,
        Block::LIGHT_GRAY_CONCRETE,
    ),
    (Block::CYAN_CONCRETE_POWDER, Block::CYAN_CONCRETE),
    (Block::PURPLE_CONCRETE_POWDER, Block::PURPLE_CONCRETE),
    (Block::BLUE_CONCRETE_POWDER, Block::BLUE_CONCRETE),
    (Block::BROWN_CONCRETE_POWDER, Block::BROWN_CONCRETE),
    (Block::GREEN_CONCRETE_POWDER, Block::GREEN_CONCRETE),
    (Block::RED_CONCRETE_POWDER, Block::RED_CONCRETE),
    (Block::BLACK_CONCRETE_POWDER, Block::BLACK_CONCRETE),
];

async fn clear(bot: &mut Bot) -> Result<()> {
    put(bot, POS, Block::AIR.default_state()).await?;
    for side in Direction::VALUES {
        put(
            bot,
            POS.relative(side, 1),
            if side == Direction::Down {
                Block::STONE
            } else {
                Block::AIR
            }
            .default_state(),
        )
        .await?;
    }
    Ok(())
}

async fn place(bot: &mut Bot, powder: Block, expected: BlockState, from_below: bool) -> Result<()> {
    // Placement controls remain frozen so source/neighbor fluid states retain
    // their exact masks. Real falling is exercised separately after these cases.
    if from_below {
        bot.teleport([8.5, -59.0, 10.5]).await?;
    } else {
        put(bot, POS.south(1), Block::STONE.default_state()).await?;
        bot.teleport([8.5, -59.0, 5.5]).await?;
    }
    redstone_support::hand(bot, powder.as_item()).await?;
    use_block(
        bot,
        if from_below {
            POS.below(1)
        } else {
            POS.south(1)
        },
        if from_below {
            Direction::Up
        } else {
            Direction::North
        },
    )
    .await?;
    expect(bot, POS, expected).await?;
    redstone_support::fence(bot).await
}

pub(super) async fn record(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    redstone_support::prepare(bot).await?;
    let mut log = ObservationLog::default();
    for (powder, concrete) in COLORS {
        clear(bot).await?;
        place(bot, powder, powder.default_state(), true).await?;
        log.record(
            format!("{}.dry", powder.name()),
            super::level_support::block(bot, POS)?.id(),
        )?;
        for level in [0, 7] {
            clear(bot).await?;
            put(
                bot,
                POS,
                state(
                    Block::WATER,
                    &[("level", if level == 0 { "0" } else { "7" })],
                ),
            )
            .await?;
            place(bot, powder, concrete.default_state(), true).await?;
            log.record(
                format!("{}.replaced_water.{level}", powder.name()),
                super::level_support::block(bot, POS)?.id(),
            )?;
        }
        for side in Direction::VALUES {
            clear(bot).await?;
            put(bot, POS.relative(side, 1), Block::WATER.default_state()).await?;
            let expected = if side == Direction::Down {
                powder
            } else {
                concrete
            }
            .default_state();
            place(bot, powder, expected, side != Direction::Down).await?;
            log.record(
                format!("{}.placed_water.{side:?}", powder.name()),
                super::level_support::block(bot, POS)?.id(),
            )?;

            clear(bot).await?;
            place(bot, powder, powder.default_state(), true).await?;
            put(bot, POS.relative(side, 1), Block::WATER.default_state()).await?;
            expect(bot, POS, expected).await?;
            log.record(
                format!("{}.neighbor_water.{side:?}", powder.name()),
                super::level_support::block(bot, POS)?.id(),
            )?;
        }
        for (kind, neighbor, side, solidifies) in [
            (
                "waterlogged_bottom_above",
                state(
                    Block::OAK_SLAB,
                    &[("type", "bottom"), ("waterlogged", "true")],
                ),
                Direction::Up,
                false,
            ),
            (
                "waterlogged_top_above",
                state(Block::OAK_SLAB, &[("type", "top"), ("waterlogged", "true")]),
                Direction::Up,
                true,
            ),
            (
                "waterlogged_slab_side",
                state(
                    Block::OAK_SLAB,
                    &[("type", "bottom"), ("waterlogged", "true")],
                ),
                Direction::East,
                true,
            ),
            (
                "lava_side",
                Block::LAVA.default_state(),
                Direction::East,
                false,
            ),
        ] {
            clear(bot).await?;
            put(bot, POS.relative(side, 1), neighbor).await?;
            let expected = if solidifies { concrete } else { powder }.default_state();
            place(bot, powder, expected, true).await?;
            log.record(
                format!("{}.{}", powder.name(), kind),
                super::level_support::block(bot, POS)?.id(),
            )?;
        }
    }
    clear(bot).await?;
    for (powder, _) in COLORS {
        clear(bot).await?;
        let source = POS.above(4);
        put(bot, source, powder.default_state()).await?;
        redstone_support::step(bot, 24).await?;
        expect(bot, source, Block::AIR.default_state()).await?;
        expect(bot, POS, powder.default_state()).await?;
        let no_falling = |snapshot: &ferraloom_parity::BotState| {
            snapshot.entities.values().all(|entity| {
                entity.entity_type != i32::from(EntityType::FALLING_BLOCK.protocol_id())
            })
        };
        if !no_falling(&bot.snapshot()) {
            bot.wait_for(
                "settled powder entity removal",
                Ticks::DEFAULT,
                |snapshot, _| no_falling(snapshot),
            )
            .await?;
        }
        log.record(
            format!("{}.dry_fall", powder.name()),
            [
                super::level_support::block(bot, source)?.id(),
                super::level_support::block(bot, POS)?.id(),
                super::level_support::block(bot, POS.below(1))?.id(),
            ],
        )?;
    }
    clear(bot).await?;
    Ok(log)
}
