//! The persisted Cow parent chain. AI, breeding and variant-dependent behavior
//! remain separate from these canonical scalar/reference and metadata reads.
use crate::{
    entity::{
        Entity, EntityData, EntityMethod, EntityRef, LivingEntity,
        animal::Cow,
        mob::{LeashAttachment, LeashData},
        player::ServerPlayer,
        synched_data,
    },
    level::{Level, LevelError},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Identifier,
    component::{CowSoundVariant, CowVariant},
};
use ferraloom_util::Vec3;

pub(crate) fn baby_dimensions() -> crate::entity::EntityDimensions {
    use crate::entity::{EntityAttachments, EntityDimensions};
    EntityDimensions::scalable(0.45, 0.7)
        .with_eye_height(0.69)
        .with_attachments(EntityAttachments::create_default(0.45, 0.7).with_points(
            ferraloom_data::EntityAttachment::Passenger,
            vec![Vec3::new(0.0, 0.75, 0.0)],
        ))
}
fn variant(
    tag: &CompoundTag,
    field: &str,
    registry: &str,
    level: &Level,
) -> Result<Option<Identifier>, LevelError> {
    let Some(id) = tag
        .get_string(field)
        .and_then(|id| id.as_str().ok()?.parse::<Identifier>().ok())
    else {
        return Ok(None);
    };
    let registry: Identifier = registry.parse().expect("native variant registry");
    Ok(level
        .context()
        .registry_context()?
        .entry_id(&registry, &id)
        .map(|_| id))
}
impl Cow {
    pub fn variant(&self) -> &CowVariant {
        self.as_living().entity_data.get(synched_data::COW_VARIANT)
    }
    pub fn sound_variant(&self) -> &CowSoundVariant {
        self.as_living()
            .entity_data
            .get(synched_data::COW_SOUND_VARIANT)
    }
    pub fn is_age_locked(&self) -> bool {
        *self.as_living().entity_data.get(synched_data::AGE_LOCKED)
    }
    /// Native setAge's server state and metadata update. A live caller runs
    /// refreshDimensions immediately when this returns a boundary transition.
    pub(crate) fn read_age(&mut self, tag: &CompoundTag) -> bool {
        let age = tag.get_int_or("Age", 0);
        let was_baby = self.parent.parent.parent.age < 0;
        self.parent.parent.parent.age = age;
        let changed = was_baby != (age < 0);
        if changed {
            self.as_living_mut()
                .entity_data
                .set(synched_data::BABY, age < 0);
        }
        changed
    }
    pub(crate) fn read_age_and_animal_tail(&mut self, tag: &CompoundTag) {
        self.parent.parent.parent.forced_age = tag.get_int_or("ForcedAge", 0);
        self.as_living_mut().entity_data.set(
            synched_data::AGE_LOCKED,
            tag.get_boolean_or("AgeLocked", false),
        );
        self.parent.parent.in_love = tag.get_int_or("InLove", 0);
        self.parent.parent.love_cause =
            crate::entity::living::damage_state::read_reference::<ServerPlayer>(tag, "LoveCause");
    }
    pub(crate) fn read_variants(
        &mut self,
        tag: &CompoundTag,
        level: &Level,
    ) -> Result<(), LevelError> {
        if let Some(variant) = variant(tag, "variant", "minecraft:cow_variant", level)? {
            self.as_living_mut()
                .entity_data
                .set(synched_data::COW_VARIANT, CowVariant(variant));
        }
        if let Some(variant) = variant(tag, "sound_variant", "minecraft:cow_sound_variant", level)?
        {
            self.as_living_mut()
                .entity_data
                .set(synched_data::COW_SOUND_VARIANT, CowSoundVariant(variant));
        }
        Ok(())
    }
    pub(crate) fn read_cold_data(
        &mut self,
        tag: &CompoundTag,
        level: &Level,
    ) -> Result<(), LevelError> {
        let mob = self.as_mob_mut();
        mob.read_persistent_fields(tag);
        mob.leash_data = LeashData::read(tag.get("leash"));
        mob.read_persistent_tail(tag);
        if self.read_age(tag) {
            let scale = self
                .as_living()
                .attributes
                .value(ferraloom_data::registries::attribute::Id::SCALE)
                .expect("living scale") as f32;
            let dimensions = if self.parent.parent.parent.age < 0 {
                baby_dimensions()
            } else {
                crate::entity::EntityDimensions::for_type(ferraloom_data::EntityType::COW)
            }
            .scale(scale, scale);
            let base = &mut self.as_living_mut().parent;
            base.eye_height = dimensions.eye_height;
            base.dimensions = dimensions;
            base.set_pos(base.position);
        }
        self.read_age_and_animal_tail(tag);
        self.read_variants(tag, level)
    }
    pub(crate) fn add_cow_save_data(
        &self,
        tag: &mut CompoundTag,
        level: &Level,
    ) -> Result<(), LevelError> {
        let mob = self.as_mob();
        for key in [
            "NoAI",
            "drop_chances",
            "leash",
            "home_radius",
            "home_pos",
            "DeathLootTable",
            "DeathLootTableSeed",
            "LoveCause",
        ] {
            tag.remove(key);
        }
        tag.insert("CanPickUpLoot", Tag::Byte(i8::from(mob.can_pick_up_loot)));
        tag.insert(
            "PersistenceRequired",
            Tag::Byte(i8::from(mob.persistence_required)),
        );
        tag.insert("LeftHanded", Tag::Byte(i8::from(mob.is_left_handed())));
        if mob.is_no_ai() {
            tag.insert("NoAI", Tag::Byte(1));
        }
        let chances = mob.drop_chances.save();
        if chances
            .as_compound()
            .is_some_and(|chances| !chances.is_empty())
        {
            tag.insert("drop_chances", chances);
        }
        if let Some(leash) = &mob.leash_data {
            let attachment = if let Some(holder) = leash.holder {
                Some(LeashAttachment::Entity(
                    level.entity::<EntityData>(holder)?.uuid,
                ))
            } else {
                leash.delayed.clone()
            };
            if let Some(attachment) = attachment {
                let value = match attachment {
                    LeashAttachment::Entity(uuid) => {
                        let mut data = CompoundTag::new();
                        data.insert("UUID", Tag::IntArray(uuid.to_int_array().to_vec()));
                        Tag::Compound(data)
                    }
                    LeashAttachment::Fence(pos) => Tag::IntArray(vec![pos.x, pos.y, pos.z]),
                };
                tag.insert("leash", value);
            }
        }
        if mob.home_radius != -1 {
            tag.insert("home_radius", Tag::Int(mob.home_radius));
            tag.insert(
                "home_pos",
                Tag::IntArray(vec![
                    mob.home_position.x,
                    mob.home_position.y,
                    mob.home_position.z,
                ]),
            );
        }
        if let Some(table) = &mob.loot_table {
            tag.insert("DeathLootTable", Tag::String(table.to_string().into()));
        }
        if mob.loot_table_seed != 0 {
            tag.insert("DeathLootTableSeed", Tag::Long(mob.loot_table_seed));
        }
        tag.insert("Age", Tag::Int(self.parent.parent.parent.age));
        tag.insert("ForcedAge", Tag::Int(self.parent.parent.parent.forced_age));
        tag.insert("AgeLocked", Tag::Byte(i8::from(self.is_age_locked())));
        tag.insert("InLove", Tag::Int(self.parent.parent.in_love));
        if let Some(cause) = self.parent.parent.love_cause {
            tag.insert(
                "LoveCause",
                Tag::IntArray(cause.uuid(level)?.to_int_array().to_vec()),
            );
        }
        tag.insert("variant", Tag::String(self.variant().0.to_string().into()));
        tag.insert(
            "sound_variant",
            Tag::String(self.sound_variant().0.to_string().into()),
        );
        Ok(())
    }
}
/// Port of Mob through Cow readAdditionalSaveData after LivingEntity. Leash
/// removal publishes its packet before the former holder's notification.
pub(crate) fn read_cow_data<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    tag: &CompoundTag,
    level: &mut Level,
) -> Result<(), crate::entity::EntityPersistenceError> {
    level
        .entity_mut::<Cow>(entity)?
        .as_mob_mut()
        .read_persistent_fields(tag);
    let next = LeashData::read(tag.get("leash"));
    let previous = level
        .entity::<Cow>(entity)?
        .as_mob()
        .leash_data
        .as_ref()
        .and_then(|data| data.holder);
    if next.is_none()
        && let Some(holder) = previous
    {
        level.retain_entity(holder)?;
        let result = (|| -> Result<(), LevelError> {
            let mob = level.entity_mut::<Cow>(entity)?.as_mob_mut();
            mob.leash_data = None;
            mob.home_radius = -1; // Mob#onLeashRemoved -> clearHome, position retained.
            let packet = ferraloom_protocol::play::clientbound::Packet::SetEntityLink(
                ferraloom_protocol::play::clientbound::SetEntityLink {
                    source_id: level.entity::<EntityData>(entity)?.network_id,
                    dest_id: 0,
                },
            );
            level
                .broadcast_tracked_packet(
                    entity,
                    crate::entity::tracking::TrackingPacket {
                        audience: crate::entity::tracking::TrackingAudience::Players,
                        packet,
                    },
                )
                .map_err(|error| error.into_level(level))?;
            level
                .entity_behavior(holder)?
                .notify_leashee_removed(holder, entity, level)
        })();
        level.release_entity(holder);
        result?;
    }
    let mob = level.entity_mut::<Cow>(entity)?.as_mob_mut();
    mob.leash_data = next;
    mob.read_persistent_tail(tag);
    let changed = level.entity_mut::<Cow>(entity)?.read_age(tag);
    if changed {
        crate::entity::dimensions::refresh_dimensions(behavior, entity, level)?;
    }
    level
        .entity_mut::<Cow>(entity)?
        .read_age_and_animal_tail(tag);
    // Decode the bound registry facts without holding the canonical receiver.
    let selected = variant(tag, "variant", "minecraft:cow_variant", level)?;
    if let Some(variant) = selected {
        level
            .entity_mut::<LivingEntity>(entity)?
            .entity_data
            .set(synched_data::COW_VARIANT, CowVariant(variant));
    }
    let sound = variant(tag, "sound_variant", "minecraft:cow_sound_variant", level)?;
    if let Some(sound) = sound {
        level
            .entity_mut::<LivingEntity>(entity)?
            .entity_data
            .set(synched_data::COW_SOUND_VARIANT, CowSoundVariant(sound));
    }
    Ok(())
}
/// Until CowSoundVariant behavior is loaded by the mob family, using a newly
/// loaded non-classic sound set is a required error instead of a classic fallback.
pub(crate) fn require_classic_sound(entity: EntityRef, level: &Level) -> Result<(), LevelError> {
    if level.entity::<Cow>(entity)?.sound_variant().0.as_str() != "minecraft:classic" {
        return Err(LevelError::UnsupportedEntityMethod(
            ferraloom_data::EntityType::COW,
            EntityMethod::CowVariantBehavior,
        ));
    }
    Ok(())
}
