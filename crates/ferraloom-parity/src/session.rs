use crate::{
    Bot, LaunchOptions, Normalization, ObservationLog, Result, SavedWorld, ServerKind,
    launch::{self, Launched},
};
use anyhow::{Context, bail, ensure};
use std::{ops::AsyncFnMut, path::Path};
use tempfile::TempDir;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    Both,
    Vanilla,
    Ferraloom,
}
impl RunMode {
    pub fn from_env() -> Result<Self> {
        match std::env::var("FERRALOOM_PARITY_SIDE").as_deref() {
            Err(std::env::VarError::NotPresent) | Ok("both") => Ok(Self::Both),
            Ok("vanilla") => Ok(Self::Vanilla),
            Ok("ferraloom") => Ok(Self::Ferraloom),
            _ => bail!("FERRALOOM_PARITY_SIDE must be both, vanilla or ferraloom"),
        }
    }
}
/// Owns a disposable universe and independent background clients. No production paths are used.
pub struct Session {
    root: Option<TempDir>,
    server: Option<Launched>,
    kind: ServerKind,
    options: LaunchOptions,
    bots: Vec<Bot>,
    rules: Vec<Normalization>,
}
impl Session {
    pub async fn start(kind: ServerKind, options: LaunchOptions) -> Result<Self> {
        let root = launch::prepare(&options)?;
        let server = launch::launch(&kind, &options, root.path()).await?;
        Ok(Self {
            root: Some(root),
            server: Some(server),
            kind,
            options,
            bots: Vec::new(),
            rules: Vec::new(),
        })
    }
    /// Adds a separately pumped client; configure its op name in LaunchOptions::operators.
    pub async fn join(&mut self, name: &str) -> Result<usize> {
        ensure!(
            !self.bots.iter().any(|bot| bot.name == name),
            "duplicate active bot name {name}"
        );
        let server = self.server.as_ref().context("server is stopped")?;
        let bot = Bot::connect(server.address, name).await?;
        let id = self.bots.len();
        self.bots.push(bot);
        Ok(id)
    }
    pub fn bot(&mut self, id: usize) -> Result<&mut Bot> {
        self.bots
            .get_mut(id)
            .context("bot id is not active (ids expire at restart)")
    }
    /// Allows simultaneous independent bot actions without borrowing the session twice.
    pub fn bots(&mut self) -> &mut [Bot] {
        &mut self.bots
    }
    pub fn normalize(&mut self, rule: Normalization) {
        self.rules.push(rule);
    }
    pub async fn console(&mut self, command: &str) -> Result<()> {
        self.server
            .as_mut()
            .context("server is stopped")?
            .command(command)
            .await
    }
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(server) = self.server.take() {
            let result = server.stop(self.options.stop_timeout).await;
            self.bots.clear();
            result?;
        }
        Ok(())
    }
    pub fn saved_world(&self) -> Result<SavedWorld> {
        ensure!(
            self.server.is_none(),
            "stop the server before reading saved data"
        );
        SavedWorld::open(self.root())
    }
    /// Stops and saves, then relaunches the same universe. Bot handles expire.
    pub async fn restart(&mut self) -> Result<()> {
        self.stop().await?;
        self.server = Some(launch::launch(&self.kind, &self.options, self.root()).await?);
        Ok(())
    }
    fn root(&self) -> &Path {
        self.root.as_ref().expect("session root").path()
    }
    fn diagnostics(&self) -> String {
        std::fs::read_to_string(self.root().join("server.log")).unwrap_or_default()
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Some(server) = self.server.take() {
            let root = self.root.take();
            let duration = self.options.stop_timeout;
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = server.stop(duration).await;
                    drop(root);
                });
            }
        }
    }
}
/// The same async function runs on each backend. Assertions inside it prove its own expectations
/// even in single-side mode. Normalization declarations must also match across runs.
pub async fn run<F>(options: LaunchOptions, mode: RunMode, mut scenario: F) -> Result<()>
where
    F: AsyncFnMut(&mut Session) -> Result<ObservationLog>,
{
    let kinds = match mode {
        RunMode::Both => vec![ServerKind::Vanilla, ferraloom_kind(&options)],
        RunMode::Vanilla => vec![ServerKind::Vanilla],
        RunMode::Ferraloom => vec![ferraloom_kind(&options)],
    };
    let mut expected: Option<(ObservationLog, Vec<Normalization>)> = None;
    for kind in kinds {
        let label = format!("{kind:?}");
        let mut session = Session::start(kind, options.clone())
            .await
            .with_context(|| format!("{label} launch"))?;
        let result = scenario(&mut session).await;
        let stopped = session.stop().await;
        let diagnostics = session.diagnostics();
        let log = result.with_context(|| format!("{label} scenario failed\n{diagnostics}"))?;
        stopped.with_context(|| format!("{label} stop/save failed\n{diagnostics}"))?;
        let log = log.normalized(&session.rules)?;
        eprintln!(
            "{label} observations: {}",
            serde_json::to_string_pretty(&log)?
        );
        if let Some((expected, rules)) = &expected {
            ensure!(
                *rules == session.rules,
                "normalization rules differ between backends"
            );
            expected.compare(&log)?;
        } else {
            expected = Some((log, session.rules.clone()));
        }
    }
    Ok(())
}
fn ferraloom_kind(options: &LaunchOptions) -> ServerKind {
    options
        .ferraloom_binary
        .clone()
        .map_or(ServerKind::Ferraloom, ServerKind::FerraloomBinary)
}
