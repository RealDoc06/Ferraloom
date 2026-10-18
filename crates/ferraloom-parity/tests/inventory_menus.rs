use ferraloom_data::Item;
use ferraloom_parity::{
    Bot, Container, ObservationLog, Result, Session, Ticks, ensure, parity_test,
    protocol::{
        codec::VarInt,
        component::{DataComponent, DataComponentPatch, HashedStack, ItemStack},
        play::{self, clientbound as cb, serverbound as sb},
    },
};

fn stack(item: Item, count: i32) -> ItemStack {
    ItemStack::new(item, count, DataComponentPatch::default())
}
async fn creative(bot: &mut Bot) -> Result<()> {
    bot.command("gamemode creative").await?;
    bot.wait_for(
        "creative mode",
        Ticks::DEFAULT,
        |_, p| matches!(p,cb::Packet::GameEvent(p) if p.event==3 && p.param==1.),
    )
    .await?;
    bot.synchronize().await
}
async fn set_slot(bot: &mut Bot, slot: i16, stack: ItemStack) -> Result<()> {
    bot.send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
        slot_num: slot,
        item_stack: play::UntrustedItemStack(stack),
    }))
    .await?;
    bot.synchronize().await
}
// A deliberately stale state ID exercises vanilla's execute-then-full-resync
// branch and gives each observation an authoritative inventory snapshot.
async fn full_click(
    bot: &mut Bot,
    slot: i16,
    button: i8,
    input: play::ContainerInput,
) -> Result<Container> {
    let before = bot.snapshot().containers[&0].state_id;
    bot.send(sb::Packet::ContainerClick(sb::ContainerClick {
        container_id: VarInt(0),
        state_id: VarInt(-1),
        slot_num: slot,
        button_num: button,
        container_input: input,
        changed_slots: play::ChangedSlots(Vec::new()),
        carried_item: HashedStack::default(),
    }))
    .await?;
    bot.wait_for("inventory full resync",Ticks::DEFAULT,|_,p|matches!(p,cb::Packet::ContainerSetContent(p) if p.container_id.0==0 && p.state_id.0!=before)).await?;
    bot.synchronize().await?;
    Ok(bot.snapshot().containers[&0].clone())
}
fn record(log: &mut ObservationLog, label: &str, container: &Container) -> Result<()> {
    use ferraloom_parity::protocol::component::Persistent;
    let contents = container
        .slots
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.is_empty())
        .map(|(slot, s)| Ok((slot, s.to_json()?)))
        .collect::<Result<Vec<_>>>()?;
    log.record(format!("{label}.slots"), contents)?;
    let carried = (!container.carried.is_empty())
        .then(|| container.carried.to_json())
        .transpose()?;
    log.record(format!("{label}.carried"), carried)?;
    log.record(format!("{label}.state_id"), container.state_id)
}
async fn clicks(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    creative(bot).await?;
    set_slot(bot, 36, stack(Item::STONE, 8)).await?;
    set_slot(bot, 37, stack(Item::DIRT, 5)).await?;
    set_slot(bot, 45, stack(Item::STONE, 4)).await?;
    let mut log = ObservationLog::default();
    for (label, slot, button, input) in [
        ("initial", -1, 0, play::ContainerInput::Pickup),
        ("half", 36, 1, play::ContainerInput::Pickup),
        ("place_one", 38, 1, play::ContainerInput::Pickup),
        ("offhand_swap", 37, 40, play::ContainerInput::Swap),
        ("outside_drop", -999, 0, play::ContainerInput::Pickup),
        ("shift_move", 37, 0, play::ContainerInput::QuickMove),
        ("pickup", 36, 0, play::ContainerInput::Pickup),
        ("drag_start", -999, 0, play::ContainerInput::QuickCraft),
        ("drag_first", 39, 1, play::ContainerInput::QuickCraft),
        ("drag_second", 40, 1, play::ContainerInput::QuickCraft),
        ("drag_end", -999, 2, play::ContainerInput::QuickCraft),
    ] {
        record(
            &mut log,
            label,
            &full_click(bot, slot, button, input).await?,
        )?;
    }
    record(
        &mut log,
        "before_close",
        &full_click(bot, 39, 0, play::ContainerInput::Pickup).await?,
    )?;
    bot.send(sb::Packet::ContainerClose(sb::ContainerClose {
        container_id: VarInt(99),
    }))
    .await?;
    let returned = bot
        .wait_for("close inventory return", Ticks::DEFAULT, |_, packet| {
            matches!(packet, cb::Packet::SetPlayerInventory(_))
        })
        .await?;
    let cb::Packet::SetPlayerInventory(returned) = returned.as_ref() else {
        unreachable!()
    };
    let menu_slot = match returned.slot.0 {
        0..=8 => returned.slot.0 + 36,
        9..=35 => returned.slot.0,
        40 => 45,
        slot => anyhow::bail!("unexpected close return slot {slot}"),
    };
    // Closing returns the stack immediately, but only the entity tick updates
    // the menu's remote slot. A ping alone can race that state-ID increment.
    bot.wait_for("close inventory broadcast", Ticks::DEFAULT, |_, packet| {
        matches!(packet, cb::Packet::ContainerSetSlot(packet)
            if packet.container_id.0 == 0
                && i32::from(packet.slot) == menu_slot
                && packet.item_stack == returned.contents)
    })
    .await?;
    let closed = full_click(bot, -1, 0, play::ContainerInput::Pickup).await?;
    ensure!(
        closed.carried.is_empty(),
        "close did not return the carried stack"
    );
    record(&mut log, "closed", &closed)?;
    Ok(log)
}
async fn drops(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    creative(bot).await?;
    set_slot(bot, 36, stack(Item::STONE, 3)).await?;
    let mut log = ObservationLog::default();
    bot.drop_item(false).await?;
    let single = full_click(bot, -1, 0, play::ContainerInput::Pickup).await?;
    ensure!(
        single.slots[36].0.as_ref().is_some_and(|s| s.count == 2),
        "single drop removed wrong count"
    );
    record(&mut log, "single", &single)?;
    bot.drop_item(true).await?;
    let all = full_click(bot, -1, 0, play::ContainerInput::Pickup).await?;
    ensure!(all.slots[36].is_empty(), "stack drop retained items");
    record(&mut log, "stack", &all)?;
    Ok(log)
}
async fn creative_validation(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    creative(bot).await?;
    let custom = ItemStack::new(
        Item::STONE,
        7,
        DataComponentPatch {
            added: vec![DataComponent::MaxStackSize(VarInt(7))],
            removed: Vec::new(),
        },
    );
    set_slot(bot, 36, custom).await?;
    // The persistent stack codec accepts the count; the handler rejects the
    // oversized count against the effective max-stack-size component.
    set_slot(
        bot,
        37,
        ItemStack::new(
            Item::STONE,
            8,
            DataComponentPatch {
                added: vec![DataComponent::MaxStackSize(VarInt(7))],
                removed: Vec::new(),
            },
        ),
    )
    .await?;
    set_slot(bot, 46, stack(Item::DIRT, 1)).await?;
    let snapshot = full_click(bot, -1, 0, play::ContainerInput::Pickup).await?;
    ensure!(
        snapshot.slots[36].0.as_ref().is_some_and(|s| s.count == 7)
            && snapshot.slots[37].is_empty(),
        "creative bounds/component validation mismatch"
    );
    let mut log = ObservationLog::default();
    record(&mut log, "creative", &snapshot)?;
    Ok(log)
}
parity_test!(parity_inventory_clicks, clicks);
parity_test!(parity_inventory_drops, drops);
parity_test!(parity_creative_slot_validation, creative_validation);
