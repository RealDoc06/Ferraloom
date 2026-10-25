//! DataAccessor's canonical live receiver and saved-data adapters.
use super::*;
use ferraloom_game::{
    block::entity::BlockEntityRef,
    commands::{
        data::{self, DataAccess, DataAccessor, DataError},
        source::EntityAnchor,
    },
    entity::{EntityData, EntityState},
};
use ferraloom_nbt::CompoundTag;

struct DataHost<'a> {
    server: &'a mut MinecraftServer,
    roots: Vec<BlockEntityRef>,
}
impl Drop for DataHost<'_> {
    fn drop(&mut self) {
        for handle in self.roots.drain(..) {
            self.server.level.level.release_block_entity(handle);
        }
    }
}
impl DataAccess for DataHost<'_> {
    fn resolve(
        &mut self,
        command: &PreparedCommand,
        kind: &str,
        argument: &str,
    ) -> Result<DataAccessor, DataError> {
        Ok(match kind {
            "storage" => DataAccessor::Storage(protocol::Identifier::parse(
                command.string(argument).expect("storage identifier"),
            )?),
            "entity" => {
                let entities = self.server.command_entities(command, argument)?;
                if entities.len() != 1 {
                    return Err(exceptions::error("argument.entity.toomany", &[]).into());
                }
                DataAccessor::Entity(entities[0])
            }
            "block" => {
                let coordinates = command.coordinates(argument).expect("block coordinates");
                let eye_height = if matches!(
                    coordinates,
                    ferraloom_game::commands::arguments::coordinates::Coordinates::Local { .. }
                ) && command.source.anchor == EntityAnchor::Eyes
                {
                    command
                        .source
                        .entity
                        .map(|entity| {
                            self.server
                                .level
                                .level
                                .entity::<EntityData>(entity)
                                .map(|state| f64::from(state.eye_height))
                        })
                        .transpose()?
                } else {
                    None
                };
                let position = coordinates.position(&command.source, eye_height);
                let pos = BlockPos::containing(position.x, position.y, position.z);
                let level = &mut self.server.level.level;
                if !level.has_chunk_at(pos) {
                    return Err(exceptions::error("argument.pos.unloaded", &[]).into());
                }
                if !level.is_in_valid_bounds(pos) {
                    return Err(exceptions::error("argument.pos.outofworld", &[]).into());
                }
                let entity = level
                    .block_entity_ref(pos)?
                    .ok_or_else(|| exceptions::error("commands.data.block.invalid", &[]))?;
                level.retain_block_entity(entity)?;
                self.roots.push(entity);
                DataAccessor::Block { entity, pos }
            }
            _ => unreachable!("data provider"),
        })
    }
    fn get_data(&mut self, accessor: &DataAccessor) -> Result<CompoundTag, DataError> {
        Ok(match accessor {
            DataAccessor::Storage(id) => self.server.command_storage.get(id)?,
            DataAccessor::Entity(entity) => {
                self.server.level.level.entity_nbt_to_compare(*entity)?
            }
            DataAccessor::Block { entity, .. } => self
                .server
                .level
                .level
                .block_entity_state(*entity)?
                .save_with_full_metadata()?,
        })
    }
    fn set_data(&mut self, accessor: &DataAccessor, value: CompoundTag) -> Result<(), DataError> {
        match accessor {
            DataAccessor::Storage(id) => {
                self.server.command_storage.set(id, value)?;
                self.server
                    .level
                    .level
                    .set_command_storage(self.server.command_storage.snapshot());
            }
            DataAccessor::Block { entity, pos } => {
                // BlockDataAccessor captures this state before either load callback.
                let state = self.server.level.level.get_block_state(*pos)?;
                entity.load_with_components(&value, &mut self.server.level.level)?;
                entity.set_changed(&mut self.server.level.level)?;
                self.server
                    .level
                    .level
                    .send_block_updated(*pos, state, state);
            }
            DataAccessor::Entity(entity) => {
                if matches!(
                    self.server.level.level.entity_state(*entity)?,
                    EntityState::Player(_)
                ) {
                    return Err(exceptions::error("commands.data.entity.invalid", &[]).into());
                }
                // Full subclass Entity#load/postDataManipulated belongs to entity
                // core; base-only EntityData decoding cannot substitute for it.
                return Err(exceptions::error(
                    "ferraloom.command.pending",
                    &["data entity mutation".into()],
                )
                .into());
            }
        }
        Ok(())
    }
    fn publish_read_mutation(
        &mut self,
        accessor: &DataAccessor,
        value: &CompoundTag,
    ) -> Result<(), DataError> {
        if let DataAccessor::Storage(id) = accessor {
            self.server
                .command_storage
                .publish_read_mutation(id, value)?;
            self.server
                .level
                .level
                .set_command_storage(self.server.command_storage.snapshot());
        }
        Ok(())
    }
    fn display_name(&self, entity: EntityRef) -> Result<Component, DataError> {
        Ok(self
            .server
            .level
            .level
            .entity_display_name_with_scoreboard(
                entity,
                self.server.shared().scoreboard.scoreboard(),
            )?)
    }
}
impl MinecraftServer {
    pub(in crate::server) fn data_command(
        &mut self,
        command: &PreparedCommand,
    ) -> Result<CommandResult> {
        let result = data::execute(
            command,
            &mut DataHost {
                server: self,
                roots: Vec::new(),
            },
        );
        // Partial modifications and load callbacks publish their actual ordered
        // effects before syntax feedback, just as every other live command does.
        self.apply_level_effects()?;
        result.map_err(|error| match error {
            DataError::Syntax(error) => anyhow::Error::new(error),
            error => anyhow::Error::new(error),
        })
    }
}
