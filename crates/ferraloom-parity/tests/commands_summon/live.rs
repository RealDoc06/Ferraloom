//! One Strict boot per backend for summon construction, ordered pairing and data writes.
use anyhow::Context;
use ferraloom_data::EntityType;
use ferraloom_parity::{
    Bot, BotId, LaunchOptions, Normalization, ObservationLog, Result, RunMode, Session, Ticks,
    WorldPreset, ensure,
    protocol::{
        Uuid, Wire, Writer,
        codec::VarInt,
        common::Utf8,
        entity_data::EntityDataValue,
        play::{clientbound as cb, serverbound as sb},
    },
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn wire(packet: &impl Wire) -> Result<String> {
    let mut writer = Writer::new();
    packet.encode(&mut writer)?;
    Ok(writer
        .as_slice()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
async fn marker(bot: &mut Bot, id: i32) -> Result<()> {
    bot.send(sb::Packet::CommandSuggestion(sb::CommandSuggestion {
        id: VarInt(id),
        command: Utf8("/summon_parity_barrier_".into()),
    }))
    .await
}
async fn fence(bot: &mut Bot, id: i32) -> Result<()> {
    marker(bot, id).await?;
    bot.wait_for(
        "exclusive command barrier",
        Ticks::DEFAULT,
        |_, packet| matches!(packet,cb::Packet::CommandSuggestions(p) if p.id.0 == id),
    )
    .await?;
    Ok(())
}
fn feedback_key(value: &Value) -> Option<&str> {
    fn component_key(value: &Value) -> Option<&str> {
        value.get("translate").and_then(Value::as_str).or_else(|| {
            value
                .get("extra")?
                .as_array()?
                .iter()
                .find_map(component_key)
        })
    }
    // CommandSourceStack#sendFailure wraps the native error in a red component.
    component_key(value.get("content")?)
}
struct Capture {
    events: Vec<Value>,
    ids: BTreeSet<i32>,
    bundle: bool,
    failure: Option<anyhow::Error>,
    implicit_cow: bool,
}
impl Capture {
    fn packet(&mut self, packet: &cb::Packet) {
        match self.event(packet) {
            Ok(Some(value)) => self.events.push(value),
            Ok(None) => (),
            Err(error) => self.failure = Some(error),
        }
    }
    fn event(&mut self, packet: &cb::Packet) -> Result<Option<Value>> {
        Ok(Some(match packet {
            cb::Packet::BundleDelimiter(_) => {
                self.bundle = !self.bundle;
                json!({"kind":"bundle"})
            }
            cb::Packet::AddEntity(p) => {
                ensure!(self.bundle, "entity pairing must be bundled");
                self.ids.insert(p.id.0);
                let mut payload = p.clone();
                payload.id = VarInt(0);
                payload.uuid = Uuid::NIL;
                json!({"kind":"add_entity","id":p.id.0,"uuid":p.uuid.to_string(),"payload":wire(&payload)?})
            }
            cb::Packet::SetEntityData(p) if self.ids.contains(&p.id.0) => {
                let mut payload = if self.implicit_cow {
                    cow_metadata(p)?
                } else {
                    p.clone()
                };
                payload.id = VarInt(0);
                json!({"kind":"metadata","id":p.id.0,"payload":wire(&payload)?})
            }
            cb::Packet::UpdateAttributes(p) if self.ids.contains(&p.entity_id.0) => {
                let mut payload = p.clone();
                payload.entity_id = VarInt(0);
                json!({"kind":"attributes","id":p.entity_id.0,"payload":wire(&payload)?})
            }
            cb::Packet::SetPassengers(p) if self.ids.contains(&p.vehicle.0) => {
                json!({"kind":"passengers","vehicle":p.vehicle.0,"passengers":p.passengers.iter().map(|id|id.0).collect::<Vec<_>>()})
            }
            cb::Packet::SystemChat(p) => {
                json!({"kind":"feedback","content":serde_json::from_str::<Value>(&p.content.to_json()?)?,"overlay":p.overlay})
            }
            _ => return Ok(None),
        }))
    }
}

/// Only metadata fields shown variable in cow-pairing-proof.json are relaxed.
/// The native follow-range bonus is not client-syncable; attributes stay exact.
pub(super) fn cow_metadata(packet: &cb::SetEntityData) -> Result<cb::SetEntityData> {
    let mut result = packet.clone();
    for entry in &result.packed_items.0 {
        match (&entry.value, entry.id) {
            (EntityDataValue::Byte(flags), 15) => ensure!(
                *flags == 2,
                "implicit Cow flags include more than left-handed: {flags}"
            ),
            (EntityDataValue::CowSoundVariant(sound), 19) => ensure!(
                matches!(sound.0.as_str(), "minecraft:classic" | "minecraft:moody"),
                "unknown native Cow sound variant: {sound:?}"
            ),
            (_, 15 | 19) => anyhow::bail!("wrong Cow metadata serializer: {entry:?}"),
            _ => (),
        }
    }
    // Their default values are omitted by native getNonDefaultValues. Removing
    // only these proven optional entries preserves every other byte and its order.
    result
        .packed_items
        .0
        .retain(|entry| ![15, 19].contains(&entry.id));
    Ok(result)
}
struct SpawnExpectation {
    count: usize,
    generated: bool,
}
impl SpawnExpectation {
    fn fixed(count: usize) -> Self {
        Self {
            count,
            generated: false,
        }
    }
    fn generated(count: usize) -> Self {
        Self {
            count,
            generated: true,
        }
    }
}

/// UUID aliases are opt-in only for the five implicit commands proven in two
/// JVMs. Opaque network IDs reuse the harness's single EntityId identity map.
async fn command(
    session: &mut Session,
    actor: BotId,
    log: &mut ObservationLog,
    index: usize,
    text: &str,
    key: &str,
    expected: SpawnExpectation,
) -> Result<Value> {
    let label = format!("case{index:03}");
    let bot = session.bot(actor)?;
    let barrier = 600_000 + index as i32 * 3;
    fence(bot, barrier).await?;
    let initial = bot
        .snapshot()
        .entities
        .keys()
        .copied()
        .collect::<BTreeSet<_>>();
    bot.command(text).await?;
    marker(bot, barrier + 1).await?;
    let mut captured = Capture {
        events: Vec::new(),
        ids: BTreeSet::new(),
        bundle: false,
        failure: None,
        implicit_cow: expected.generated && text == "summon minecraft:cow 8.5 -60 8.5",
    };
    let mut fenced = false;
    bot.wait_for(text, Ticks::DEFAULT, |_, packet| {
        captured.packet(packet);
        fenced |= matches!(packet,cb::Packet::CommandSuggestions(p) if p.id.0 == barrier + 1);
        captured.failure.is_some()
            || (fenced
                && captured.ids.len() >= expected.count
                && captured
                    .events
                    .iter()
                    .any(|value| value["kind"] == "feedback"))
    })
    .await
    .with_context(|| format!("{text}: captured {:?}", captured.events))?;
    marker(bot, barrier + 2).await?;
    bot.wait_for(
        "post-feedback pairing fence",
        Ticks::DEFAULT,
        |_, packet| {
            captured.packet(packet);
            matches!(packet,cb::Packet::CommandSuggestions(p) if p.id.0 == barrier + 2)
        },
    )
    .await?;
    if let Some(error) = captured.failure {
        return Err(error);
    }
    ensure!(!captured.bundle, "unterminated pairing bundle");
    ensure!(
        captured
            .events
            .iter()
            .any(|value| feedback_key(value) == Some(key)),
        "missing {key} for {text}: {:?}",
        captured.events
    );
    ensure!(
        captured.ids.len() == expected.count,
        "wrong spawn count for {text}: {:?}",
        captured.events
    );
    ensure!(
        captured.ids.is_disjoint(&initial),
        "summon reused a live entity ID"
    );
    let value = json!({"command":text,"events":captured.events});
    for (event, payload) in value["events"].as_array().unwrap().iter().enumerate() {
        let prefix = format!("/{label}/events/{event}");
        for field in ["id", "vehicle"] {
            if payload.get(field).is_some() {
                session.normalize(Normalization::EntityId(format!("{prefix}/{field}")));
            }
        }
        if let Some(passengers) = payload.get("passengers").and_then(Value::as_array) {
            for passenger in 0..passengers.len() {
                session.normalize(Normalization::EntityId(format!(
                    "{prefix}/passengers/{passenger}"
                )));
            }
        }
        if expected.generated {
            if payload.get("uuid").is_some() {
                session.normalize(Normalization::RandomUuid(format!("{prefix}/uuid")));
            }
            if feedback_key(payload) == Some("commands.summon.success") {
                for field in ["hover_event/uuid", "insertion"] {
                    ensure!(
                        payload
                            .pointer(&format!("/content/with/0/{field}"))
                            .is_some(),
                        "missing native display identity"
                    );
                    session.normalize(Normalization::RandomUuid(format!(
                        "{prefix}/content/with/0/{field}"
                    )));
                }
            }
        }
    }
    eprintln!("SUMMON {label}: {}", serde_json::to_string(&value)?);
    log.record(label, &value)?;
    Ok(value)
}

pub async fn run() -> Result<()> {
    let options = LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
            biome: "minecraft:plains".into(),
        },
        properties: [("gamemode".into(), "spectator".into())].into(),
        game_rules: [
            ("minecraft:random_tick_speed".into(), "0".into()),
            ("minecraft:spawn_mobs".into(), "false".into()),
            ("minecraft:advance_weather".into(), "false".into()),
        ]
        .into(),
        ..Default::default()
    };
    ferraloom_parity::run(options, RunMode::Both, async |session: &mut Session| {
        let actor = session.join("ParityBot").await?;
        let mut setup = ObservationLog::default();
        command(session, actor, &mut setup, 0, "tick freeze", "commands.tick.status.frozen",SpawnExpectation::fixed(0)).await?;
        session.bot(actor)?.teleport([8.5,-60.0,8.5]).await?;
        fence(session.bot(actor)?,599_999).await?;
        let mut log = ObservationLog::default();
        let mut index = 1;
        for kind in [EntityType::EXPERIENCE_ORB,EntityType::ITEM,EntityType::TNT,EntityType::FALLING_BLOCK,EntityType::COW] {
            command(session,actor,&mut log,index,&format!("summon {} 8.5 -60 8.5",kind.name()),"commands.summon.success",SpawnExpectation::generated(usize::from(kind!=EntityType::ITEM))).await?;
            index += 1;
        }
        for (offset,(kind,extra)) in [
            ("experience_orb","Value:7s,Count:3"),
            ("item","Item:{id:'minecraft:stone',count:3}"),
            ("tnt","fuse:120s"),
            ("falling_block","BlockState:'minecraft:stone',Time:11"),
            ("cow","NoAI:1b,Age:-20,CustomName:'named cow'"),
        ].into_iter().enumerate() {
            command(session,actor,&mut log,index,&format!("summon minecraft:{kind} 8.5 -60 8.5 {{UUID:[I;0,0,0,{}],{extra}}}",100+offset),"commands.summon.success",SpawnExpectation::fixed(1)).await?;
            index += 1;
        }
        for (text,key,count) in [
            ("summon minecraft:cow 9.5 -60 8.5 {UUID:[I;0,0,0,200],NoAI:1b,Passengers:[{id:'minecraft:item',UUID:[I;0,0,0,201],Item:{id:'minecraft:diamond',count:2}}]}","commands.summon.success",2),
            ("summon minecraft:experience_orb 8.5 -60 8.5 {UUID:[I;0,0,0,100],Value:99s}","commands.summon.failed.uuid",0),
            ("summon minecraft:cow 8.5 -60 8.5 {UUID:[I;0,0,0,300],NoAI:1b,Passengers:[{id:'minecraft:experience_orb',UUID:[I;0,0,0,100]}]}","commands.summon.failed.uuid",0),
            ("summon minecraft:cow 30000000 0 0 {}","commands.summon.invalidPosition",0),
            ("data get entity 00000000-0000-0000-0000-000000000064 Value","commands.data.entity.query",0),
            ("data get entity 00000000-0000-0000-0000-000000000064 Count","commands.data.entity.query",0),
        ] {
            command(session,actor,&mut log,index,text,key,SpawnExpectation::fixed(count)).await?;
            index += 1;
        }
        // The same fixed-UUID live receiver proves /data mutates canonical
        // subclass state, preserves identity, and republishes position changes.
        let item = "00000000-0000-0000-0000-000000000065";
        for (text,key) in [
            (format!("data merge entity {item} {{data:{{n:1,text:'A😀BC',list:[1,2],object:{{a:1}}}}}}"),"commands.data.entity.modified"),
            (format!("data get entity {item} data"),"commands.data.entity.query"),
            (format!("data modify entity {item} data.n set value 7"),"commands.data.entity.modified"),
            (format!("data modify entity {item} data.list append value 3"),"commands.data.entity.modified"),
            (format!("data modify entity {item} data.list prepend value 0"),"commands.data.entity.modified"),
            (format!("data modify entity {item} data.list insert 2 value 8"),"commands.data.entity.modified"),
            (format!("data get entity {item} data.list"),"commands.data.entity.query"),
            (format!("data modify entity {item} data.object merge value {{b:2}}"),"commands.data.entity.modified"),
            (format!("data remove entity {item} data.object.a"),"commands.data.entity.modified"),
            ("data merge storage summon:source {list:[10,11],text:'Native'}".into(),"commands.data.storage.modified"),
            (format!("data modify entity {item} data.list append from storage summon:source list[]"),"commands.data.entity.modified"),
            (format!("data modify entity {item} data.text set string storage summon:source text 1 -1"),"commands.data.entity.modified"),
            (format!("data get entity {item} data"),"commands.data.entity.query"),
            (format!("data modify entity {item} Item.count set value 7"),"commands.data.entity.modified"),
            (format!("data get entity {item} Item.count"),"commands.data.entity.query"),
            (format!("data modify entity {item} UUID set value [I;0,0,0,999]"),"commands.data.entity.modified"),
            (format!("data get entity {item} UUID"),"commands.data.entity.query"),
            (format!("data modify entity {item} Pos[0] set value 9.25d"),"commands.data.entity.modified"),
            (format!("data get entity {item} Pos"),"commands.data.entity.query"),
            ("data merge entity @s {data:{forbidden:1}}".into(),"commands.data.entity.invalid"),
        ] {
            command(session,actor,&mut log,index,&text,key,SpawnExpectation::fixed(0)).await?;
            index += 1;
        }
        Ok(log)
    }).await
}
