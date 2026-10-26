//! Owned player IO continuations. The owner polls them without suspending its
//! packet/tick loop; explicit save/shutdown boundaries may await their completion.
use super::*;

pub(super) struct PlayerIo<T> {
    reply: tokio::sync::oneshot::Receiver<Result<T>>,
}
impl<T: Send + 'static> PlayerIo<T> {
    pub(super) fn start(work: impl FnOnce() -> Result<T> + Send + 'static) -> Self {
        let (send, reply) = tokio::sync::oneshot::channel();
        tokio::task::spawn_blocking(move || {
            let _ = send.send(work());
        });
        Self { reply }
    }
    pub(super) fn poll(&mut self) -> Result<Option<T>> {
        match self.reply.try_recv() {
            Ok(result) => result.map(Some),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => Ok(None),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                bail!("player IO worker stopped before completion")
            }
        }
    }
    pub(super) async fn wait(&mut self) -> Result<T> {
        (&mut self.reply)
            .await
            .context("player IO worker stopped before completion")?
    }
}

/// Ready#spawn has allocated this player after waitForEntities. Its original
/// constructor receiver stays owned until NBT/progress IO completes and join runs.
pub(super) struct PlayerLoad {
    pub player: Box<ServerPlayer>,
    pub stage: PlayerLoadStage,
}

pub(super) enum PlayerLoadStage {
    Nbt(PlayerIo<Option<ferraloom_nbt::CompoundTag>>),
    Progress(PlayerIo<ferraloom_game::advancements::LoadedPlayerProgress>),
}

pub(super) enum PlayerLoadResult {
    Nbt(Option<ferraloom_nbt::CompoundTag>),
    Progress(Box<ferraloom_game::advancements::LoadedPlayerProgress>),
}
impl PlayerLoad {
    pub(super) fn poll(&mut self) -> Result<Option<PlayerLoadResult>> {
        match &mut self.stage {
            PlayerLoadStage::Nbt(io) => Ok(io.poll()?.map(PlayerLoadResult::Nbt)),
            PlayerLoadStage::Progress(io) => Ok(io
                .poll()?
                .map(|progress| PlayerLoadResult::Progress(Box::new(progress)))),
        }
    }
}

/// PlayerList#remove saves before removal callbacks. Keep the connection and
/// captured callback inputs until that same owned save has completed.
pub(super) struct PlayerSave {
    pub io: Option<PlayerIo<()>>,
    pub name: Component,
    pub boss_player: ferraloom_game::bossbar::BossPlayer,
}
impl PlayerSave {
    pub(super) fn poll(&mut self) -> Result<bool> {
        if let Some(io) = &mut self.io {
            if io.poll()?.is_none() {
                return Ok(false);
            }
            self.io = None;
        }
        Ok(true)
    }
    pub(super) async fn wait(&mut self) -> Result<()> {
        if let Some(io) = &mut self.io {
            io.wait().await?;
            self.io = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_player_io_yields_and_keeps_its_owned_result_or_error() {
        let (send, reply) = tokio::sync::oneshot::channel();
        let mut io = PlayerIo { reply };
        // Other owner work can run between repeated polls of the same operation.
        for _ in 0..4 {
            assert!(io.poll().unwrap().is_none());
        }
        send.send(Ok(String::from("original saved player")))
            .unwrap();
        assert_eq!(io.poll().unwrap().as_deref(), Some("original saved player"));
        assert!(io.poll().is_err());

        let (send, reply) = tokio::sync::oneshot::channel();
        let mut io = PlayerIo::<()> { reply };
        send.send(Err(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied,
        )
        .into()))
            .unwrap();
        assert_eq!(
            io.poll()
                .unwrap_err()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );
    }
}
