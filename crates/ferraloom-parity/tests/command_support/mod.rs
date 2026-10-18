use ferraloom_parity::{LaunchOptions, WorldPreset};

pub fn options() -> LaunchOptions {
    // Bedrock isolates command parity from unported terrain random ticks under Strict.
    LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
            biome: "minecraft:plains".into(),
        },
        ..LaunchOptions::default()
    }
}

macro_rules! command_test {
    ($name:ident, $scenario:path) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        #[ignore = "requires builder vanilla jar/JDK; run with --ignored"]
        async fn $name() -> ferraloom_parity::Result<()> {
            ferraloom_parity::run(
                $crate::command_support::options(),
                ferraloom_parity::RunMode::from_env()?,
                $scenario,
            )
            .await
        }
    };
}
pub(crate) use command_test;
