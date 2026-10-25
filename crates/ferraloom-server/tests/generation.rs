use anyhow::{Result, ensure};
use ferraloom_data::datapack::{Pack, ResourceManager};
use ferraloom_game::block::UnportedBehavior;
use ferraloom_server::{properties::ServerProperties, registries::RegistrySet, world::WorldRoot};
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
        let mut world_root = WorldRoot::open(
            directory.path(),
            &properties,
            &resources,
            &registries,
            policy,
        )?;
        let mut world = world_root
            .open_dimension("minecraft:overworld".parse().unwrap(), &properties)
            .unwrap();
        ensure!(
            world.generation_skips.is_empty(),
            "unexpected {policy:?} generation skips"
        );
        let pos = ChunkPos::containing(world_root.spawn_position());
        world.request(0, pos);
        let terrain = tokio::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                world.poll_generation_progress()?;
                if let Some((_, terrain)) = world.take_generation_completion()? {
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
        block::BlockBehavior,
        entity::EntityRef,
        level::{Level, LevelContext, LevelError, Region},
    };
    use ferraloom_util::BlockPos;
    struct PendingPolicyFixture;
    impl BlockBehavior for PendingPolicyFixture {
        fn step_on(
            &self,
            state: BlockState,
            _: BlockPos,
            _: EntityRef,
            _: &mut Level,
        ) -> std::result::Result<(), LevelError> {
            Err(LevelError::UnsupportedBlock(state.block()))
        }
    }
    static PENDING: Behaviors = Behaviors::EMPTY.with_block(Block::STONE, &PendingPolicyFixture);
    let mut level = Level::new(Region::new(&PENDING), LevelContext::fixture(0, 63));
    let state = Block::STONE.default_state();
    ensure!(matches!(
        level.call_step_on(state, BlockPos::ZERO, EntityRef(0)),
        Err(LevelError::UnsupportedBlock(Block::STONE))
    ));
    ensure!(level.unported_behavior_stats().skipped_calls == 0);
    level.set_unported_behavior(UnportedBehavior::Warn);
    ensure!(
        level
            .call_step_on(state, BlockPos::ZERO, EntityRef(0))
            .is_ok()
    );
    ensure!(level.unported_behavior_stats().skipped_calls == 1);
    Ok(())
}
