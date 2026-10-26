//! Real use-on, result-slot take and survival movement into the native criteria producers.
//! Pending: heart_transplanter and read_power_of_chiseled_bookshelf, BE factory not ported.
//! Their native fixtures match, but Strict cannot construct the two canonical receivers yet.
use super::*;
use ferraloom_protocol::{
    codec::VarInt,
    component::{HashedStack, Persistent},
};
use ferraloom_util::{BlockPos, Direction};
use std::time::Duration;

#[derive(Clone, Copy)]
enum Action {
    Fall,
    DriedGhast,
    SnifferSeed,
    Seed,
    Lighten,
    WaxOn,
    WaxOff,
    Pot,
}
const ACTIONS: [(&str, Action); 8] = [
    ("adventure/fall_from_world_height", Action::Fall),
    ("husbandry/place_dried_ghast_in_water", Action::DriedGhast),
    ("husbandry/plant_any_sniffer_seed", Action::SnifferSeed),
    ("husbandry/plant_seed", Action::Seed),
    ("adventure/lighten_up", Action::Lighten),
    ("husbandry/wax_on", Action::WaxOn),
    ("husbandry/wax_off", Action::WaxOff),
    (
        "adventure/craft_decorated_pot_using_only_sherds",
        Action::Pot,
    ),
];
fn cases() -> Result<Vec<Case>> {
    let resources = resources()?;
    let definitions = resources
        .registry("advancement")
        .into_iter()
        .map(|(id, resource)| Ok((id.to_string(), resource.json()?)))
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
    ACTIONS
        .iter()
        .map(|(path, _)| {
            let id = format!("minecraft:{path}");
            case(
                id.clone(),
                definitions
                    .get(&id)
                    .context("required physical advancement")?,
                Vec::new(),
            )
        })
        .collect()
}
async fn set_block(bot: &mut Bot, pos: BlockPos, state: &str) -> Result<()> {
    command(
        bot,
        &format!("setblock {} {} {} {state}", pos.x, pos.y, pos.z),
        "commands.setblock.success",
    )
    .await
}
async fn use_on(bot: &mut Bot, pos: BlockPos) -> Result<()> {
    bot.use_item_on(
        play::InteractionHand::MainHand,
        play::BlockHitResult {
            block_pos: pos,
            direction: Direction::Up,
            offset_x: 0.5,
            offset_y: 1.0,
            offset_z: 0.5,
            inside: false,
            world_border_hit: false,
        },
    )
    .await
}
async fn input_fence(bot: &mut Bot, capture: &mut Capture<'_>) -> Result<()> {
    let context = bot.registry_context()?;
    bot.command("scoreboard players get #recipe_capture recipe_capture")
        .await?;
    bot.wait_for("movement execution fence",Ticks::DEFAULT,|_,packet|{
        capture.observe(packet,context.as_ref());
        matches!(packet,cb::Packet::SystemChat(packet) if packet.content.to_json().is_ok_and(|text| text.contains("commands.scoreboard.players.get.success") && text.contains("#recipe_capture")))
    }).await?;
    Ok(())
}
async fn move_packet(bot: &Bot, z: f64, y: f64, on_ground: bool) -> Result<()> {
    let rotation = bot.snapshot().rotation;
    bot.send(sb::Packet::MovePlayerPosRot(sb::MovePlayerPosRot {
        x: 8.5,
        y,
        z,
        y_rot: rotation[0],
        x_rot: rotation[1],
        flags: play::MovementFlags {
            on_ground,
            horizontal_collision: false,
        },
    }))
    .await?;
    bot.send(sb::Packet::ClientTickEnd(sb::ClientTickEnd {}))
        .await
}
async fn fall(bot: &mut Bot, case: &Case, z: i32, log: &mut ObservationLog) -> Result<()> {
    set_block(bot, BlockPos::new(8, -61, z), "minecraft:powder_snow").await?;
    command(bot, "gamemode survival", "commands.gamemode.success").await?;
    bot.teleport([8.5, 320.0, f64::from(z) + 0.5]).await?;
    let teleports = bot.snapshot().teleports;
    let mut capture = Capture::new(case, bot)?;
    // ServerPlayer#trackStartFallingPosition records the first descending position,
    // so descend half a block first and allow an entity tick at y319.5.
    move_packet(bot, f64::from(z) + 0.5, 319.5, false).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    input_fence(bot, &mut capture).await?;
    let bottom = -60.10000002384186; // PowderSnowBlock.FALLING_COLLISION_SHAPE's exact top.
    for step in 1..=380 {
        let y = 319.5 + (bottom - 319.5) * f64::from(step) / 380.0;
        move_packet(bot, f64::from(z) + 0.5, y, step == 380).await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        if step % 20 == 0 {
            input_fence(bot, &mut capture).await?;
        }
    }
    flush(bot, &mut capture, false).await?;
    ensure!(
        bot.snapshot().teleports == teleports,
        "server corrected the survival fall trajectory"
    );
    capture.record(log)
}
async fn click_wait(bot: &mut Bot, menu: i32, slot: i16, button: i8) -> Result<()> {
    click_packet(bot, menu, slot, button).await?;
    bot.wait_for("authoritative crafting menu",Ticks::DEFAULT,|_,packet| matches!(packet,cb::Packet::ContainerSetContent(packet) if packet.container_id.0==menu)).await?;
    Ok(())
}
async fn click_packet(bot: &Bot, menu: i32, slot: i16, button: i8) -> Result<()> {
    bot.send(sb::Packet::ContainerClick(sb::ContainerClick {
        container_id: VarInt(menu),
        state_id: VarInt(-1),
        slot_num: slot,
        button_num: button,
        container_input: play::ContainerInput::Pickup,
        changed_slots: play::ChangedSlots(Vec::new()),
        carried_item: HashedStack::default(),
    }))
    .await
}
async fn pot(bot: &mut Bot, case: &Case, pos: BlockPos, log: &mut ObservationLog) -> Result<()> {
    set_block(bot, pos, "minecraft:crafting_table").await?;
    set_item(bot, 36, Item::ANGLER_POTTERY_SHERD, 4).await?;
    command(
        bot,
        "scoreboard players get #recipe_capture recipe_capture",
        "commands.scoreboard.players.get.success",
    )
    .await?;
    use_on(bot, pos).await?;
    let packet = bot
        .wait_for("crafting table menu", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::OpenScreen(_))
        })
        .await?;
    let cb::Packet::OpenScreen(open) = packet.as_ref() else {
        unreachable!()
    };
    let menu = open.container_id.0;
    bot.wait_for("initial crafting contents",Ticks::DEFAULT,|_,packet|matches!(packet,cb::Packet::ContainerSetContent(packet) if packet.container_id.0==menu)).await?;
    click_wait(bot, menu, 37, 0).await?;
    for slot in [2, 4, 6, 8] {
        click_wait(bot, menu, slot, 1).await?;
    }
    ensure!(
        bot.snapshot().containers[&menu].slots[0].item() == Item::DECORATED_POT,
        "native four-sherd cross did not produce a decorated pot"
    );
    let mut capture = Capture::new(case, bot)?;
    click_packet(bot, menu, 0, 0).await?;
    flush(bot, &mut capture, true).await?;
    capture.record(log)?;
    let crafted = bot.snapshot().containers[&menu].carried.clone();
    ensure!(
        crafted.item() == Item::DECORATED_POT,
        "taking result did not put pot on cursor"
    );
    log.record("crafted.pot", crafted.to_json()?)
}
async fn block_action(
    bot: &mut Bot,
    case: &Case,
    action: Action,
    pos: BlockPos,
    log: &mut ObservationLog,
) -> Result<()> {
    let below = BlockPos::new(pos.x, pos.y - 1, pos.z);
    let (item, clicked) = match action {
        Action::DriedGhast => {
            set_block(bot, below, "minecraft:stone").await?;
            set_block(bot, pos, "minecraft:water").await?;
            (Item::DRIED_GHAST, below)
        }
        Action::SnifferSeed | Action::Seed => {
            set_block(
                bot,
                BlockPos::new(pos.x, pos.y - 2, pos.z),
                "minecraft:stone",
            )
            .await?;
            set_block(bot, below, "minecraft:farmland[moisture=7]").await?;
            (
                if matches!(action, Action::Seed) {
                    Item::WHEAT_SEEDS
                } else {
                    Item::TORCHFLOWER_SEEDS
                },
                below,
            )
        }
        Action::Lighten | Action::WaxOn | Action::WaxOff => {
            set_block(bot, below, "minecraft:stone").await?;
            let state = match action {
                Action::Lighten => "minecraft:oxidized_copper_bulb[lit=true]",
                Action::WaxOn => "minecraft:copper_block",
                _ => "minecraft:waxed_copper_block",
            };
            set_block(bot, pos, state).await?;
            (
                if matches!(action, Action::WaxOn) {
                    Item::HONEYCOMB
                } else {
                    Item::IRON_AXE
                },
                pos,
            )
        }
        Action::Fall | Action::Pot => unreachable!(),
    };
    set_item(bot, 36, item, 1).await?;
    command(
        bot,
        "scoreboard players get #recipe_capture recipe_capture",
        "commands.scoreboard.players.get.success",
    )
    .await?;
    let mut capture = Capture::new(case, bot)?;
    use_on(bot, clicked).await?;
    flush(bot, &mut capture, true).await?;
    capture.record(log)
}
async fn sweep(session: &mut Session, cases: &[Case]) -> Result<ObservationLog> {
    let mut log = ObservationLog::default();
    // Fall runs first, before freezing or scheduling any other fixture work.
    // Only players keep ticking after freeze; no unrelated fluid/crop work
    // is stepped to force an advancement update.
    for (index, (case, (_, action))) in cases.iter().zip(ACTIONS).enumerate() {
        let player = session.join(&name("Act", index)).await?;
        let bot = session.bot(player)?;
        if index == 0 {
            initialize(bot).await?;
        }
        if index == 1 {
            command(bot, "tick freeze", "commands.tick.status.frozen").await?;
        }
        let pos = BlockPos::new(8, 80, 10 + i32::try_from(index)? * 4);
        if matches!(action, Action::Fall) {
            fall(bot, case, pos.z, &mut log).await?;
        } else {
            bot.teleport([8.5, 80.0, f64::from(pos.z) + 2.5]).await?;
            match action {
                Action::Pot => pot(bot, case, pos, &mut log).await?,
                _ => block_action(bot, case, action, pos, &mut log).await?,
            }
        }
        bot.disconnect().await?;
        eprintln!(
            "physical advancement {}/{} {} complete",
            index + 1,
            cases.len(),
            case.id
        );
    }
    session.stop().await?;
    saved(session, "Act", cases, &mut log)?;
    eprintln!(
        "non-recipe physical: {} complete builtin advancements",
        cases.len()
    );
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires disposable Strict vanilla/Ferraloom servers and native physical fixtures"]
async fn parity_non_recipe_physical_advancements() -> Result<()> {
    let cases = cases()?;
    ferraloom_parity::run(
        options("Act", cases.len()),
        RunMode::from_env()?,
        async |session| sweep(session, &cases).await,
    )
    .await
}
