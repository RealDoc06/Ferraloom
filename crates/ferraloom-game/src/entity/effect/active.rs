//! LivingEntity's canonical holder-keyed active-effects map.
use super::{MobEffect, MobEffectInstance};
use ferraloom_nbt::{ListTag, Tag};
use ferraloom_protocol::{
    Error,
    component::{self, Persistent},
};
use ferraloom_util::collections::{ReferenceHashMap, ReferenceKey};

#[derive(Clone, Copy, Debug)]
struct MobEffectKey(MobEffect);
impl ReferenceKey for MobEffectKey {
    fn reference_address(&self) -> usize {
        usize::from(self.0.protocol_id()) + 1
    }
}
/// Port of LivingEntity.activeEffects. Holder identity uses our deterministic
/// identity-map order, which is intentionally not a vanilla traversal property.
#[derive(Debug, Default)]
pub struct ActiveEffects {
    effects: ReferenceHashMap<MobEffectKey, MobEffectInstance>,
}
impl ActiveEffects {
    pub fn get(&self, effect: MobEffect) -> Option<&MobEffectInstance> {
        self.effects.get(&MobEffectKey(effect))
    }
    pub fn contains(&self, effect: MobEffect) -> bool {
        self.effects.contains_key(&MobEffectKey(effect))
    }
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }
    pub fn len(&self) -> usize {
        self.effects.len()
    }
    /// Traversal uses our deterministic identity-map order, including saves.
    pub fn values(&self) -> impl Iterator<Item = &MobEffectInstance> {
        self.effects.values()
    }
    /// Port of readAdditionalSaveData's list codec partial result. Invalid entries
    /// are diagnosed while successful entries still reach the holder-keyed map.
    pub(crate) fn load(&mut self, tag: Option<&Tag>) -> Vec<Error> {
        self.effects.clear();
        let Some(tag) = tag else {
            return Vec::new();
        };
        let Tag::List(entries) = tag else {
            return vec![Error::invalid("active effects list", format!("{tag:?}"))];
        };
        let mut errors = Vec::new();
        for entry in &entries.0 {
            match component::MobEffectInstance::from_nbt(entry)
                .and_then(MobEffectInstance::from_component)
            {
                Ok(effect) => {
                    self.effects.insert(MobEffectKey(effect.effect()), effect);
                }
                Err(error) => errors.push(error),
            }
        }
        errors
    }
    /// Port of LivingEntity#addAdditionalSaveData. Save order follows our
    /// deterministic identity-map order, not native holder hash iteration.
    pub(crate) fn save(&self) -> Tag {
        Tag::List(ListTag(
            self.effects
                .values()
                .map(|effect| {
                    effect
                        .to_component()
                        .to_nbt()
                        .expect("canonical mob effect")
                })
                .collect(),
        ))
    }
}
