//! Port of BossBarCommands' live getters, component resolver and player sends.
use super::*;
use ferraloom_game::{
    bossbar::{BossPlayer, CustomBossEvents},
    commands::{
        bossbar,
        components::{self, ResolutionContext, ResolutionError},
    },
};

impl MinecraftServer {
    pub(super) fn bossbar_command(
        &mut self,
        command: &PreparedCommand,
    ) -> Result<CommandResult, ferraloom_brigadier::CommandSyntaxException> {
        let result = self.execute_bossbar_command(command);
        self.drain_bossbar_packets();
        result
    }
    fn execute_bossbar_command(
        &mut self,
        command: &PreparedCommand,
    ) -> Result<CommandResult, ferraloom_brigadier::CommandSyntaxException> {
        let verb = command.literals[1].as_str();
        let id = command.argument("id").map(|_| bossbar::id(command));
        if let Some(id) = &id
            && verb != "add"
        {
            bossbar::require(&self.shared().custom_boss_events, id)?;
        }
        let mut resolved = command.clone();
        if let Some(ArgumentValue::Component(name)) = command.argument("name") {
            // ComponentArgument#getResolvedComponent precedes createBar's duplicate check.
            let mut context = ResolutionContext::new(Some(&command.source));
            let name = components::resolve(&mut context, name, self).map_err(component_error)?;
            if verb == "add"
                && self
                    .shared()
                    .custom_boss_events
                    .get(id.as_ref().unwrap())
                    .is_some()
            {
                return Err(exceptions::error(
                    "commands.bossbar.create.failed",
                    &[id.as_ref().unwrap().to_string()],
                ));
            }
            // BossBarCommands#createBar/setName resolve once more with a null entity override.
            let mut context = ResolutionContext::new(Some(&command.source));
            context.default_scoreboard_entity = None;
            let name = components::resolve(&mut context, &name, self).map_err(component_error)?;
            resolved
                .arguments
                .insert("name".into(), ArgumentValue::Component(name));
        }
        let targets = if let Some(ArgumentValue::Entity(selector)) = command.argument("targets") {
            // EntityArgument#getOptionalPlayers permits an empty selector result.
            selector
                .find(&command.source, self, true)?
                .into_iter()
                .map(|entity| {
                    self.boss_player(entity).map_err(|error| {
                        exceptions::error("ferraloom.bossbar.player_read", &[error.to_string()])
                    })
                })
                .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let uuid = if verb == "add" {
            if command.source.level.dimension != self.level.dimension {
                return Err(exceptions::error(
                    "ferraloom.bossbar.source_level.unsupported",
                    std::slice::from_ref(&command.source.level.dimension),
                ));
            }
            Some(Uuid::insecure(self.level.level.random()))
        } else {
            None
        };
        let pending = bossbar::execute(
            &mut self.shared_mut().custom_boss_events,
            &resolved,
            &targets,
            uuid,
        )?;
        pending.resolve_feedback(|entity| {
            self.level
                .level
                .entity_display_name_with_scoreboard(entity, self.shared().scoreboard.scoreboard())
                .map_err(|error| {
                    exceptions::error("ferraloom.bossbar.entity_read", &[format!("{error:?}")])
                })
        })
    }
    pub(super) fn boss_player(&self, entity: EntityRef) -> Result<BossPlayer> {
        let player = self.player(entity)?;
        Ok(BossPlayer {
            entity,
            uuid: player.uuid,
            network_id: player.network_id,
        })
    }
    pub(in crate::server) fn send_bossbar_packet(&mut self, entity: EntityRef, packet: cb::Packet) {
        if let Some(connection) = self
            .connections
            .values_mut()
            .find(|connection| matches!(connection.phase,Phase::Play(player) if player==entity))
        {
            connection.push(packet);
        }
    }
    pub(in crate::server) fn drain_bossbar_packets(&mut self) {
        for (entity, packet) in self.shared_mut().custom_boss_events.drain_packets() {
            self.send_bossbar_packet(entity, packet);
        }
    }
    /// Capture immutable Packed data before IO. Native collectDirtyTagsToSave clears the flag first.
    pub(in crate::server) fn save_custom_boss_events(
        &mut self,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static {
        let storage = SavedDataStorage::new(
            self.level
                .generation()
                .with_world(|world| world.access.path().join("data")),
        );
        let events = &mut self.shared_mut().custom_boss_events;
        let mut packed = SavedData::new(events.pack());
        packed.set_dirty(events.is_dirty());
        events.set_dirty(false);
        async move {
            tokio::task::spawn_blocking(move || {
                storage.save(CustomBossEvents::STORAGE_ID, &mut packed)
            })
            .await??;
            Ok(())
        }
    }
}
fn component_error(error: ResolutionError) -> ferraloom_brigadier::CommandSyntaxException {
    match error {
        ResolutionError::Syntax(error) => error,
        ResolutionError::EntityData(error) | ResolutionError::BlockData(error) => {
            exceptions::error("ferraloom.bossbar.component_read", &[format!("{error:?}")])
        }
        ResolutionError::Serialization(error) => {
            exceptions::error("ferraloom.component.serialization", &[error.to_string()])
        }
        ResolutionError::Unsupported(kind) => {
            exceptions::error("ferraloom.component.unsupported", &[format!("{kind:?}")])
        }
        ResolutionError::Storage(error) => {
            exceptions::error("ferraloom.command_storage.invalid", &[error.to_string()])
        }
        ResolutionError::InvalidContent(message) => {
            exceptions::error("argument.component.invalid", &[message])
        }
    }
}
