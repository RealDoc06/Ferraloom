//! ZombieVillager's own synchronized fields; profession selection uses the entity RNG.
use crate::entity::synched_data::{EntityDataAccessor, SynchedEntityData, SynchedValue};
use ferraloom_protocol::entity_data::{EntityDataValue, VillagerData};

/// Port of ZombieVillager#DATA_CONVERTING_ID.
pub const DATA_CONVERTING_ID: EntityDataAccessor<bool> = EntityDataAccessor::new(19);
/// Port of ZombieVillager#DATA_VILLAGER_DATA.
pub const DATA_VILLAGER_DATA: EntityDataAccessor<VillagerData> = EntityDataAccessor::new(20);
/// Port of ZombieVillager#DATA_VILLAGER_DATA_FINALIZED.
pub const DATA_VILLAGER_DATA_FINALIZED: EntityDataAccessor<bool> = EntityDataAccessor::new(21);

impl SynchedValue for VillagerData {
    fn from_value(value: &EntityDataValue) -> Option<&Self> {
        if let EntityDataValue::VillagerData(value) = value {
            Some(value)
        } else {
            None
        }
    }
    fn from_value_mut(value: &mut EntityDataValue) -> Option<&mut Self> {
        if let EntityDataValue::VillagerData(value) = value {
            Some(value)
        } else {
            None
        }
    }
    fn into_value(self) -> EntityDataValue {
        EntityDataValue::VillagerData(self)
    }
}

/// Port of ZombieVillager#defineSynchedData. The caller supplies the result of
/// initializeZombieVillagerData at the native Entity-constructor RNG draw point.
pub fn define_synched_data(data: &mut SynchedEntityData, initial: VillagerData) {
    data.define(DATA_CONVERTING_ID, false);
    data.define(DATA_VILLAGER_DATA, initial);
    data.define(DATA_VILLAGER_DATA_FINALIZED, false);
}

/// Port of ZombieVillager's own primitive state. MerchantOffers and
/// GossipContainer belong to npc; present-data loads require that later port.
#[derive(Debug)]
pub struct ZombieVillager {
    pub parent: super::Zombie,
    pub villager_conversion_time: i32,
    pub conversion_starter: Option<ferraloom_util::Uuid>,
    pub villager_xp: i32,
}

/// Required NPC codecs, never an opaque saved-tag substitute for canonical data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsupportedNpcData {
    MerchantOffers,
    GossipContainer,
}

/// Synchronous continuation of startConverting's scalar prefix. The root loader
/// must remove Weakness, add Strength, broadcast event 16, THEN read XP.
#[must_use = "finish the native effect and entity-event callbacks before reading XP"]
#[derive(Debug)]
pub struct ConversionStart {
    pub player: Option<ferraloom_util::Uuid>,
    pub time: i32,
}

/// Port of ZombieVillager#initializeZombieVillagerData. The built-in registry's
/// native bootstrap order, including NONE, determines the random profession.
pub fn initialize_zombie_villager_data(
    random: &mut impl ferraloom_util::random::RandomSource,
) -> VillagerData {
    let professions = ferraloom_data::registry_entries("minecraft:villager_profession")
        .expect("extracted built-in villager professions");
    let profession = if professions.is_empty() {
        "minecraft:none"
    } else {
        professions[random.next_int_bound(professions.len() as i32) as usize]
    };
    VillagerData {
        villager_type: ferraloom_protocol::component::VillagerType(
            "minecraft:plains".parse().unwrap(),
        ),
        profession: ferraloom_protocol::component::VillagerProfession(profession.parse().unwrap()),
        level: 1,
    }
}

fn read_holder(
    tag: &ferraloom_nbt::CompoundTag,
    field: &str,
    registry: &str,
    fallback: &str,
) -> ferraloom_protocol::Identifier {
    tag.get_string(field)
        .and_then(|s| {
            s.to_string_checked()
                .ok()?
                .parse::<ferraloom_protocol::Identifier>()
                .ok()
        })
        .filter(|id| {
            ferraloom_data::registry_entries(registry)
                .expect("built-in villager registry")
                .contains(&id.as_str())
        })
        .unwrap_or_else(|| fallback.parse().unwrap())
}

/// Port of VillagerData.CODEC, including field defaults and the minimum level.
pub fn read_villager_data(tag: &ferraloom_nbt::Tag) -> Option<VillagerData> {
    let tag = tag.as_compound()?;
    Some(VillagerData {
        villager_type: ferraloom_protocol::component::VillagerType(read_holder(
            tag,
            "type",
            "minecraft:villager_type",
            "minecraft:plains",
        )),
        profession: ferraloom_protocol::component::VillagerProfession(read_holder(
            tag,
            "profession",
            "minecraft:villager_profession",
            "minecraft:none",
        )),
        level: tag.get_int_or("level", 1).max(1),
    })
}

