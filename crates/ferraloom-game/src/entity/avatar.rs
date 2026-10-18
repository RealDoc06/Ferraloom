use super::{EntityData, EntityDataView, EntityState, LivingEntity, synched_data as data};
use ferraloom_protocol::entity_data::HumanoidArm;

/// Port of Avatar, the immediate embedded parent between Player and LivingEntity.
#[derive(Debug)]
pub struct Avatar {
    pub parent: LivingEntity,
}
impl Avatar {
    pub(crate) fn with_parent(parent: EntityData) -> Self {
        let mut avatar = Self {
            parent: LivingEntity::with_parent(parent),
        };
        avatar
            .entity_data
            .define(data::PLAYER_MAIN_HAND, HumanoidArm::Right);
        avatar
            .entity_data
            .define(data::PLAYER_MODEL_CUSTOMISATION, 0);
        avatar
    }
    pub fn main_arm(&self) -> HumanoidArm {
        *self.entity_data.get(data::PLAYER_MAIN_HAND)
    }
    pub fn set_main_arm(&mut self, arm: HumanoidArm) {
        self.entity_data.set(data::PLAYER_MAIN_HAND, arm);
    }
    pub fn is_model_part_shown(&self, mask: i8) -> bool {
        self.entity_data.get(data::PLAYER_MODEL_CUSTOMISATION) & mask == mask
    }
}
impl std::ops::Deref for Avatar {
    type Target = LivingEntity;
    fn deref(&self) -> &Self::Target {
        &self.parent
    }
}
impl std::ops::DerefMut for Avatar {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.parent
    }
}
impl EntityDataView for Avatar {
    fn view(state: &EntityState) -> Option<&Self> {
        if let EntityState::Player(player) = state {
            Some(&player.parent)
        } else {
            None
        }
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        if let EntityState::Player(player) = state {
            Some(&mut player.parent)
        } else {
            None
        }
    }
}
