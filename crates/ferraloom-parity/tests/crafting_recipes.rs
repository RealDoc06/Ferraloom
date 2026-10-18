use ferraloom_data::Item;
use ferraloom_parity::{
    Bot, Container, ObservationLog, Result, Session, Ticks, ensure, parity_test,
    protocol::{
        self,
        codec::VarInt,
        component::{HashedStack, ItemStack, Persistent},
        play::{self, clientbound as cb, serverbound as sb},
    },
};
fn stack(item: Item, count: i32) -> ItemStack {
    ItemStack::new(item, count, Default::default())
}
async fn creative(bot: &mut Bot) -> Result<()> {
    bot.command("gamemode creative").await?;
    bot.wait_for(
        "creative",
        Ticks::DEFAULT,
        |_, p| matches!(p,cb::Packet::GameEvent(p) if p.event==3&&p.param==1.),
    )
    .await?;
    bot.synchronize().await
}
async fn set_item(bot: &mut Bot, slot: i16, item: Item, count: i32) -> Result<()> {
    bot.send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
        slot_num: slot,
        item_stack: play::UntrustedItemStack(stack(item, count)),
    }))
    .await?;
    bot.synchronize().await
}
async fn click(
    bot: &mut Bot,
    id: i32,
    slot: i16,
    button: i8,
    input: play::ContainerInput,
) -> Result<Container> {
    bot.send(sb::Packet::ContainerClick(sb::ContainerClick {
        container_id: VarInt(id),
        state_id: VarInt(-1),
        slot_num: slot,
        button_num: button,
        container_input: input,
        changed_slots: play::ChangedSlots(Vec::new()),
        carried_item: HashedStack::default(),
    }))
    .await?;
    bot.wait_for(
        "authoritative crafting resync",
        Ticks::DEFAULT,
        |_, p| matches!(p,cb::Packet::ContainerSetContent(p) if p.container_id.0==id),
    )
    .await?;
    bot.synchronize().await?;
    Ok(bot.snapshot().containers[&id].clone())
}
fn record(log: &mut ObservationLog, label: &str, menu: &Container) -> Result<()> {
    let slots = menu
        .slots
        .iter()
        .enumerate()
        .filter(|(_, stack)| !stack.is_empty())
        .map(|(slot, stack)| Ok((slot, stack.to_json()?)))
        .collect::<Result<Vec<_>>>()?;
    log.record(format!("{label}.slots"), slots)?;
    log.record(
        format!("{label}.carried"),
        if menu.carried.is_empty() {
            None
        } else {
            Some(menu.carried.to_json()?)
        },
    )?;
    log.record(format!("{label}.state_id"), menu.state_id)
}
async fn player_grid(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    creative(bot).await?;
    set_item(bot, 36, Item::OAK_LOG, 1).await?;
    let mut log = ObservationLog::default();
    click(bot, 0, 36, 0, play::ContainerInput::Pickup).await?;
    let placed = click(bot, 0, 4, 0, play::ContainerInput::Pickup).await?;
    ensure!(
        placed.slots[0] == stack(Item::OAK_PLANKS, 4),
        "offset 2x2 log must make four planks"
    );
    record(&mut log, "placed", &placed)?;
    let taken = click(bot, 0, 0, 0, play::ContainerInput::Pickup).await?;
    ensure!(
        taken.carried == stack(Item::OAK_PLANKS, 4)
            && taken.slots[4].is_empty()
            && taken.slots[0].is_empty(),
        "taking result must consume one log and return four planks"
    );
    record(&mut log, "taken", &taken)?;
    Ok(log)
}
async fn recipe_book(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    creative(bot).await?;
    set_item(bot, 36, Item::OAK_PLANKS, 8).await?;
    // Inventory advancement criteria may already have unlocked this recipe.
    bot.command("recipe take @s minecraft:crafting_table")
        .await?;
    bot.synchronize().await?;
    bot.command("recipe give @s minecraft:crafting_table")
        .await?;
    let packet=bot.wait_for("crafting table book unlock",Ticks::DEFAULT,|_,packet|matches!(packet,cb::Packet::RecipeBookAdd(packet) if packet.entries.iter().any(|entry|matches!(&entry.contents.display,play::RecipeDisplay::CraftingShaped{result,..} if matches!(result.as_ref(),play::SlotDisplay::ItemStack{stack} if stack.item==Item::CRAFTING_TABLE))))).await?;
    let cb::Packet::RecipeBookAdd(packet) = packet.as_ref() else {
        unreachable!()
    };
    let display=packet.entries.iter().find(|entry|matches!(&entry.contents.display,play::RecipeDisplay::CraftingShaped{result,..} if matches!(result.as_ref(),play::SlotDisplay::ItemStack{stack} if stack.item==Item::CRAFTING_TABLE))).unwrap().contents.id;
    bot.send(sb::Packet::PlaceRecipe(sb::PlaceRecipe {
        container_id: VarInt(0),
        recipe: display,
        use_max_items: true,
    }))
    .await?;
    bot.synchronize().await?;
    let placed = click(bot, 0, -1, 0, play::ContainerInput::Pickup).await?;
    ensure!(
        placed.slots[0] == stack(Item::CRAFTING_TABLE, 1)
            && (1..=4).all(|slot| placed.slots[slot] == stack(Item::OAK_PLANKS, 2)),
        "book max placement must split eight planks across 2x2"
    );
    let mut log = ObservationLog::default();
    record(&mut log, "placed", &placed)?;
    let taken = click(bot, 0, 0, 0, play::ContainerInput::QuickMove).await?;
    ensure!(
        (1..=4).all(|slot| taken.slots[slot].is_empty())
            && taken
                .slots
                .iter()
                .filter_map(|s| s.0.as_ref())
                .filter(|s| s.item == Item::CRAFTING_TABLE)
                .map(|s| s.count)
                .sum::<i32>()
                == 2,
        "shift result must craft exactly two tables"
    );
    record(&mut log, "taken", &taken)?;
    Ok(log)
}
async fn crafting_table(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    creative(bot).await?;
    set_item(bot, 36, Item::OAK_PLANKS, 8).await?;
    bot.teleport([8.5, 150., 8.5]).await?;
    bot.command("setblock 8 149 9 minecraft:crafting_table")
        .await?;
    bot.synchronize().await?;
    bot.use_item_on(
        play::InteractionHand::MainHand,
        play::BlockHitResult {
            block_pos: protocol::BlockPos::new(8, 149, 9),
            direction: ferraloom_util::Direction::Up,
            offset_x: 0.5,
            offset_y: 1.,
            offset_z: 0.5,
            inside: false,
            world_border_hit: false,
        },
    )
    .await?;
    let packet = bot
        .wait_for("crafting table menu", Ticks::DEFAULT, |_, p| {
            matches!(p, cb::Packet::OpenScreen(_))
        })
        .await?;
    let cb::Packet::OpenScreen(open) = packet.as_ref() else {
        unreachable!()
    };
    let menu_id = open.container_id.0;
    bot.wait_for(
        "crafting table contents",
        Ticks::DEFAULT,
        |_, p| matches!(p,cb::Packet::ContainerSetContent(p) if p.container_id.0==menu_id),
    )
    .await?;
    click(bot, menu_id, 37, 0, play::ContainerInput::Pickup).await?;
    let mut placed = None;
    for slot in [1, 2, 3, 4, 6, 7, 8, 9] {
        placed = Some(click(bot, menu_id, slot, 1, play::ContainerInput::Pickup).await?);
    }
    let placed = placed.unwrap();
    ensure!(
        placed.slots[0] == stack(Item::CHEST, 1),
        "3x3 ring of planks must make chest"
    );
    let mut log = ObservationLog::default();
    record(&mut log, "placed", &placed)?;
    let taken = click(bot, menu_id, 0, 0, play::ContainerInput::Pickup).await?;
    ensure!(
        taken.carried == stack(Item::CHEST, 1) && (1..=9).all(|slot| taken.slots[slot].is_empty()),
        "chest result must consume all eight planks"
    );
    record(&mut log, "taken", &taken)?;
    Ok(log)
}
parity_test!(parity_player_crafting_grid, player_grid);
parity_test!(parity_recipe_book_crafting, recipe_book);
parity_test!(parity_crafting_table, crafting_table);