impl ZombieVillager {
    /// Port of ZombieVillager#ZombieVillager. Profession draws precede Living's
    /// yaw RNG, through Mob's sole early-derived-metadata callback.
    pub fn with_level(
        parent: crate::entity::EntityData,
        level: &crate::level::Level,
    ) -> Result<Self, crate::level::LevelError> {
        let parent = super::Zombie::with_metadata(parent, level, |base, _| {
            base.entity_data.define(DATA_CONVERTING_ID, false);
            let data = initialize_zombie_villager_data(&mut base.random);
            base.entity_data.define(DATA_VILLAGER_DATA, data);
            base.entity_data.define(DATA_VILLAGER_DATA_FINALIZED, false);
            Ok(())
        })?;
        Ok(Self {
            parent,
            villager_conversion_time: 0,
            conversion_starter: None,
            villager_xp: 0,
        })
    }
    pub fn villager_data(&self) -> &VillagerData {
        self.parent.as_living().entity_data.get(DATA_VILLAGER_DATA)
    }
    pub fn villager_data_finalized(&self) -> bool {
        *self
            .parent
            .as_living()
            .entity_data
            .get(DATA_VILLAGER_DATA_FINALIZED)
    }
    pub fn is_converting(&self) -> bool {
        *self.parent.as_living().entity_data.get(DATA_CONVERTING_ID)
    }
    /// Port of readAdditionalSaveData's VillagerData prefix, after Zombie load.
    pub fn read_villager_data_fields(&mut self, tag: &ferraloom_nbt::CompoundTag) {
        let data = tag.get("VillagerData").and_then(read_villager_data);
        if tag.get_boolean_or("VillagerDataFinalized", false) || data.is_some() {
            self.parent
                .as_living_mut()
                .entity_data
                .set(DATA_VILLAGER_DATA_FINALIZED, true);
            let data = data.unwrap_or_else(|| {
                initialize_zombie_villager_data(&mut self.parent.as_living_mut().random)
            });
            self.parent
                .as_living_mut()
                .entity_data
                .set(DATA_VILLAGER_DATA, data);
        }
    }
    /// Native Offers then Gossips reads. Absent fields are null; any present data
    /// requires the NPC codecs before conversion or XP can be read.
    pub fn read_npc_data(
        &mut self,
        tag: &ferraloom_nbt::CompoundTag,
    ) -> Result<(), UnsupportedNpcData> {
        if tag.get("Offers").is_some() {
            return Err(UnsupportedNpcData::MerchantOffers);
        }
        if tag.get("Gossips").is_some() {
            return Err(UnsupportedNpcData::GossipContainer);
        }
        Ok(())
    }
    /// Port of startConverting's primitive prefix, or the nonconverting load
    /// branch. Finish the returned synchronous continuation before read XP.
    pub fn read_conversion_fields(
        &mut self,
        tag: &ferraloom_nbt::CompoundTag,
    ) -> Option<ConversionStart> {
        let time = tag.get_int_or("ConversionTime", -1);
        if time != -1 {
            let player = tag
                .get("ConversionPlayer")
                .and_then(|tag| ferraloom_util::Uuid::from_nbt(tag).ok());
            self.conversion_starter = player;
            self.villager_conversion_time = time;
            self.parent
                .as_living_mut()
                .entity_data
                .set(DATA_CONVERTING_ID, true);
            Some(ConversionStart { player, time })
        } else {
            self.parent
                .as_living_mut()
                .entity_data
                .set(DATA_CONVERTING_ID, false);
            self.villager_conversion_time = -1;
            None
        }
    }
    /// Port of readAdditionalSaveData's final XP read, after conversion callbacks.
    pub fn read_villager_xp(&mut self, tag: &ferraloom_nbt::CompoundTag) {
        self.villager_xp = tag.get_int_or("Xp", 0);
    }
    /// Port of ZombieVillager#addAdditionalSaveData's own tail for absent NPC
    /// collections. Parent saving and its eye-in-water predicate come first.
    pub fn add_villager_save_data(&self, tag: &mut ferraloom_nbt::CompoundTag) {
        use ferraloom_nbt::{CompoundTag, Tag};
        let data = self.villager_data();
        let mut fields = CompoundTag::new();
        fields.insert("type", Tag::String(data.villager_type.0.as_str().into()));
        fields.insert("profession", Tag::String(data.profession.0.as_str().into()));
        fields.insert("level", Tag::Int(data.level));
        tag.insert("VillagerData", Tag::Compound(fields));
        tag.insert(
            "VillagerDataFinalized",
            Tag::Byte(i8::from(self.villager_data_finalized())),
        );
        tag.remove("Offers");
        tag.remove("Gossips");
        tag.insert(
            "ConversionTime",
            Tag::Int(if self.is_converting() {
                self.villager_conversion_time
            } else {
                -1
            }),
        );
        tag.remove("ConversionPlayer");
        if let Some(player) = self.conversion_starter {
            tag.insert(
                "ConversionPlayer",
                Tag::IntArray(player.to_int_array().to_vec()),
            );
        }
        tag.insert("Xp", Tag::Int(self.villager_xp));
    }
}
