//! Actual protochunk receivers share the canonical game states and codecs.
//! No world/level callback occurs while these level-less objects are borrowed.
use super::BlockEntityState;
use ferraloom_data::EntityType;
use ferraloom_nbt::CompoundTag;
use ferraloom_util::random::WorldgenRandom;
use ferraloom_worldgen::{Error, Identifier, region::WorldgenBlockEntity};

impl WorldgenBlockEntity for BlockEntityState {
    fn clone_state(&self) -> Box<dyn WorldgenBlockEntity> {
        Box::new(self.clone())
    }
    fn save(&self) -> Result<CompoundTag, Error> {
        self.save_with_full_metadata()
            .map_err(|error| Error(format!("block entity save: {error}")))
    }
    fn is_randomizable(&self) -> bool {
        self.randomizable().is_some()
    }
    fn set_loot_table(&mut self, table: &Identifier, seed: i64) -> Result<(), Error> {
        if let Some(container) = self.randomizable_mut() {
            container.loot_table = Some(
                ferraloom_protocol::Identifier::parse(&table.to_string())
                    .map_err(|error| Error(format!("loot table: {error}")))?,
            );
            container.loot_table_seed = seed;
        }
        Ok(())
    }
    fn set_entity_id(
        &mut self,
        kind: EntityType,
        random: &mut WorldgenRandom,
    ) -> Result<(), Error> {
        if let Self::Spawner(entity) = self {
            entity.spawner.set_entity_id(kind, random);
        }
        Ok(())
    }
    fn populate_beehive(&mut self, random: &mut WorldgenRandom) -> Result<(), Error> {
        if let Self::Beehive(entity) = self {
            entity.populate_worldgen(random);
        }
        Ok(())
    }
    fn load_with_components(&mut self, tag: &CompoundTag) -> Result<(), Error> {
        BlockEntityState::load_with_components(self, tag);
        Ok(())
    }
}
