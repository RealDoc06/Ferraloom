//! Agent control as a setting (Horizon Settings > AI Agent): when the control socket is open, who
//! is connected, and the details an agent needs. The socket opens when the game was started for
//! an agent (`--control-socket`: the `horizon-server` supervisor) or
//! when the player turned Agent Control on, and never under No AI (`horizon_core::no_ai`).
//! Turning either switch in the game opens or closes it at once.
use super::Game;
use horizon_client::{Result, control};
use std::path::PathBuf;

impl Game<'_> {
    /// The endpoint: the one the command line gave, else `control::default_path`.
    fn agent_socket_path(&self) -> PathBuf {
        self.options
            .control
            .clone()
            .unwrap_or_else(control::default_path)
    }

    /// At startup: open the socket if the game was started for an agent or Agent Control is on.
    pub(super) fn start_agent_control(&mut self) -> Result<()> {
        self.agent_control_on = self.settings.extras.agent_control;
        if self.settings.extras.no_ai {
            if self.options.control.is_some() {
                log::warn!(
                    "Horizon: control socket not opened: {}",
                    horizon_core::no_ai::REFUSAL
                );
            }
            return Ok(());
        }
        if let Some(path) = &self.options.control {
            // Asked for on the command line: failing to listen is an error.
            let server = control::Server::start(path)
                .map_err(|e| format!("control socket {}: {e}", path.display()))?;
            log::info!("Horizon: control socket {}", server.path().display());
            self.control = Some(server);
        } else if self.agent_control_on {
            self.open_agent_socket();
        }
        Ok(())
    }

    /// The setting's socket: a failure (another game holds the default endpoint) is logged and
    /// shown on the AI Agent page, not fatal.
    fn open_agent_socket(&mut self) {
        let path = self.agent_socket_path();
        match control::Server::start(&path) {
            Ok(server) => {
                log::info!("Horizon: control socket {}", server.path().display());
                self.control = Some(server);
            }
            Err(e) => log::warn!("Horizon: control socket {}: {e}", path.display()),
        }
    }
}
