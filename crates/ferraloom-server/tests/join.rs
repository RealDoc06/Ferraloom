mod support;
use anyhow::Result;
use ferraloom_server::server::{MinecraftServer, RunningServer, ServerOptions};

async fn ferraloom_scenario(
    server: RunningServer,
    sample_periodic_time: bool,
) -> Result<support::Outcome> {
    match support::scenario(server.address, sample_periodic_time).await {
        Ok(outcome) => {
            server.wait().await?;
            Ok(outcome)
        }
        Err(error) => {
            let stopped = tokio::time::timeout(std::time::Duration::from_secs(60), async {
                let _ = server.command("stop").await;
                server.wait().await
            })
            .await;
            match stopped {
                Ok(Ok(())) => Err(error),
                stopped => Err(error.context(format!(
                    "Ferraloom stop/wait after failed join scenario:\n{stopped:?}"
                ))),
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flat_world_boots_and_retains_generator_settings_on_reload() -> Result<()> {
    use ferraloom_nbt::CompoundTag;
    use ferraloom_storage::SavedDataStorage;
    let root = tempfile::tempdir()?;
    support::properties(root.path())?;
    let properties_path = root.path().join("server.properties");
    let properties = std::fs::read_to_string(&properties_path)?;
    let config = serde_json::json!({
        "layers": [{"height":1,"block":"minecraft:bedrock"}, {"height":3,"block":"minecraft:stone"}],
        "biome":"minecraft:plains", "features":false, "lakes":false,
        "structure_overrides":[]
    });
    std::fs::write(
        &properties_path,
        format!("{properties}level-type=flat\ngenerator-settings={config}\n"),
    )?;
    for iteration in 0..2 {
        let mut options = ServerOptions::new(root.path());
        options.console = false;
        options.port = Some(0);
        let server = MinecraftServer::start(options).await?;
        server.command("stop").await?;
        server.wait().await?;
        let data = SavedDataStorage::new(root.path().join("world/data"))
            .load::<CompoundTag>("minecraft:world_gen_settings")?
            .unwrap();
        let level = ferraloom_storage::LevelStorageAccess::open(root.path().join("world"))?
            .read_level_data()?;
        assert_eq!(
            level
                .data
                .get_compound("spawn")
                .unwrap()
                .get_int_array("pos")
                .unwrap(),
            &[0, -60, 0]
        );
        let generator = data
            .value()
            .get_compound("dimensions")
            .unwrap()
            .get_compound("minecraft:overworld")
            .unwrap()
            .get_compound("generator")
            .unwrap();
        assert_eq!(
            generator.get_string("type").unwrap().to_string_lossy(),
            "minecraft:flat"
        );
        assert_eq!(
            generator
                .get_compound("settings")
                .unwrap()
                .get_list("layers")
                .unwrap()
                .0
                .len(),
            2
        );
        if iteration == 0 {
            // Existing saved dimensions, rather than changed properties, select
            // the generator on the second boot.
            std::fs::write(
                &properties_path,
                format!("{properties}level-type=minecraft:debug_all_block_states\n"),
            )?;
        }
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn offline_client_joins_walks_chats_and_saves() -> Result<()> {
    let root = tempfile::tempdir()?;
    support::properties(root.path())?;
    let mut options = ServerOptions::new(root.path());
    options.console = false;
    options.port = Some(0);
    let server = MinecraftServer::start(options).await?;
    let outcome = ferraloom_scenario(server, false).await?;
    assert!(outcome.chunks >= 9);
    assert!(outcome.unloaded > 0);
    support::verify_save(root.path(), outcome.position)?;
    eprintln!(
        "Ferraloom login: {:?}; configuration: {:?}; join: {:?}",
        outcome.sequence.login, outcome.sequence.configuration, outcome.sequence.join
    );
    Ok(())
}

/// Java is confined to the explicit parity gate; ordinary tests remain Java-free.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "gated both-server join parity requires the vanilla server jar and builder JDK"]
async fn join_and_configuration_sequence_matches_vanilla() -> Result<()> {
    use std::{
        io::Write,
        net::TcpListener,
        process::{Command, Stdio},
        time::Duration,
    };
    let vanilla_root = tempfile::tempdir()?;
    support::properties(vanilla_root.path())?;
    let reservation = TcpListener::bind("127.0.0.1:0")?;
    let address = reservation.local_addr()?;
    drop(reservation);
    let path = vanilla_root.path().join("server.properties");
    let props = std::fs::read_to_string(&path)?
        .replace("server-port=0", &format!("server-port={}", address.port()));
    std::fs::write(path, props)?;
    let java = std::path::PathBuf::from(std::env::var_os("JAVA_HOME").expect("builder JDK"))
        .join("bin/java");
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder jar");
    let log = std::fs::File::create(vanilla_root.path().join("server.log"))?;
    let child = Command::new(java)
        .args(["-Xms128M", "-Xmx1G", "-jar"])
        .arg(jar)
        .arg("nogui")
        .current_dir(vanilla_root.path())
        .stdin(Stdio::piped())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()?;
    let mut vanilla = VanillaGuard(child);
    let ready = tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            if let Ok(stream) = tokio::net::TcpStream::connect(address).await {
                drop(stream);
                break;
            }
            if let Some(status) = vanilla.0.try_wait().unwrap() {
                panic!(
                    "vanilla exited {status}: {}",
                    std::fs::read_to_string(vanilla_root.path().join("server.log")).unwrap()
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    ready?;
    let vanilla_result = support::scenario(address, true).await;
    if vanilla_result.is_err() {
        eprintln!(
            "Vanilla log: {}",
            std::fs::read_to_string(vanilla_root.path().join("server.log"))?
        );
    }
    let vanilla_outcome = vanilla_result?;
    tokio::time::timeout(Duration::from_secs(30), async {
        while vanilla.0.try_wait().unwrap().is_none() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await?;
    support::verify_save(vanilla_root.path(), vanilla_outcome.position)?;
    let root = tempfile::tempdir()?;
    support::properties(root.path())?;
    let mut options = ServerOptions::new(root.path());
    options.console = false;
    options.port = Some(0);
    let server = MinecraftServer::start(options).await?;
    let outcome = ferraloom_scenario(server, true).await?;
    support::verify_save(root.path(), outcome.position)?;
    for (name, sequence) in [
        ("Vanilla", &vanilla_outcome.sequence),
        ("Ferraloom", &outcome.sequence),
    ] {
        eprintln!(
            "{name} login: {:?}; configuration: {:?}; join: {:?}",
            sequence.login, sequence.configuration, sequence.join
        );
    }
    assert_eq!(outcome.sequence, vanilla_outcome.sequence);
    for (root, outcome) in [
        (vanilla_root.path(), &vanilla_outcome),
        (root.path(), &outcome),
    ] {
        let saved =
            ferraloom_storage::LevelStorageAccess::open(root.join("world"))?.read_level_data()?;
        let expected = ferraloom_protocol::play::clientbound::SetTime {
            game_time: saved.data.get_long("Time").expect("saved overworld Time"),
            clock_updates: Vec::new(),
        };
        assert_eq!(outcome.frozen_periodic_time.as_ref(), Some(&expected));
        for time in outcome.time_updates.iter().skip(1) {
            assert!(time.clock_updates.is_empty(), "periodic SetTime: {time:?}");
        }
    }
    struct VanillaGuard(std::process::Child);
    impl Drop for VanillaGuard {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                if let Some(stdin) = &mut self.0.stdin {
                    let _ = stdin.write_all(b"stop\n");
                }
                let _ = self.0.wait();
            }
        }
    }
    Ok(())
}

/// The same join/walk/save sequence on normal noise terrain, independently of flat coverage.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla jar/JDK; default-noise both-server parity"]
async fn default_noise_join_and_configuration_sequence_matches_vanilla() -> Result<()> {
    use std::{
        io::Write,
        net::TcpListener,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    fn properties(root: &std::path::Path) -> Result<()> {
        support::properties(root)?;
        let path = root.join("server.properties");
        let mut properties = std::fs::read_to_string(&path)?
            .lines()
            .filter(|line| {
                !line.starts_with("level-type=") && !line.starts_with("generator-settings=")
            })
            .map(|line| format!("{line}\n"))
            .collect::<String>();
        properties.push_str("level-type=minecraft:normal\n");
        std::fs::write(path, properties)?;
        Ok(())
    }
    let vanilla_started = Instant::now();
    let vanilla_root = tempfile::tempdir()?;
    properties(vanilla_root.path())?;
    let reservation = TcpListener::bind("127.0.0.1:0")?;
    let address = reservation.local_addr()?;
    drop(reservation);
    let path = vanilla_root.path().join("server.properties");
    let props = std::fs::read_to_string(&path)?
        .replace("server-port=0", &format!("server-port={}", address.port()));
    std::fs::write(path, props)?;
    let java = std::path::PathBuf::from(std::env::var_os("JAVA_HOME").expect("builder JDK"))
        .join("bin/java");
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder jar");
    let log = std::fs::File::create(vanilla_root.path().join("server.log"))?;
    let child = Command::new(java)
        .args(["-Xms128M", "-Xmx1G", "-jar"])
        .arg(jar)
        .arg("nogui")
        .current_dir(vanilla_root.path())
        .stdin(Stdio::piped())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()?;
    let mut vanilla = VanillaGuard(child);
    let ready = tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            if let Ok(stream) = tokio::net::TcpStream::connect(address).await {
                drop(stream);
                break;
            }
            if let Some(status) = vanilla.0.try_wait().unwrap() {
                panic!(
                    "vanilla exited {status}: {}",
                    std::fs::read_to_string(vanilla_root.path().join("server.log")).unwrap()
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    ready?;
    let vanilla_result = support::scenario(address, true).await;
    if vanilla_result.is_err() {
        eprintln!(
            "Vanilla log: {}",
            std::fs::read_to_string(vanilla_root.path().join("server.log"))?
        );
    }
    let vanilla_outcome = vanilla_result?;
    tokio::time::timeout(Duration::from_secs(30), async {
        while vanilla.0.try_wait().unwrap().is_none() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await?;
    support::verify_save(vanilla_root.path(), vanilla_outcome.position)?;
    eprintln!(
        "Vanilla default-noise join: {:?}, {} chunks, {} unloads",
        vanilla_started.elapsed(),
        vanilla_outcome.chunks,
        vanilla_outcome.unloaded
    );
    let ferraloom_started = Instant::now();
    let root = tempfile::tempdir()?;
    properties(root.path())?;
    let mut options = ServerOptions::new(root.path());
    options.unported_behavior = ferraloom_game::block::UnportedBehavior::Strict;
    options.console = false;
    options.port = Some(0);
    let outcome = async {
        let server = MinecraftServer::start(options).await?;
        ferraloom_scenario(server, true).await
    }
    .await;
    eprintln!(
        "Ferraloom default-noise join elapsed: {:?}",
        ferraloom_started.elapsed()
    );
    let outcome = outcome?;
    eprintln!(
        "Ferraloom default-noise join: {} chunks, {} unloads",
        outcome.chunks, outcome.unloaded
    );
    support::verify_save(root.path(), outcome.position)?;
    for (name, sequence) in [
        ("Vanilla", &vanilla_outcome.sequence),
        ("Ferraloom", &outcome.sequence),
    ] {
        eprintln!(
            "{name} login: {:?}; configuration: {:?}; join: {:?}",
            sequence.login, sequence.configuration, sequence.join
        );
    }
    assert_eq!(outcome.sequence, vanilla_outcome.sequence);
    for (root, outcome) in [
        (vanilla_root.path(), &vanilla_outcome),
        (root.path(), &outcome),
    ] {
        let saved =
            ferraloom_storage::LevelStorageAccess::open(root.join("world"))?.read_level_data()?;
        let expected = ferraloom_protocol::play::clientbound::SetTime {
            game_time: saved.data.get_long("Time").expect("saved overworld Time"),
            clock_updates: Vec::new(),
        };
        assert_eq!(outcome.frozen_periodic_time.as_ref(), Some(&expected));
        for time in outcome.time_updates.iter().skip(1) {
            assert!(time.clock_updates.is_empty(), "periodic SetTime: {time:?}");
        }
    }
    struct VanillaGuard(std::process::Child);
    impl Drop for VanillaGuard {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                if let Some(stdin) = &mut self.0.stdin {
                    let _ = stdin.write_all(b"stop\n");
                }
                let _ = self.0.wait();
            }
        }
    }
    Ok(())
}
