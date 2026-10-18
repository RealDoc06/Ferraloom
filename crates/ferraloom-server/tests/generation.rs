use anyhow::{Result, ensure};
use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_game::block::UnportedBehavior;
use ferraloom_server::{properties::ServerProperties, registries::RegistrySet, world::World};
use ferraloom_util::ChunkPos;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_noise_generation_warn_reaches_full_and_strict_rejects_missing_behavior()
-> Result<()> {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar))?]);
    let registries = RegistrySet::load(&resources)?;
    let mut properties = ServerProperties::from_values(Default::default());
    properties.set("level-seed", 0);
    let strict = tempfile::tempdir()?;
    let error = match World::open(
        strict.path(),
        &properties,
        &resources,
        &registries,
        UnportedBehavior::Strict,
    ) {
        Ok(_) => anyhow::bail!("Strict generation accepted missing block behavior"),
        Err(error) => error,
    };
    ensure!(
        format!("{error:#}").contains("Unsupported"),
        "unexpected strict failure: {error:#}"
    );
    let warn = tempfile::tempdir()?;
    let mut world = World::open(
        warn.path(),
        &properties,
        &resources,
        &registries,
        UnportedBehavior::Warn,
    )?;
    ensure!(
        !world.generation_skips.is_empty(),
        "Warn must expose generation skips"
    );
    let pos = ChunkPos::containing(world.spawn);
    world.request(0, pos);
    let terrain = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let mut completed = world.poll_terrain()?;
            if let Some((_, terrain)) = completed.pop() {
                break Ok::<_, anyhow::Error>(terrain);
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await??;
    ensure!(
        terrain
            .disk
            .status
            .as_ref()
            .is_some_and(|s| s.to_string_lossy() == "minecraft:full")
    );
    world.close_chunk_storage().await?;
    Ok(())
}
