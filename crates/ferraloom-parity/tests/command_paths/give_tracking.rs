//! GiveCommand's fake-item and leftover paths through the canonical entity tracker.
use super::{Driver, Step, event};
use ferraloom_data::{EntityType, Item};
use ferraloom_parity::{
    BotId, Result, Ticks, ensure,
    protocol::{
        Wire, Writer,
        codec::VarInt,
        component::ItemStack,
        entity_data::EntityDataValue,
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

async fn inventory(driver: &mut Driver<'_>, actor: BotId, full: bool) -> Result<()> {
    for slot in 9..45 {
        driver
            .session
            .bot(actor)?
            .send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
                slot_num: slot,
                item_stack: play::UntrustedItemStack(if full {
                    ItemStack::new(Item::COBBLESTONE, 64, Default::default())
                } else {
                    ItemStack::default()
                }),
            }))
            .await?;
    }
    driver.exchange(actor, None).await?;
    Ok(())
}
async fn pair(
    driver: &mut Driver<'_>,
    actor: BotId,
    item: Item,
    count: i32,
) -> Result<(i32, Value)> {
    driver.exchange(actor, None).await?;
    let bot = driver.session.bot(actor)?;
    let own = bot.snapshot().entity_id;
    let origin = bot.snapshot().position;
    bot.command(&format!("give @s {} {count}", item.name()))
        .await?;
    let mut spawn = None;
    let mut metadata = None;
    let mut bundled = false;
    let mut complete = false;
    let mut events = Vec::new();
    let mut error = None;
    bot.wait_for(
        "give item pairing and feedback",
        Ticks::DEFAULT,
        |_, packet| {
            match packet {
                cb::Packet::BundleDelimiter(_) => {
                    bundled = !bundled;
                    if !bundled && metadata.is_some() {
                        complete = true;
                    }
                }
                cb::Packet::AddEntity(p)
                    if p.entity_type.0 == i32::from(EntityType::ITEM.protocol_id())
                        && spawn.is_none() =>
                {
                    if !bundled {
                        error = Some(anyhow::anyhow!("item pairing is not bundled"));
                    }
                    spawn = Some(p.clone());
                    events.push(json!({"kind":"AddEntity"}));
                }
                cb::Packet::SetEntityData(p)
                    if spawn.as_ref().is_some_and(|s: &cb::AddEntity| s.id == p.id) =>
                {
                    metadata = Some(p.clone());
                    events.push(json!({"kind":"SetEntityData"}));
                }
                _ => match event(packet, own, &BTreeMap::new(), true) {
                    Ok(Some(value)) => events.push(value),
                    Ok(None) => (),
                    Err(e) => error = Some(e),
                },
            }
            complete
                && events
                    .iter()
                    .any(|e| super::translation(e, "commands.give.success.single").is_some())
        },
    )
    .await?;
    if let Some(error) = error {
        return Err(error);
    }
    let spawn = spawn.ok_or_else(|| anyhow::anyhow!("missing item spawn"))?;
    let mut metadata = metadata.ok_or_else(|| anyhow::anyhow!("missing item metadata"))?;
    let stack = metadata
        .packed_items
        .0
        .iter()
        .find_map(|entry| match &entry.value {
            EntityDataValue::ItemStack(stack) if entry.id == 8 => Some(stack),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("missing paired item stack"))?;
    // GiveCommand's fake pickup entity always contains a single prototype item.
    ensure!(
        stack.item() == item && stack.0.as_ref().is_some_and(|s| s.count == count),
        "wrong give item metadata: {stack:?}"
    );
    ensure!(
        spawn.x == origin[0]
            && spawn.z == origin[2]
            && (spawn.y - origin[1] - (f64::from(1.62_f32) - 0.30000001192092896)).abs() < 1e-9,
        "wrong give drop position: {spawn:?}"
    );
    let movement = &spawn.movement.0;
    // Process-local player RNG is checked exactly by the native constructor
    // vectors. Live tests retain the native yaw=0, pitch=0 launch envelope.
    ensure!(
        movement.x.abs() < 0.021
            && (0.279..0.321).contains(&movement.z)
            && (-0.001..0.201).contains(&movement.y),
        "wrong give launch velocity: {movement:?}"
    );
    metadata.id = VarInt(0);
    let mut writer = Writer::new();
    metadata.encode(&mut writer)?;
    Ok((
        spawn.id.0,
        json!({"events":events,"position":[spawn.x,spawn.y,spawn.z],"metadata":writer.as_slice().iter().map(|b|format!("{b:02x}")).collect::<String>()}),
    ))
}

pub(super) async fn run(driver: &mut Driver<'_>, actor: BotId) -> Result<Value> {
    driver
        .step(&Step::feedback(
            "teleport PathGuest 32.5 -63 0.5",
            "commands.teleport.success.location.single",
        ))
        .await?;
    driver
        .step(&Step::feedback(
            "teleport @s 4.5 -63 4.5 0 0",
            "commands.teleport.success.location.single",
        ))
        .await?;
    driver
        .step(&Step::feedback(
            "gamemode creative",
            "commands.gamemode.success.self",
        ))
        .await?;
    inventory(driver, actor, false).await?;
    // Successful insertion registers a one-item fake drop, never a pickup.
    let (fake, fake_pair) = pair(driver, actor, Item::GOLD_INGOT, 1).await?;
    let bot = driver.session.bot(actor)?;
    bot.command("tick step 1").await?;
    let mut fake_taken = false;
    bot.wait_for("fake give item expires after one simulation tick", Ticks::DEFAULT, |_, packet| {
        if matches!(packet,cb::Packet::TakeItemEntity(p) if p.item_id.0 == fake) { fake_taken = true; }
        matches!(packet,cb::Packet::RemoveEntities(p) if p.entity_ids.iter().any(|id| id.0 == fake))
    }).await?;
    ensure!(!fake_taken, "fake give entity generated a pickup packet");
    driver.exchange(actor, None).await?;

    inventory(driver, actor, true).await?;
    driver
        .step(&Step::feedback(
            "gamemode survival",
            "commands.gamemode.success.self",
        ))
        .await?;
    let (leftover, leftover_pair) = pair(driver, actor, Item::EMERALD, 3).await?;
    let bot = driver.session.bot(actor)?;
    let player = bot
        .snapshot()
        .entity_id
        .ok_or_else(|| anyhow::anyhow!("missing collector id"))?;
    // Players still tick while the game is frozen. Switching to Creative here
    // would consume the item immediately, even with a full inventory. The real
    // swap-hands packet frees a slot in Survival; start observing before pickup.
    bot.swap_hands().await?;
    bot.command("tick step 1").await?;
    let mut pickup = Vec::new();
    let mut invalid = false;
    bot.wait_for(
        "leftover give item pickup and removal",
        Ticks::DEFAULT,
        |_, packet| {
            match packet {
                cb::Packet::TakeItemEntity(p) if p.item_id.0 == leftover => {
                    invalid |= p.player_id.0 != player || p.amount.0 != 3;
                    pickup.push("TakeItemEntity");
                }
                cb::Packet::RemoveEntities(p) if p.entity_ids.iter().any(|id| id.0 == leftover) => {
                    pickup.push("RemoveEntities");
                    return true;
                }
                _ => (),
            }
            false
        },
    )
    .await?;
    ensure!(
        !invalid && pickup == ["TakeItemEntity", "RemoveEntities"],
        "wrong leftover pickup packets: {pickup:?}"
    );
    let has_stack = |state: &ferraloom_parity::BotState| {
        state.inventory.get(&0).is_some_and(|s| {
            s.item() == Item::EMERALD && s.0.as_ref().is_some_and(|s| s.count == 3)
        })
    };
    if !has_stack(&bot.snapshot()) {
        bot.wait_for(
            "picked up give remainder in inventory",
            Ticks::DEFAULT,
            |state, _| has_stack(state),
        )
        .await?;
    }
    Ok(
        json!({"fake_pair":fake_pair,"fake_removal":["RemoveEntities"],"leftover_pair":leftover_pair,"pickup":pickup,"inventory":{"item":"minecraft:emerald","count":3}}),
    )
}
