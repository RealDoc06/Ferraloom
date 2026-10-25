//! ProtoChunk keeps instantiated receivers separately from pending NBT. The
//! game owns each concrete state; this cold boundary exposes only generation's
//! typed operations, without Any/downcasts or intermediate NBT writeback.
use crate::{Error, Identifier};
use ferraloom_data::EntityType;
use ferraloom_nbt::CompoundTag;
use ferraloom_util::random::WorldgenRandom;

pub trait WorldgenBlockEntity: std::fmt::Debug + Send + Sync {
    /// Owned generation inputs may be cloned between jobs. A concrete clone
    /// preserves its internal object references, including SpawnData aliases.
    fn clone_state(&self) -> Box<dyn WorldgenBlockEntity>;
    fn save(&self) -> Result<CompoundTag, Error>;
    fn is_randomizable(&self) -> bool;
    fn set_loot_table(&mut self, table: &Identifier, seed: i64) -> Result<(), Error>;
    fn set_entity_id(&mut self, kind: EntityType, random: &mut WorldgenRandom)
    -> Result<(), Error>;
    fn populate_beehive(&mut self, random: &mut WorldgenRandom) -> Result<(), Error>;
    fn load_with_components(&mut self, tag: &CompoundTag) -> Result<(), Error>;
}
impl Clone for Box<dyn WorldgenBlockEntity> {
    fn clone(&self) -> Self {
        self.clone_state()
    }
}
