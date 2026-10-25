//! SummonCommand's exclusive adapter over the one canonical entity loader.
use super::*;
use ferraloom_game::{
    commands::summon::{self, SummonAccess, SummonError},
    entity::{self, EntityData, EntityPersistenceError, EntitySpawnReason, EntitySpawnRequest},
    level::Level,
};
use ferraloom_nbt::CompoundTag;

struct SummonHost<'a> {
    level: &'a mut Level,
    root: Option<EntityRef>,
}
impl Drop for SummonHost<'_> {
    fn drop(&mut self) {
        if let Some(root) = self.root.take() {
            self.level.release_entity(root);
        }
    }
}
impl SummonAccess for SummonHost<'_> {
    type Loaded = EntityRef;
    type Error = EntityPersistenceError;
    fn eye_height(&self, entity: EntityRef) -> Result<f64, Self::Error> {
        Ok(f64::from(
            self.level.entity::<EntityData>(entity)?.eye_height,
        ))
    }
    fn difficulty(&self) -> protocol::play::Difficulty {
        self.level.difficulty()
    }
    fn allowed_in_peaceful(&self, kind: ferraloom_data::EntityType) -> bool {
        kind.data().allowed_in_peaceful
    }
    fn load_entity_recursive(
        &mut self,
        tag: CompoundTag,
        position: Vec3,
    ) -> Result<Option<EntityRef>, Self::Error> {
        let root = entity::load_entity_recursive(
            &tag,
            EntitySpawnRequest {
                reason: EntitySpawnReason::Command,
                ignore_checks: false,
            },
            self.level,
            |entity, level| {
                // The loader invokes this on the retained, unpublished receiver
                // before its passengers: EntityType.loadEntityRecursive's hook.
                let base = level.entity_mut::<EntityData>(entity)?;
                base.snap_to(position, base.yaw, base.pitch);
                Ok(())
            },
        )?;
        self.root = root;
        Ok(root)
    }
    fn is_mob(&self, entity: &EntityRef) -> Result<bool, Self::Error> {
        Ok(self.level.entity_state(*entity)?.as_mob().is_some())
    }
    fn finalize_spawn(&mut self, entity: &mut EntityRef) -> Result<(), Self::Error> {
        entity::mob::finalize_spawn(*entity, EntitySpawnReason::Command, None, self.level)?;
        Ok(())
    }
    fn try_add_fresh_entity_with_passengers(
        &mut self,
        entity: EntityRef,
    ) -> Result<Option<EntityRef>, Self::Error> {
        Ok(self
            .level
            .try_add_fresh_entity_with_passengers(entity)?
            .then_some(entity))
    }
    fn display_name(&self, entity: EntityRef) -> Result<Component, Self::Error> {
        // Native can report success for a removed empty ItemEntity. The loader's
        // root retain keeps that receiver readable until feedback is constructed.
        Ok(self.level.entity_display_name_with_scoreboard(
            entity,
            self.level.shared()?.scoreboard.scoreboard(),
        )?)
    }
}
impl MinecraftServer {
    pub(in crate::server) fn summon_command(
        &mut self,
        command: &PreparedCommand,
    ) -> Result<CommandResult> {
        let result = summon::run(
            command,
            &mut SummonHost {
                level: &mut self.level.level,
                root: None,
            },
        );
        self.apply_level_effects()?;
        result.map_err(|error| match error {
            SummonError::Syntax(error) => anyhow::Error::new(error),
            SummonError::World(error) => anyhow::Error::new(error),
        })
    }
}
