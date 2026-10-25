//! Load the same legacy chunk entity and persistent entity-region entity on
//! both live servers. Only disposable fixture regions are written by this test.
#[expect(
    dead_code,
    reason = "Generation uses the shared command/freezing helpers."
)]
mod level_support;

use anyhow::Context;
use ferraloom_data::{EntityType, Item};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_parity::{
    LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, WorldPreset, ensure,
    protocol::{Uuid, entity_data::EntityDataValue},
};
use ferraloom_storage::{
    BlockStatePaletteEntry, ChunkSection, EntityChunk, IntArray, LevelStorageAccess, PackedPalette,
    RegionFileStorage, RegionFileVersion, SerializableChunkData,
};
use ferraloom_util::ChunkPos;
use std::path::Path;

const CENTER: ChunkPos = ChunkPos::new(64, 64);
const UUIDS: [Uuid; 2] = [Uuid::new(0, 101), Uuid::new(0, 102)];

fn item(serial: i32, x: f64) -> Result<CompoundTag> {
    // ItemEntity's native infinite age/pickup-delay sentinels and NoGravity
    // remove simulation-time variation without changing registration/tracking.
    Ok(ferraloom_nbt::parse_compound(&format!(
        "{{id:\"minecraft:item\",UUID:[I;0,0,0,{serial}],Pos:[{x}d,80.0d,1028.5d],Motion:[0d,0d,0d],Rotation:[0f,0f],NoGravity:1b,Age:-32768s,PickupDelay:32767s,Item:{{id:\"minecraft:stone\",count:1}}}}"
    ))?)
}

fn fixture(path: &Path) -> Result<()> {
    let sections = (-4..20)
        .map(|y| ChunkSection {
            y: Some(y),
            block_states: Some(PackedPalette {
                palette: vec![BlockStatePaletteEntry {
                    name: Some("minecraft:air".into()),
                    ..Default::default()
                }],
                data: None,
                extra: CompoundTag::new(),
            }),
            biomes: Some(PackedPalette {
                palette: vec!["minecraft:plains".into()],
                data: None,
                extra: CompoundTag::new(),
            }),
            ..Default::default()
        })
        .collect();
    let chunk = SerializableChunkData {
        data_version: Some(ferraloom_storage::DATA_VERSION),
        x_pos: Some(CENTER.x),
        z_pos: Some(CENTER.z),
        y_pos: Some(-4),
        status: Some("minecraft:full".into()),
        is_light_on: Some(false),
        sections: Some(sections),
        entities: Some(vec![item(101, 1028.5)?]),
        ..Default::default()
    };
    let entity_chunk = EntityChunk {
        data_version: Some(ferraloom_storage::DATA_VERSION),
        position: Some(IntArray(vec![CENTER.x, CENTER.z])),
        entities: Some(vec![item(102, 1030.5)?]),
        ..Default::default()
    };
    let access = LevelStorageAccess::open(path)?;
    let dimension = access.dimension("minecraft:overworld")?;
    for (directory, tag) in [
        (dimension.region, chunk.write()?),
        (dimension.entities, entity_chunk.write()?),
    ] {
        std::fs::create_dir_all(&directory)?;
        let mut storage = RegionFileStorage::new(directory, RegionFileVersion::Deflate, false);
        storage.write(CENTER, Some(&tag))?;
        storage.close()?;
    }
    Ok(())
}

async fn load_and_track(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    level_support::freeze(bot).await?;
    level_support::command(
        bot,
        "forceload add 1024 1024",
        "commands.forceload.added.single",
    )
    .await?;
    let position = [1024.5, 80.0, 1024.5];
    bot.teleport(position).await?;
    if !bot.snapshot().chunks.contains(&(CENTER.x, CENTER.z)) {
        bot.wait_for("loaded entity chunk sent", Ticks::DEFAULT, |state, _| {
            state.chunks.contains(&(CENTER.x, CENTER.z))
        })
        .await?;
    }
    // ChunkMap#move retries entity pairing through isChunkTracked, which is
    // false while PlayerChunkSender still has the chunk pending after teleport.
    bot.move_to(position, false).await?;
    let paired = |state: &ferraloom_parity::BotState| {
        UUIDS.iter().all(|uuid| {
            state.entities.values().any(|entity| {
                entity.uuid == *uuid
                    && entity.data.values().any(|value| {
                        matches!(value,
                        EntityDataValue::ItemStack(stack) if stack.item() == Item::STONE
                            && stack.0.as_ref().is_some_and(|stack| stack.count == 1))
                    })
            })
        })
    };
    if !paired(&bot.snapshot()) {
        bot.wait_for(
            "both loaded entities paired with metadata",
            Ticks::DEFAULT,
            |state, _| paired(state),
        )
        .await
        .with_context(|| format!("loaded entity snapshot: {:?}", bot.snapshot().entities))?;
    }
    bot.synchronize().await?;
    let snapshot = bot.snapshot();
    let mut log = ObservationLog::default();
    for (index, uuid) in UUIDS.into_iter().enumerate() {
        let matches = snapshot
            .entities
            .values()
            .filter(|entity| entity.uuid == uuid)
            .collect::<Vec<_>>();
        ensure!(
            matches.len() == 1,
            "loaded UUID was published more than once"
        );
        let entity = matches[0];
        ensure!(entity.entity_type == i32::from(EntityType::ITEM.protocol_id()));
        ensure!(entity.position == [1028.5 + index as f64 * 2., 80.0, 1028.5]);
        log.record(format!("entity.{index}.type"), entity.entity_type)?;
        log.record(format!("entity.{index}.position"), entity.position)?;
        log.record(format!("entity.{index}.stack"), ("minecraft:stone", 1))?;
    }
    session.stop().await?;
    let world = session.saved_world()?;
    let block_chunk = world
        .chunk("minecraft:overworld", CENTER)?
        .context("FULL chunk was not saved")?;
    ensure!(
        block_chunk
            .entities
            .as_deref()
            .unwrap_or_default()
            .is_empty(),
        "legacy entity NBT was not consumed from the block chunk"
    );
    let saved = world
        .entities("minecraft:overworld", CENTER)?
        .context("loaded entities were not saved")?;
    let mut uuids = saved
        .entities
        .unwrap_or_default()
        .into_iter()
        .filter(|tag| {
            tag.get_string("id")
                .is_some_and(|id| id.to_string_lossy() == "minecraft:item")
        })
        .map(|tag| match tag.get("UUID") {
            Some(Tag::IntArray(uuid)) => Ok(uuid.clone()),
            _ => anyhow::bail!("saved item omitted UUID"),
        })
        .collect::<Result<Vec<_>>>()?;
    uuids.sort();
    ensure!(
        uuids == [vec![0, 0, 0, 101], vec![0, 0, 0, 102]],
        "saved loaded-entity UUIDs differ: {uuids:?}"
    );
    log.record("saved.entity_uuids", uuids)?;
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla jar/JDK; Strict both-server chunk/entity IO publication"]
async fn loaded_chunk_entities_track_and_save_once_on_both_servers() -> Result<()> {
    let world = tempfile::tempdir()?;
    fixture(world.path())?;
    let options = LaunchOptions {
        fixture: Some(world.path().to_owned()),
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
            biome: "minecraft:plains".into(),
        },
        properties: [("gamemode".into(), "creative".into())].into(),
        ..level_support::options()
    };
    ferraloom_parity::run(options, RunMode::Both, load_and_track).await
}
