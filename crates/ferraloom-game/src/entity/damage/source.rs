use super::{DamageScaling, DamageType, DamageTypeHolder, DamageTypeKey, DamageTypeTag};
use crate::{
    entity::{EntityData, EntityRef, EntityState},
    level::{Level, LevelError},
};
use ferraloom_util::Vec3;

/// Port of DamageSource. Entity handles refer to the actual direct and causing
/// objects, including removed objects still retained by an owning entity.
#[derive(Clone, Debug)]
pub struct DamageSource {
    holder: DamageTypeHolder,
    direct_entity: Option<EntityRef>,
    causing_entity: Option<EntityRef>,
    position: Option<Vec3>,
}
impl DamageSource {
    pub fn new(holder: DamageTypeHolder) -> Self {
        Self::with_entities(holder, None, None)
    }
    pub fn with_entities(
        holder: DamageTypeHolder,
        direct_entity: Option<EntityRef>,
        causing_entity: Option<EntityRef>,
    ) -> Self {
        Self {
            holder,
            direct_entity,
            causing_entity,
            position: None,
        }
    }
    pub fn with_cause(holder: DamageTypeHolder, cause: Option<EntityRef>) -> Self {
        Self::with_entities(holder, cause, cause)
    }
    pub fn at_position(holder: DamageTypeHolder, position: Vec3) -> Self {
        Self {
            position: Some(position),
            ..Self::new(holder)
        }
    }
    pub fn damage_type(&self) -> &DamageType {
        self.holder.value()
    }
    pub fn type_holder(&self) -> &DamageTypeHolder {
        &self.holder
    }
    pub fn is_type(&self, key: DamageTypeKey) -> bool {
        self.holder.is_type(key)
    }
    pub fn is_tag(&self, tag: DamageTypeTag) -> bool {
        self.holder.is_tag(tag)
    }
    pub fn direct_entity(&self) -> Option<EntityRef> {
        self.direct_entity
    }
    pub fn causing_entity(&self) -> Option<EntityRef> {
        self.causing_entity
    }
    /// Port of DamageSource#getWeaponItem: resolve the direct entity's virtual
    /// main-hand read at the call, independently of the causing entity.
    pub fn weapon_item<'level>(
        &self,
        level: &'level Level,
    ) -> Result<Option<&'level ferraloom_protocol::component::ItemStack>, LevelError> {
        match self.direct_entity {
            Some(entity) => level.entity_behavior(entity)?.weapon_item(entity, level),
            None => Ok(None),
        }
    }
    pub fn is_direct(&self) -> bool {
        self.direct_entity == self.causing_entity
    }
    pub fn food_exhaustion(&self) -> f32 {
        self.damage_type().exhaustion
    }
    pub fn source_position_raw(&self) -> Option<Vec3> {
        self.position
    }
    /// Port of DamageSource#getSourcePosition: a direct source's position is
    /// read at the call, while an explicit position stays captured.
    pub fn source_position(&self, level: &Level) -> Result<Option<Vec3>, LevelError> {
        if self.position.is_some() {
            Ok(self.position)
        } else {
            self.direct_entity
                .map(|id| level.entity::<EntityData>(id).map(|data| data.position))
                .transpose()
        }
    }
    pub fn scales_with_difficulty(&self, level: &Level) -> Result<bool, LevelError> {
        Ok(match self.damage_type().scaling {
            DamageScaling::Never => false,
            DamageScaling::Always => true,
            DamageScaling::WhenCausedByLivingNonPlayer => match self.causing_entity {
                None => false,
                Some(id) => {
                    let state = level.entity_state(id)?;
                    state.as_living().is_some() && !matches!(state, EntityState::Player(_))
                }
            },
        })
    }
    pub fn is_creative_player(&self, level: &Level) -> Result<bool, LevelError> {
        Ok(match self.causing_entity {
            Some(id) => match level.entity_state(id)? {
                EntityState::Player(player) => player.instabuild(),
                _ => false,
            },
            None => false,
        })
    }
    /// Every canonical field holding a DamageSource must include this in its
    /// entity reference trace, just as fields holding an EntityRef do.
    pub fn trace_entity_refs(&self, references: &mut Vec<EntityRef>) {
        references.extend(self.direct_entity);
        references.extend(self.causing_entity);
    }
    /// Explicit immutable predicate DTO boundary; it does not become canonical
    /// entity damage state or snapshot the referenced entities themselves.
    pub fn to_loot_source(&self) -> crate::loot::DamageSource {
        crate::loot::DamageSource {
            damage_type: self
                .holder
                .name()
                .to_string()
                .parse()
                .expect("validated damage identifier"),
            source: self.causing_entity,
            direct: self.direct_entity,
            position: self.position,
        }
    }
}
