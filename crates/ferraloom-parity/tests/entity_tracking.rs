#[expect(
    dead_code,
    reason = "Tracking keeps world time frozen and uses only the non-block level fixtures."
)]
mod level_support;
use ferraloom_data::{EntityType, Item};
use ferraloom_parity::{
    Bot, LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, WorldPreset, ensure,
    protocol::{
        self as p, Uuid, Wire, Writer,
        codec::VarInt,
        component::ItemStack,
        entity_data::EntityDataValue,
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use level_support::freeze;

fn options() -> LaunchOptions {
    LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
            biome: "minecraft:plains".into(),
        },
        properties: [("gamemode".into(), "creative".into())].into(),
        operators: vec!["ParityBot".into(), "TrackerOther".into()],
        ..level_support::options()
    }
}
async fn chunk(bot: &mut Bot, position: (i32, i32)) -> Result<()> {
    if !bot.snapshot().chunks.contains(&position) {
        bot.wait_for(
            "client received tracked chunk",
            Ticks::DEFAULT,
            |state, _| state.chunks.contains(&position),
        )
        .await?;
    }
    Ok(())
}
async fn equip(bot: &mut Bot, item: Item, count: i32) -> Result<()> {
    bot.send(sb::Packet::SetCreativeModeSlot(sb::SetCreativeModeSlot {
        slot_num: 36,
        item_stack: play::UntrustedItemStack(ItemStack::new(item, count, Default::default())),
    }))
    .await?;
    bot.synchronize().await
}
fn wire(packet: &impl Wire) -> Result<String> {
    let mut writer = Writer::new();
    packet.encode(&mut writer)?;
    Ok(writer
        .as_slice()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
async fn dropped_item_pairing_and_motion(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    bot.teleport([8.5, 80.0, 10.5]).await?;
    chunk(bot, (0, 0)).await?;
    equip(bot, Item::STONE, 3).await?;
    bot.look(0.0, 0.0).await?;
    let origin = bot.snapshot().position;
    bot.drop_item(true).await?;
    let mut spawn = None;
    let mut metadata = None;
    let mut order = Vec::new();
    let mut bundled = false;
    bot.wait_for(
        "dropped item pairing bundle",
        Ticks::DEFAULT,
        |_, packet| {
            match packet {
                cb::Packet::BundleDelimiter(_) => {
                    bundled = !bundled;
                    if !bundled && spawn.is_some() && metadata.is_some() {
                        return true;
                    }
                }
                cb::Packet::AddEntity(packet)
                    if packet.entity_type.0 == i32::from(EntityType::ITEM.protocol_id()) =>
                {
                    if bundled {
                        spawn = Some(packet.clone());
                        order.push("AddEntity");
                    }
                }
                cb::Packet::SetEntityData(packet)
                    if spawn
                        .as_ref()
                        .is_some_and(|spawn: &cb::AddEntity| spawn.id == packet.id) =>
                {
                    metadata = Some(packet.clone());
                    order.push("SetEntityData");
                }
                _ => {}
            }
            false
        },
    )
    .await?;
    let spawn = spawn.unwrap();
    let metadata = metadata.unwrap();
    ensure!(
        order == ["AddEntity", "SetEntityData"],
        "wrong item pairing order: {order:?}"
    );
    let stack = metadata
        .packed_items
        .0
        .iter()
        .find_map(|value| match &value.value {
            EntityDataValue::ItemStack(stack) if value.id == 8 => Some(stack),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("item pairing omitted its stack"))?;
    ensure!(
        stack.item() == Item::STONE && stack.0.as_ref().is_some_and(|stack| stack.count == 3),
        "wrong paired item stack"
    );
    let initial = &spawn.movement.0;
    ensure!(
        initial.x.abs() < 0.021
            && (0.279..0.321).contains(&initial.z)
            && (-0.001..0.201).contains(&initial.y),
        "wrong native thrown-item velocity: {initial:?}"
    );
    // Frozen ticks still send tracker updates. One actual simulation tick changes
    // the item's velocity; capture its motion rather than consuming it in feedback.
    bot.command("tick step 1").await?;
    let packet = bot.wait_for("tracked item motion after one simulation tick",Ticks::DEFAULT,|_,packet|matches!(packet,cb::Packet::SetEntityMotion(packet) if packet.id == spawn.id)).await?;
    let cb::Packet::SetEntityMotion(motion) = packet.as_ref() else {
        unreachable!()
    };
    let drag = f64::from(0.98_f32);
    let expected = [
        initial.x * drag,
        (initial.y - 0.04) * drag,
        initial.z * drag,
    ];
    let actual = [
        motion.movement.0.x,
        motion.movement.0.y,
        motion.movement.0.z,
    ];
    // Both endpoints are LpVec3-quantized. This bounds the two codec errors;
    // process-local player RNG is checked by its native drop envelope above.
    ensure!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() <= 4.0 / 32766.0),
        "motion did not preserve the first native gravity/drag step: {actual:?}, expected {expected:?}"
    );
    let mut normalized = metadata;
    normalized.id = VarInt(0);
    let mut log = ObservationLog::default();
    order.push("SetEntityMotion");
    log.record("item.packet_order", order)?;
    log.record(
        "item.spawn_position",
        [
            spawn.x - origin[0],
            spawn.y - origin[1],
            spawn.z - origin[2],
        ],
    )?;
    log.record("item.metadata", wire(&normalized)?)?;
    log.record("item.motion_matches_first_tick", true)?;
    Ok(log)
}

struct Pairing {
    uuid: Uuid,
    network: Option<i32>,
    bundled: bool,
    active: bool,
    complete: bool,
    packets: Vec<cb::Packet>,
}
impl Pairing {
    fn new(name: &str) -> Self {
        Self {
            uuid: Uuid::offline_player(name),
            network: None,
            bundled: false,
            active: false,
            complete: false,
            packets: Vec::new(),
        }
    }
    fn observe(&mut self, packet: &cb::Packet) {
        if let cb::Packet::BundleDelimiter(_) = packet {
            self.bundled = !self.bundled;
            if self.active && !self.bundled {
                self.complete = true;
                self.active = false;
            }
            return;
        }
        if let cb::Packet::AddEntity(add) = packet
            && add.uuid == self.uuid
            && self.bundled
            && !self.complete
        {
            self.network = Some(add.id.0);
            self.active = true;
        }
        if self.active {
            self.packets.push(packet.clone());
        }
    }
    fn record(&self, log: &mut ObservationLog, label: &str, item: Item, count: i32) -> Result<()> {
        ensure!(self.complete, "pairing bundle not complete");
        let mut order = Vec::new();
        let mut equipped = false;
        for packet in &self.packets {
            match packet {
                cb::Packet::AddEntity(packet) => {
                    ensure!(
                        packet.entity_type.0 == i32::from(EntityType::PLAYER.protocol_id()),
                        "paired entity is not a player"
                    );
                    order.push("AddEntity");
                    log.record(format!("{label}.spawn_type"), packet.entity_type.0)?;
                }
                cb::Packet::SetEntityData(packet) => {
                    ensure!(
                        Some(packet.id.0) == self.network,
                        "metadata receiver mismatch"
                    );
                    let mut packet = packet.clone();
                    packet.id = VarInt(0);
                    order.push("SetEntityData");
                    log.record(format!("{label}.metadata"), wire(&packet)?)?;
                }
                cb::Packet::UpdateAttributes(packet) => {
                    ensure!(
                        Some(packet.entity_id.0) == self.network,
                        "attribute receiver mismatch"
                    );
                    let mut packet = packet.clone();
                    packet.entity_id = VarInt(0);
                    // Holder identity hashes vary between JVM processes. Compare
                    // independent attributes by registry id, retaining each value.
                    packet.values.sort_by_key(|attribute| attribute.attribute.0);
                    order.push("UpdateAttributes");
                    log.record(format!("{label}.attributes"), wire(&packet)?)?;
                }
                cb::Packet::SetEquipment(packet) => {
                    ensure!(
                        Some(packet.entity.0) == self.network,
                        "equipment receiver mismatch"
                    );
                    equipped = packet.slots.iter().any(|entry| {
                        entry.slot == p::component::EquipmentSlot::Mainhand
                            && entry.item.item() == item
                            && entry
                                .item
                                .0
                                .as_ref()
                                .is_some_and(|stack| stack.count == count)
                    });
                    let mut packet = packet.clone();
                    packet.entity = VarInt(0);
                    order.push("SetEquipment");
                    log.record(format!("{label}.equipment"), wire(&packet)?)?;
                }
                packet => anyhow::bail!("unexpected player pairing packet: {packet:?}"),
            }
        }
        ensure!(
            equipped,
            "player pairing omitted the actual selected hand stack"
        );
        ensure!(
            order
                == [
                    "AddEntity",
                    "SetEntityData",
                    "UpdateAttributes",
                    "SetEquipment"
                ],
            "wrong player pairing order: {order:?}"
        );
        log.record(format!("{label}.order"), order)
    }
}
async fn reciprocal_pairing_equipment_and_leave(session: &mut Session) -> Result<ObservationLog> {
    let first = session.join("ParityBot").await?;
    {
        let bot = session.bot(first)?;
        freeze(bot).await?;
        bot.teleport([8.5, 80.0, 10.5]).await?;
        chunk(bot, (0, 0)).await?;
        equip(bot, Item::STONE, 3).await?;
    }
    let second = session.join("TrackerOther").await?;
    {
        let bot = session.bot(second)?;
        bot.teleport([80.5, 80.0, 10.5]).await?;
        chunk(bot, (5, 0)).await?;
        equip(bot, Item::DIAMOND, 5).await?;
    }
    session.bot(first)?.synchronize().await?;
    session.bot(second)?.synchronize().await?;
    let mut first_view = Pairing::new("TrackerOther");
    let mut second_view = Pairing::new("ParityBot");
    session
        .bot(first)?
        .command("tp TrackerOther 9.5 80 10.5")
        .await?;
    session
        .bot(first)?
        .wait_for(
            "first player sees equipped second player",
            Ticks::DEFAULT,
            |_, packet| {
                first_view.observe(packet);
                first_view.complete
            },
        )
        .await?;
    let bot = session.bot(second)?;
    // Keep any pairing received alongside the returning chunk. Otherwise send a
    // real positional client update after that chunk is no longer pending.
    if !bot.snapshot().chunks.contains(&(0, 0)) {
        bot.wait_for(
            "second player receives return chunk",
            Ticks::DEFAULT,
            |state, packet| {
                second_view.observe(packet);
                state.chunks.contains(&(0, 0))
            },
        )
        .await?;
    }
    if !second_view.complete {
        let state = bot.snapshot();
        bot.send(sb::Packet::MovePlayerPosRot(sb::MovePlayerPosRot {
            x: 9.5,
            y: 80.0,
            z: 10.5,
            y_rot: state.rotation[0],
            x_rot: state.rotation[1],
            flags: play::MovementFlags {
                on_ground: false,
                horizontal_collision: false,
            },
        }))
        .await?;
        bot.send(sb::Packet::ClientTickEnd(sb::ClientTickEnd {}))
            .await?;
        bot.wait_for(
            "second player sees equipped first player",
            Ticks::DEFAULT,
            |_, packet| {
                second_view.observe(packet);
                second_view.complete
            },
        )
        .await?;
    }
    let mut log = ObservationLog::default();
    first_view.record(&mut log, "first_sees_second", Item::DIAMOND, 5)?;
    second_view.record(&mut log, "second_sees_first", Item::STONE, 3)?;
    let second_network = first_view.network.unwrap();
    let bot = session.bot(first)?;
    bot.command("kick TrackerOther").await?;
    let mut leave = Vec::new();
    bot.wait_for(
        "tracker removal precedes player info removal",
        Ticks::DEFAULT,
        |_, packet| {
            match packet {
                cb::Packet::RemoveEntities(packet)
                    if packet.entity_ids.iter().any(|id| id.0 == second_network) =>
                {
                    leave.push("RemoveEntities")
                }
                cb::Packet::PlayerInfoRemove(packet)
                    if packet
                        .profile_ids
                        .contains(&Uuid::offline_player("TrackerOther")) =>
                {
                    leave.push("PlayerInfoRemove");
                    return true;
                }
                _ => {}
            }
            false
        },
    )
    .await?;
    ensure!(
        leave == ["RemoveEntities", "PlayerInfoRemove"],
        "wrong leave order: {leave:?}"
    );
    ensure!(
        !bot.snapshot().entities.contains_key(&second_network),
        "departing player is still visible"
    );
    log.record("leave.order", leave)?;
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "launches disposable Strict vanilla/Ferraloom servers"]
async fn dropped_item_pairing_metadata_and_motion_match() -> Result<()> {
    ferraloom_parity::run(
        options(),
        RunMode::from_env()?,
        dropped_item_pairing_and_motion,
    )
    .await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "launches disposable Strict vanilla/Ferraloom servers"]
async fn players_pair_equipment_reciprocally_then_remove_on_leave() -> Result<()> {
    ferraloom_parity::run(
        options(),
        RunMode::from_env()?,
        reciprocal_pairing_equipment_and_leave,
    )
    .await
}
