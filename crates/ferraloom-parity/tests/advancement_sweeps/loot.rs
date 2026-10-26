//! Real RandomizableContainer#unpackLootTable, then inventory insertion by chest QuickMove.
use super::*;
use ferraloom_protocol::{
    codec::VarInt,
    component::{HashedStack, Persistent},
};
use ferraloom_util::{BlockPos, ChunkPos, Direction};

const FIXTURES: [(&str, &str); 2] = [
    ("adventure/salvage_sherd", "archaeology/desert_well"),
    ("nether/loot_bastion", "chests/bastion_bridge"),
];
fn cases() -> Result<Vec<Case>> {
    let resources = resources()?;
    let definitions = resources
        .registry("advancement")
        .into_iter()
        .map(|(id, resource)| Ok((id.to_string(), resource.json()?)))
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
    FIXTURES
        .iter()
        .map(|(path, _)| {
            let id = format!("minecraft:{path}");
            case(
                id.clone(),
                definitions.get(&id).context("required loot advancement")?,
                Vec::new(),
            )
        })
        .collect()
}
fn contents(bot: &Bot, menu: i32) -> Result<Vec<ItemStack>> {
    let state = bot.snapshot();
    let container = state.containers.get(&menu).context("open chest contents")?;
    ensure!(
        container.slots.len() == 63,
        "27-slot chest menu layout changed"
    );
    Ok(container.slots[..27].to_vec())
}
/// Chest slots contain optional stacks; preserve every empty slot's position as null.
fn stacks(items: &[ItemStack]) -> Result<Vec<Value>> {
    items
        .iter()
        .map(|stack| {
            if stack.is_empty() {
                Ok(Value::Null)
            } else {
                Ok(stack.to_json()?)
            }
        })
        .collect()
}
async fn use_chest(bot: &mut Bot, pos: BlockPos) -> Result<()> {
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
async fn open(bot: &mut Bot, pos: BlockPos, capture: &mut Capture<'_>) -> Result<i32> {
    let context = bot.registry_context()?;
    use_chest(bot, pos).await?;
    let packet = bot
        .wait_for("loot chest menu", Ticks::DEFAULT, |_, packet| {
            capture.observe(packet, context.as_ref());
            matches!(packet, cb::Packet::OpenScreen(_))
        })
        .await?;
    let cb::Packet::OpenScreen(packet) = packet.as_ref() else {
        unreachable!()
    };
    let menu = packet.container_id.0;
    bot.wait_for("loot chest contents", Ticks::DEFAULT, |_, packet| {
        capture.observe(packet, context.as_ref());
        matches!(packet,cb::Packet::ContainerSetContent(packet) if packet.container_id.0==menu)
    })
    .await?;
    Ok(menu)
}
async fn sweep(session: &mut Session, cases: &[Case]) -> Result<ObservationLog> {
    let mut log = ObservationLog::default();
    for (index, (case, (_, table))) in cases.iter().zip(FIXTURES).enumerate() {
        let player = session.join(&name("Loot", index)).await?;
        let bot = session.bot(player)?;
        if index == 0 {
            initialize(bot).await?;
            command(bot, "tick freeze", "commands.tick.status.frozen").await?;
        }
        let pos = BlockPos::new(8, 80, 10 + i32::try_from(index)? * 4);
        bot.teleport([8.5, 80.0, f64::from(pos.z) + 2.5]).await?;
        // Plain placement constructs the canonical receiver. /data merge uses
        // BlockDataAccessor's retained loadWithComponents/setChanged path; the
        // still-unported inline setblock-NBT adapter is never substituted.
        command(
            bot,
            &format!("setblock {} {} {} minecraft:chest", pos.x, pos.y, pos.z),
            "commands.setblock.success",
        )
        .await?;
        command(bot,&format!("data merge block {} {} {} {{LootTable:\"minecraft:{table}\",LootTableSeed:12345L}}",pos.x,pos.y,pos.z),"commands.data.block.modified").await?;
        let mut capture = Capture::new(case, bot)?;
        let menu = open(bot, pos, &mut capture).await?;
        let before = contents(bot, menu)?;
        let slot = before
            .iter()
            .position(|stack| !stack.is_empty())
            .context("seeded native chest produced no loot")?;
        if index == 0 {
            ensure!(
                before[slot].item() == Item::BREWER_POTTERY_SHERD,
                "native desert-well seed12345 should supply the second AND requirement"
            );
        }
        log.record(format!("contents.before.{}", case.id), stacks(&before)?)?;
        // Loot generation alone does not complete salvage_sherd: take the actual
        // generated sherd into the player's inventory through ChestMenu.
        bot.send(sb::Packet::ContainerClick(sb::ContainerClick {
            container_id: VarInt(menu),
            state_id: VarInt(-1),
            slot_num: i16::try_from(slot)?,
            button_num: 0,
            container_input: play::ContainerInput::QuickMove,
            changed_slots: play::ChangedSlots(Vec::new()),
            carried_item: HashedStack::default(),
        }))
        .await?;
        // loot_bastion completes on opening, before QuickMove. Its advancement
        // flush cannot fence the click's menu synchronization; state_id -1
        // requests the authoritative full contents after the actual transfer.
        let context = bot.registry_context()?;
        bot.wait_for(
            "authoritative loot transfer",
            Ticks::DEFAULT,
            |_, packet| {
                capture.observe(packet, context.as_ref());
                matches!(packet, cb::Packet::ContainerSetContent(packet)
                if packet.container_id.0 == menu
                    && packet.items.len() == 63
                    && packet.items[slot].is_empty()
                    && packet.items[27..].iter().any(|stack| stack.item() == before[slot].item()))
            },
        )
        .await?;
        flush(bot, &mut capture, true).await?;
        let after = contents(bot, menu)?;
        ensure!(
            after[slot].is_empty(),
            "QuickMove did not transfer the generated stack"
        );
        ensure!(
            bot.snapshot().containers[&menu].slots[27..]
                .iter()
                .any(|stack| stack.item() == before[slot].item()),
            "generated stack missing from player inventory slots"
        );
        log.record(format!("contents.after.{}", case.id), stacks(&after)?)?;
        bot.send(sb::Packet::ContainerClose(sb::ContainerClose {
            container_id: VarInt(menu),
        }))
        .await?;
        // Reopening exercises the consumed loot-table state rather than assuming
        // that the first menu's inventory happened to look populated.
        let reopened = open(bot, pos, &mut capture).await?;
        ensure!(
            contents(bot, reopened)? == after,
            "reopening regenerated or changed seeded loot"
        );
        log.record(
            format!("reopen.{}", case.id),
            stacks(&contents(bot, reopened)?)?,
        )?;
        capture.record(&mut log)?;
        bot.disconnect().await?;
        eprintln!("container-loot advancement {} complete", case.id);
    }
    session.stop().await?;
    saved(session, "Loot", cases, &mut log)?;
    let world = session.saved_world()?;
    let entities = world.block_entities("minecraft:overworld", ChunkPos::new(0, 0))?;
    for (index, case) in cases.iter().enumerate() {
        let z = 10 + i32::try_from(index)? * 4;
        let chest = entities
            .iter()
            .find(|entity| {
                entity.get_int("x") == Some(8)
                    && entity.get_int("y") == Some(80)
                    && entity.get_int("z") == Some(z)
            })
            .context("saved seeded chest")?;
        ensure!(
            !chest.contains_key("LootTable") && !chest.contains_key("LootTableSeed"),
            "unpacked chest retained deferred loot NBT"
        );
        log.record(format!("saved.loot_consumed.{}", case.id), true)?;
    }
    eprintln!(
        "non-recipe container loot: {} complete builtin advancements",
        cases.len()
    );
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires disposable Strict servers and actual seeded native loot resources"]
async fn parity_non_recipe_container_loot_advancements() -> Result<()> {
    let cases = cases()?;
    ferraloom_parity::run(
        options("Loot", cases.len()),
        RunMode::from_env()?,
        async |session| sweep(session, &cases).await,
    )
    .await
}
