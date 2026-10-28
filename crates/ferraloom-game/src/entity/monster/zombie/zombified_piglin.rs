//! ZombifiedPiglin's canonical NeutralMob anger state; live target callbacks stay
//! at the root dispatch boundary, after the UUID is read and the borrow ends.
use super::Zombie;
use crate::{
    entity::{EntityData, EntityRef, EntityReference, LivingEntity, ai::pathfinder::PathType},
    level::{Level, LevelError},
};
use ferraloom_nbt::{CompoundTag, Tag};

/// Port of ZombifiedPiglin's subclass fields. Timers consume no constructor RNG.
#[derive(Debug)]
pub struct ZombifiedPiglin {
    pub parent: Zombie,
    pub play_first_anger_sound_in: i32,
    pub persistent_anger_end_time: i64,
    pub persistent_anger_target: Option<EntityReference<LivingEntity>>,
    pub ticks_until_next_alert: i32,
}
impl ZombifiedPiglin {
    /// Port of ZombifiedPiglin#ZombifiedPiglin.
    pub fn with_level(parent: EntityData, level: &Level) -> Result<Self, LevelError> {
        let mut parent = Zombie::with_level(parent, level)?;
        parent.as_mob_mut().pathfinding_malus[PathType::Lava.id() as usize] = Some(8.0);
        Ok(Self {
            parent,
            play_first_anger_sound_in: 0,
            persistent_anger_end_time: 0,
            persistent_anger_target: None,
            ticks_until_next_alert: 0,
        })
    }
    /// Port of NeutralMob#readPersistentAngerSaveData's end-time prefix. The
    /// caller reads the actual dimension game time for legacy AngerTime only.
    pub fn read_anger_end_time(&mut self, tag: &CompoundTag, game_time: i64) {
        self.persistent_anger_end_time = tag.get_long("anger_end_time").unwrap_or_else(|| {
            tag.get_int("AngerTime")
                .map_or(-1, |remaining| game_time.wrapping_add(i64::from(remaining)))
        });
    }
    /// Port of NeutralMob#readPersistentAngerSaveData's ServerLevel reference
    /// step. The live caller next resolves this reference, ends the borrow, and
    /// invokes virtual setTarget before returning from the load.
    pub fn read_anger_target(&mut self, tag: &CompoundTag) {
        self.persistent_anger_target =
            crate::entity::living::damage_state::read_reference(tag, "angry_at");
    }
    /// Port of NeutralMob#addPersistentAngerSaveData, after Zombie's save tail.
    pub fn add_anger_save_data(
        &self,
        tag: &mut CompoundTag,
        level: &Level,
    ) -> Result<(), LevelError> {
        tag.insert("anger_end_time", Tag::Long(self.persistent_anger_end_time));
        tag.remove("angry_at");
        if let Some(reference) = self.persistent_anger_target {
            tag.insert(
                "angry_at",
                Tag::IntArray(reference.uuid(level)?.to_int_array().to_vec()),
            );
        }
        Ok(())
    }
    /// Trace the retained native EntityReference cache, including removed actors.
    pub fn trace_anger_entity_refs(&self, references: &mut Vec<EntityRef>) {
        references.extend(
            self.persistent_anger_target
                .and_then(|reference| reference.cached_entity()),
        );
    }
}
