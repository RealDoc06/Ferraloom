//! Gated fresh-world region comparison. Every universe is disposable; no live
//! server or user world is read or changed. Strict is required on Ferraloom.
use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_parity::{
    LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, ensure, run,
};
use ferraloom_server::{registries::RegistrySet, world::RegistryAdapter};
use ferraloom_util::ChunkPos;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn canonical(tag: &Tag) -> Value {
    match tag {
        Tag::End => Value::Null,
        Tag::Compound(tags) => Value::Object(
            tags.iter()
                .map(|(key, value)| (key.to_string_lossy(), canonical(value)))
                .collect(),
        ),
        Tag::List(list) => Value::Array(list.0.iter().map(canonical).collect()),
        Tag::String(text) => json!(["string", text.as_utf16()]),
        Tag::Byte(value) => json!(["byte", value]),
        Tag::Short(value) => json!(["short", value]),
        Tag::Int(value) => json!(["int", value]),
        Tag::Long(value) => json!(["long", value]),
        Tag::Float(value) => json!(["float", value.to_bits()]),
        Tag::Double(value) => json!(["double", value.to_bits()]),
        Tag::ByteArray(value) => json!(["bytes", value]),
        Tag::IntArray(value) => json!(["ints", value]),
        Tag::LongArray(value) => json!(["longs", value]),
    }
}
fn block_entities(tags: Vec<CompoundTag>) -> BTreeMap<[i32; 3], Value> {
    tags.into_iter()
        .map(|tag| {
            (
                [
                    tag.get_int_or("x", 0),
                    tag.get_int_or("y", 0),
                    tag.get_int_or("z", 0),
                ],
                canonical(&Tag::Compound(tag)),
            )
        })
        .collect()
}
async fn walk_and_compare_saved_chunks(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    bot.command("tick freeze").await?;
    bot.command("gamemode spectator").await?;
    bot.synchronize().await?;
    for (dimension, origin, y) in [
        ("minecraft:overworld", 8192.5, 300.),
        ("minecraft:the_nether", 16384.5, 160.),
        ("minecraft:the_end", 32768.5, 200.),
    ] {
        if dimension == "minecraft:overworld" {
            bot.teleport([origin, y, origin]).await?;
        } else {
            let before = bot.snapshot().teleports;
            bot.command(&format!(
                "execute in {dimension} run tp @s {origin} {y} {origin}"
            ))
            .await?;
            bot.wait_for("dimension transfer", Ticks::DEFAULT, |state, packet| {
                state.teleports > before
                    && matches!(
                        packet,
                        ferraloom_parity::protocol::play::clientbound::Packet::PlayerPosition(_)
                    )
            })
            .await?;
        }
        bot.set_min_section(if dimension == "minecraft:overworld" {
            -4
        } else {
            0
        });
        bot.move_to([origin + 48., y, origin], false).await?;
        let center = (origin.floor() as i32 >> 4) + 3;
        if !bot.snapshot().chunks.contains(&(center, center - 3)) {
            bot.wait_for("newly walked terrain", Ticks(2400), |state, _| {
                state.chunks.contains(&(center, center - 3))
            })
            .await?;
        }
        bot.synchronize().await?;
    }
    session.stop().await?;
    let world = session.saved_world()?;
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    let resources = std::sync::Arc::new(ResourceManager::new(vec![Pack::vanilla_jar(
        std::path::Path::new(&jar),
    )?]));
    let registry = RegistryAdapter::new(&RegistrySet::load(resources)?)?;
    let mut log = ObservationLog::default();
    for (dimension, origin) in [
        ("minecraft:overworld", 512),
        ("minecraft:the_nether", 1024),
        ("minecraft:the_end", 2048),
    ] {
        for x in origin..=origin + 3 {
            let pos = ChunkPos::new(x, origin);
            let disk = world
                .chunk(dimension, pos)?
                .expect("walked chunk must be saved");
            ensure!(
                disk.status
                    .as_ref()
                    .is_some_and(|s| s.to_string_lossy() == "minecraft:full")
            );
            let mut blocks = BTreeMap::new();
            for section in disk.sections.as_deref().unwrap_or_default() {
                if let Some(runtime) = section.to_runtime(&registry)? {
                    let states = (0..4096)
                        .map(|index| {
                            runtime
                                .get_block_state(
                                    (index & 15) as u8,
                                    (index >> 8) as u8,
                                    ((index >> 4) & 15) as u8,
                                )
                                .id()
                        })
                        .collect::<Vec<_>>();
                    blocks.insert(section.y.expect("section Y"), states);
                }
            }
            log.record(format!("{dimension}.{x}.blocks"), blocks)?;
            // Coordinates become explicit strings because JSON map keys are scalar.
            let entities = block_entities(disk.block_entities.unwrap_or_default())
                .into_iter()
                .map(|(pos, tag)| (format!("{},{},{}", pos[0], pos[1], pos[2]), tag))
                .collect::<BTreeMap<_, _>>();
            log.record(format!("{dimension}.{x}.block_entities"), entities)?;
            let heightmaps = disk
                .heightmaps
                .unwrap_or_default()
                .into_iter()
                .map(|(name, words)| (name.to_string_lossy(), words.0))
                .collect::<BTreeMap<_, _>>();
            log.record(format!("{dimension}.{x}.heightmaps"), heightmaps)?;
            log.record(
                format!("{dimension}.{x}.structures"),
                canonical(&Tag::Compound(
                    disk.structures.expect("structure payload").to_nbt(),
                )),
            )?;
        }
    }
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "gated fresh vanilla worlds: Strict shape/survival/liquid, block-entity, entity, dimension transfer and synchronous-neighbor behavior pending; enable FERRALOOM_GENERATION_PARITY=1 on builder"]
async fn fresh_world_saved_regions_match_after_walking_into_new_terrain() -> Result<()> {
    ensure!(
        std::env::var("FERRALOOM_GENERATION_PARITY").as_deref() == Ok("1"),
        "set FERRALOOM_GENERATION_PARITY=1 explicitly"
    );
    for seed in [0, 8675309] {
        let mut options = LaunchOptions {
            seed,
            ..Default::default()
        };
        options
            .properties
            .insert("generate-structures".into(), "true".into());
        options
            .properties
            .insert("spawn-chunk-radius".into(), "0".into());
        run(options, RunMode::from_env()?, walk_and_compare_saved_chunks).await?;
    }
    Ok(())
}
