//! Headless 26.3 clients and identical, explicitly observed vanilla/Ferraloom scenarios.
mod bot;
mod launch;
mod observation;
mod session;
mod world;

pub use anyhow::{Result, bail, ensure};
pub use bot::{BossEventState, Bot, BotState, Container, Entity, Sequence, Ticks};
pub use ferraloom_protocol as protocol;
pub use launch::{LaunchOptions, Regions, ServerKind, WorldPreset};
pub use observation::{Normalization, ObservationLog};
pub use session::{BotId, RunMode, Session, run};
pub use world::SavedWorld;

/// Declare one opt-in test. FERRALOOM_PARITY_SIDE selects both (default), vanilla or ferraloom.
#[macro_export]
macro_rules! parity_test {
    ($name:ident, $scenario:path) => {
        #[$crate::__private::tokio::test(flavor = "multi_thread", worker_threads = 2)]
        #[ignore = "requires builder vanilla jar/JDK; run with --ignored"]
        async fn $name() -> $crate::Result<()> {
            $crate::run(
                $crate::LaunchOptions::default(),
                $crate::RunMode::from_env()?,
                $scenario,
            )
            .await
        }
    };
}
#[doc(hidden)]
pub mod __private {
    pub use tokio;
}
