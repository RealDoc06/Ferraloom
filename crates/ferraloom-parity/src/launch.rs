use anyhow::{Context, Result, bail, ensure};
use ferraloom_protocol::Uuid;
use ferraloom_server::server::{MinecraftServer, RunningServer, ServerOptions};
use std::{
    collections::BTreeMap,
    net::{SocketAddr, TcpListener},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Regions {
    Off,
    On,
}
#[derive(Clone, Debug)]
pub enum ServerKind {
    Vanilla,
    Ferraloom,
    FerraloomBinary(PathBuf),
}
#[derive(Clone, Debug, Default)]
pub enum WorldPreset {
    #[default]
    Default,
    /// Layers are ordered bottom to top as (block identifier, height).
    Flat {
        layers: Vec<(String, u32)>,
        biome: String,
    },
    Void,
}
#[derive(Clone, Debug)]
pub struct LaunchOptions {
    pub seed: i64,
    pub preset: WorldPreset,
    /// A saved world directory, copied into the disposable universe as `world`.
    pub fixture: Option<PathBuf>,
    pub properties: BTreeMap<String, String>,
    pub operators: Vec<String>,
    /// Applied by a setup op bot before any scenario bot joins; errors fail startup.
    pub game_rules: BTreeMap<String, String>,
    pub regions: Regions,
    pub java: Option<PathBuf>,
    pub vanilla_jar: Option<PathBuf>,
    pub ferraloom_binary: Option<PathBuf>,
    pub startup_timeout: Duration,
    pub stop_timeout: Duration,
}
impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            seed: 12345,
            preset: WorldPreset::Default,
            fixture: None,
            properties: BTreeMap::new(),
            operators: vec!["ParityBot".into()],
            game_rules: BTreeMap::new(),
            regions: Regions::Off,
            java: None,
            vanilla_jar: None,
            ferraloom_binary: None,
            startup_timeout: Duration::from_secs(180),
            stop_timeout: Duration::from_secs(60),
        }
    }
}
pub(crate) enum Process {
    InProcess(RunningServer),
    External(Child),
}
pub(crate) struct Launched {
    pub address: SocketAddr,
    pub process: Process,
}
impl Launched {
    pub async fn command(&mut self, text: &str) -> Result<()> {
        match &mut self.process {
            Process::InProcess(server) => server.command(text).await,
            Process::External(child) => {
                child
                    .stdin
                    .as_mut()
                    .context("server console closed")?
                    .write_all(format!("{text}\n").as_bytes())
                    .await?;
                Ok(())
            }
        }
    }
    pub async fn stop(mut self, duration: Duration) -> Result<()> {
        // A scenario may already have issued /stop. Successful exit/save is authoritative.
        let _ = self.command("stop").await;
        tokio::time::timeout(duration, async {
            match self.process {
                Process::InProcess(server) => server.wait().await,
                Process::External(mut child) => {
                    ensure!(
                        child.wait().await?.success(),
                        "server exited unsuccessfully"
                    );
                    Ok(())
                }
            }
        })
        .await
        .context("server did not stop and save before watchdog")?
    }
}
pub(crate) fn prepare(options: &LaunchOptions) -> Result<TempDir> {
    let root = tempfile::tempdir()?;
    if let Some(fixture) = &options.fixture {
        copy_world(fixture, &root.path().join("world"))?;
    }
    // EULA acceptance is confined to this newly-created disposable universe.
    std::fs::write(root.path().join("eula.txt"), "eula=true\n")?;
    let mut operators = options.operators.clone();
    if !options.game_rules.is_empty() {
        operators.push("ParitySetup".into());
    }
    let ops:Vec<_>=operators.iter().map(|name|serde_json::json!({"uuid":Uuid::offline_player(name).to_string(),"name":name,"level":4,"bypassesPlayerLimit":true})).collect();
    std::fs::write(root.path().join("ops.json"), serde_json::to_vec(&ops)?)?;
    Ok(root)
}
fn copy_world(source: &Path, target: &Path) -> Result<()> {
    ensure!(
        source.is_dir(),
        "fixture is not a directory: {}",
        source.display()
    );
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if entry.file_name() == "session.lock" {
            continue;
        }
        ensure!(
            !kind.is_symlink(),
            "fixture symlinks are unsupported: {}",
            entry.path().display()
        );
        if kind.is_dir() {
            copy_world(&entry.path(), &target.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), target.join(entry.file_name()))?;
        }
    }
    Ok(())
}
pub(crate) async fn launch(
    kind: &ServerKind,
    options: &LaunchOptions,
    root: &Path,
) -> Result<Launched> {
    let reservation = TcpListener::bind("127.0.0.1:0")?;
    let address = reservation.local_addr()?;
    let mut props: BTreeMap<String, String> = [
        ("server-ip", "127.0.0.1"),
        ("online-mode", "false"),
        ("white-list", "false"),
        ("enforce-secure-profile", "false"),
        ("view-distance", "2"),
        ("simulation-distance", "2"),
        ("generate-structures", "false"),
        ("pause-when-empty-seconds", "0"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect();
    props.insert("level-seed".into(), options.seed.to_string());
    match &options.preset {
        WorldPreset::Default => {}
        preset => {
            props.insert("level-type".into(), "minecraft:flat".into());
            let (layers, biome) = match preset {
                WorldPreset::Flat { layers, biome } => (layers.clone(), biome.clone()),
                _ => (
                    vec![("minecraft:air".into(), 1)],
                    "minecraft:the_void".into(),
                ),
            };
            props.insert("generator-settings".into(),serde_json::json!({"layers":layers.into_iter().map(|(block,height)|serde_json::json!({"block":block,"height":height})).collect::<Vec<_>>(),"biome":biome,"features":false,"lakes":false,"structure_overrides":[]}).to_string());
        }
    }
    props.extend(options.properties.clone());
    // These isolation invariants cannot be overridden by a scenario.
    props.insert("server-ip".into(), "127.0.0.1".into());
    props.insert("server-port".into(), address.port().to_string());
    props.insert("online-mode".into(), "false".into());
    props.insert("level-name".into(), "world".into());
    for (k, v) in &props {
        ensure!(
            !k.contains(['\n', '\r', '=', '\\']) && !v.contains(['\n', '\r']),
            "invalid property {k}"
        );
    }
    std::fs::write(
        root.join("server.properties"),
        props
            .iter()
            .map(|(k, v)| format!("{k}={v}\n"))
            .collect::<String>(),
    )?;
    drop(reservation);
    let process = match kind {
        ServerKind::Ferraloom => {
            ensure!(
                options.regions == Regions::Off,
                "in-process Ferraloom currently supports only Regions::Off"
            );
            let mut config = ServerOptions::new(root);
            config.console = false;
            config.port = Some(address.port());
            let server =
                tokio::time::timeout(options.startup_timeout, MinecraftServer::start(config))
                    .await
                    .context("Ferraloom startup watchdog")??;
            Process::InProcess(server)
        }
        _ => {
            let mut command = match kind {
                ServerKind::Vanilla => {
                    let java = options
                        .java
                        .clone()
                        .or_else(|| {
                            std::env::var_os("JAVA_HOME").map(|p| PathBuf::from(p).join("bin/java"))
                        })
                        .context("set JAVA_HOME or LaunchOptions::java")?;
                    let jar = options
                        .vanilla_jar
                        .clone()
                        .or_else(|| std::env::var_os("FERRALOOM_VANILLA_JAR").map(PathBuf::from))
                        .context("set FERRALOOM_VANILLA_JAR or LaunchOptions::vanilla_jar")?;
                    let mut c = Command::new(java);
                    c.args(["-Xms128M", "-Xmx1G", "-jar"]).arg(jar).arg("nogui");
                    c
                }
                ServerKind::FerraloomBinary(binary) => {
                    let mut c = Command::new(
                        std::fs::canonicalize(binary)
                            .with_context(|| format!("Ferraloom binary {}", binary.display()))?,
                    );
                    c.args(["--unported-behavior", "strict"]);
                    c.args([
                        "nogui",
                        "--regions",
                        if options.regions == Regions::Off {
                            "off"
                        } else {
                            "on"
                        },
                    ]);
                    c
                }
                _ => unreachable!(),
            };
            let log = std::fs::File::create(root.join("server.log"))?;
            let mut child = command
                .current_dir(root)
                .stdin(Stdio::piped())
                .stdout(log.try_clone()?)
                .stderr(log)
                .kill_on_drop(true)
                .spawn()?;
            tokio::time::timeout(options.startup_timeout, async {
                loop {
                    if let Some(status) = child.try_wait()? {
                        bail!(
                            "server exited {status}: {}",
                            std::fs::read_to_string(root.join("server.log"))?
                        );
                    }
                    if let Ok(socket) = tokio::net::TcpStream::connect(address).await {
                        drop(socket);
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Ok::<_, anyhow::Error>(())
            })
            .await
            .context("server startup watchdog")??;
            Process::External(child)
        }
    };
    let launched = Launched { address, process };
    if !options.game_rules.is_empty() {
        let configured = async {
            let mut setup = crate::Bot::connect(address, "ParitySetup").await?;
            setup.synchronize().await?;
            for (rule, value) in &options.game_rules {
                setup.command(&format!("gamerule {rule} {value}")).await?;
                let packet = setup
                    .wait_for("game rule result", crate::Ticks::DEFAULT, |_, p| {
                        matches!(
                            p,
                            ferraloom_protocol::play::clientbound::Packet::SystemChat(_)
                        )
                    })
                    .await?;
                let ferraloom_protocol::play::clientbound::Packet::SystemChat(p) = packet.as_ref()
                else {
                    unreachable!()
                };
                ensure!(
                    game_rule_is_installed(&p.content, rule, value),
                    "game rule {rule}={value} failed: {:?}",
                    p.content
                );
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(error) = configured {
            let _ = launched.stop(options.stop_timeout).await;
            return Err(error);
        }
    }
    Ok(launched)
}

/// GameRuleCommand#setRule reports unchanged values as a translated error.
/// That exact rule/value reply proves installation just as a successful set does.
fn game_rule_is_installed(
    message: &ferraloom_protocol::chat::Component,
    rule: &str,
    value: &str,
) -> bool {
    use ferraloom_protocol::chat::{Argument, Contents};
    let installed = match &message.contents {
        Contents::Translatable { key, args, .. } if key == "commands.gamerule.not_set" => {
            matches!(args.as_slice(), [Argument::String(name), Argument::String(installed)]
                if (name == rule || name == rule.strip_prefix("minecraft:").unwrap_or(rule))
                    && installed == value)
        }
        Contents::Translatable { key, .. } => key == "commands.gamerule.set",
        _ => false,
    };
    installed
        || message
            .extra
            .iter()
            .any(|child| game_rule_is_installed(child, rule, value))
}

#[cfg(test)]
mod tests {
    use super::game_rule_is_installed;
    use ferraloom_protocol::chat::{Argument, Component};

    #[test]
    fn unchanged_game_rule_requires_exact_rule_and_serialized_value() {
        let response = |key: &str, rule: &str, value: Argument| {
            Component::translatable(key, vec![Argument::String(rule.into()), value])
        };
        let value = || Argument::String("0".into());
        let mut native_error = Component::literal("");
        native_error.extra.push(response(
            "commands.gamerule.not_set",
            "random_tick_speed",
            value(),
        ));
        let check = |message: &Component| {
            game_rule_is_installed(message, "minecraft:random_tick_speed", "0")
        };
        assert!(check(&native_error));
        assert!(check(&response(
            "commands.gamerule.set",
            "random_tick_speed",
            value()
        )));
        assert!(!check(&response(
            "commands.gamerule.not_set",
            "random_tick_speed",
            Argument::String("1".into())
        )));
        assert!(!check(&response(
            "commands.gamerule.not_set",
            "snow_accumulation_height",
            value()
        )));
        assert!(!check(&response(
            "commands.gamerule.query",
            "random_tick_speed",
            value()
        )));
        assert!(!check(&Component::literal("commands.gamerule.set")));
        assert!(!check(&response(
            "commands.gamerule.not_set",
            "random_tick_speed",
            Argument::Component(Box::new(Component::literal("0")))
        )));
    }
}
