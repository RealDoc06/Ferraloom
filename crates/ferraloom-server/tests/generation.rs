use anyhow::{Result, ensure};
use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_game::block::UnportedBehavior;
use ferraloom_server::{properties::ServerProperties, registries::RegistrySet, world::World};
use ferraloom_util::ChunkPos;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_noise_reaches_full_without_skips_and_synthetic_unported_is_rejected() -> Result<()>
{
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder provides vanilla jar");
    let resources = ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar))?]);
    let registries = RegistrySet::load(&resources)?;
    let mut properties = ServerProperties::from_values(Default::default());
    properties.set("level-seed", 0);
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let directory = tempfile::tempdir()?;
        let mut world = World::open(
            directory.path(),
            &properties,
            &resources,
            &registries,
            policy,
        )?;
        ensure!(
            world.generation_skips.is_empty(),
            "unexpected {policy:?} generation skips"
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
                .is_some_and(|status| status.to_string_lossy() == "minecraft:full")
        );
        ensure!(
            world.generation_skips.is_empty(),
            "unexpected {policy:?} generation skips"
        );
        world.close_chunk_storage().await?;
    }

    use ferraloom_data::{Block, BlockState};
    use ferraloom_game::{
        Behaviors,
        block::{BlockBehavior, BlockMethod},
        level::{Level, LevelContext, LevelError, Region},
    };
    use ferraloom_util::BlockPos;
    struct PendingPolicyFixture;
    impl BlockBehavior for PendingPolicyFixture {
        fn random_tick(
            &self,
            state: BlockState,
            _: BlockPos,
            _: &mut Level,
        ) -> std::result::Result<(), LevelError> {
            Err(LevelError::UnsupportedBlock(state.block()))
        }
    }
    static PENDING: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &PendingPolicyFixture);
    let mut level = Level::new(Region::new(&PENDING), LevelContext::fixture(0, 63));
    let state = Block::STONE.default_state();
    ensure!(matches!(
        level.optional_block_call(Block::STONE, BlockMethod::RandomTick, |behavior, level| {
            behavior.random_tick(state, BlockPos::ZERO, level)
        }),
        Err(LevelError::UnsupportedBlock(Block::STONE))
    ));
    ensure!(level.unported_behavior_stats().skipped_calls == 0);
    level.set_unported_behavior(UnportedBehavior::Warn);
    ensure!(matches!(
        level.optional_block_call(Block::STONE, BlockMethod::RandomTick, |behavior, level| {
            behavior.random_tick(state, BlockPos::ZERO, level)
        }),
        Ok(None)
    ));
    ensure!(level.unported_behavior_stats().skipped_calls == 1);
    Ok(())
}
