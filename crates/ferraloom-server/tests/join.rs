mod support;
use anyhow::Result;
use ferraloom_server::server::{MinecraftServer, ServerOptions};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn offline_client_joins_walks_chats_and_saves() -> Result<()> {
    let root = tempfile::tempdir()?;
    support::properties(root.path())?;
    let mut options = ServerOptions::new(root.path());
    options.console = false;
    options.port = Some(0);
    let server = MinecraftServer::start(options).await?;
    let outcome = support::scenario(server.address).await?;
    server.wait().await?;
    assert!(outcome.chunks >= 9);
    assert!(outcome.unloaded > 0);
    support::verify_save(root.path(), outcome.position)?;
    eprintln!("Ferraloom packet sequence: {:?}", outcome.sequence);
    Ok(())
}

/// Java is confined to the explicit parity gate; ordinary tests remain Java-free.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires JAVA_HOME and FERRALOOM_VANILLA_JAR; run on the builder"]
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
    let vanilla_result = support::scenario(address).await;
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
    let outcome = support::scenario(server.address).await?;
    server.wait().await?;
    support::verify_save(root.path(), outcome.position)?;
    eprintln!(
        "Vanilla packet sequence: {:?}\nFerraloom packet sequence: {:?}",
        vanilla_outcome.sequence, outcome.sequence
    );
    assert_eq!(outcome.sequence, vanilla_outcome.sequence);
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
