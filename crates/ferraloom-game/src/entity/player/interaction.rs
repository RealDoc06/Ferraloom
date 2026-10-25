//! Canonical Player hand, input and interaction-range queries.
use super::{GameType, ServerPlayer};
use crate::entity::ai::attributes::{AttributeModifier, AttributeOperation};
use ferraloom_data::registries::attribute::Id as Attribute;
use ferraloom_protocol::{
    Identifier,
    component::{EquipmentSlot, ItemStack},
    play::InteractionHand,
};
use ferraloom_util::{Aabb, BlockPos, Vec3};
use std::sync::Arc;

// Retained immutable handles reproduce the native static modifier's identity
// across ticks. AttributeInstance compares modifier identity, not its fields.
pub(super) fn range_modifiers() -> [Arc<AttributeModifier>; 2] {
    [
        ("minecraft:creative_mode_block_range", 0.5),
        ("minecraft:creative_mode_entity_range", 2.0),
    ]
    .map(|(id, amount)| {
        Arc::new(AttributeModifier {
            id: Identifier::parse(id).expect("native modifier identifier"),
            amount: amount.into(),
            operation: AttributeOperation::AddValue,
        })
    })
}

/// Port of ServerPlayer#swingAndResetAttackStrength. A rejected restart leaves
/// both tickers unchanged; the tracker send precedes a successful reset.
pub fn swing_and_reset_attack_strength(
    entity: crate::entity::EntityRef,
    hand: InteractionHand,
    animation: ferraloom_protocol::component::SwingAnimation,
    send_to_self: bool,
    level: &mut crate::level::Level,
) -> Result<(), crate::level::LevelError> {
    level.entity::<ServerPlayer>(entity)?;
    if level
        .entity_behavior(entity)?
        .swing(entity, hand, animation, send_to_self, level)?
    {
        level
            .entity_mut::<ServerPlayer>(entity)?
            .reset_attack_strength_ticker();
    }
    Ok(())
}

impl ServerPlayer {
    /// Port of Player#resetAttackStrengthTicker, resetting the swap ticker too.
    pub fn reset_attack_strength_ticker(&mut self) {
        self.attack_strength_ticker = 0;
        self.item_swap_ticker = 0;
    }
    /// Player#tick's native cooldown clock and last-main-hand snapshot slot.
    pub(crate) fn tick_attack_strength(&mut self) {
        self.attack_strength_ticker = self.attack_strength_ticker.wrapping_add(1);
        self.item_swap_ticker = self.item_swap_ticker.wrapping_add(1);
        let current = self.get_main_hand_item();
        if super::inventory::count(&self.last_item_in_main_hand) != super::inventory::count(current)
            || !super::inventory::is_same_item_same_components(
                &self.last_item_in_main_hand,
                current,
            )
        {
            let same_item = self.last_item_in_main_hand.item() == current.item();
            let snapshot = current.copy_with_count(super::inventory::count(current));
            if !same_item {
                self.reset_attack_strength_ticker();
            }
            self.last_item_in_main_hand = snapshot;
        }
    }
    /// Port of Player#hasCorrectToolForDrops, including the native short circuit.
    pub fn has_correct_tool_for_drops(
        &self,
        state: ferraloom_data::BlockState,
        level: &crate::level::Level,
    ) -> Result<bool, crate::level::LevelError> {
        Ok(!state.data().requires_correct_tool
            || super::inventory::is_correct_tool_for_drops(
                self.inventory.selected_item(),
                state,
                level,
            )?)
    }
    /// Port of LivingEntity#getItemInHand with Player's main-hand inventory slot.
    pub fn get_item_in_hand(&self, hand: InteractionHand) -> &ItemStack {
        match hand {
            InteractionHand::MainHand => self.get_main_hand_item(),
            InteractionHand::OffHand => self.get_offhand_item(),
        }
    }
    /// Port of LivingEntity#setItemInHand and Player#setItemSlot.
    pub fn set_item_in_hand(&mut self, hand: InteractionHand, stack: ItemStack) {
        match hand {
            InteractionHand::MainHand => {
                self.inventory.set_selected_item(stack);
            }
            InteractionHand::OffHand => {
                self.equipment.set(EquipmentSlot::Offhand, stack);
            }
        }
    }
    pub fn get_main_hand_item(&self) -> &ItemStack {
        self.inventory.selected_item()
    }
    pub fn get_offhand_item(&self) -> &ItemStack {
        self.equipment.get(EquipmentSlot::Offhand)
    }
    /// Port of Player#isSecondaryUseActive; last input is stored independently.
    pub fn is_secondary_use_active(&self) -> bool {
        self.shared_flag(1)
    }
    pub fn is_text_filtering_enabled(&self) -> bool {
        self.text_filtering_enabled
    }
    pub fn has_client_loaded(&self) -> bool {
        self.loaded
    }
    /// Port of Player#blockInteractionRange.
    pub fn block_interaction_range(&self) -> f64 {
        self.attributes
            .value(Attribute::BLOCK_INTERACTION_RANGE)
            .expect("player attribute supplier defines block interaction range")
    }
    /// Port of Player#entityInteractionRange.
    pub fn entity_interaction_range(&self) -> f64 {
        self.attributes
            .value(Attribute::ENTITY_INTERACTION_RANGE)
            .expect("player attribute supplier defines entity interaction range")
    }
    /// Port of Player#isWithinBlockInteractionRange. The native comparison is strict.
    pub fn is_within_block_interaction_range(&self, pos: BlockPos, buffer: f64) -> bool {
        let range = self.block_interaction_range() + buffer;
        let eye = self.position + Vec3::new(0.0, f64::from(self.eye_height), 0.0);
        let min = Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        Aabb::new(min.x, min.y, min.z, min.x + 1.0, min.y + 1.0, min.z + 1.0).distance_to_sqr(eye)
            < range * range
    }
    /// Port of Player#isWithinEntityInteractionRange(AABB,double).
    pub fn is_within_entity_interaction_range(&self, bounds: Aabb, buffer: f64) -> bool {
        let range = self.entity_interaction_range() + buffer;
        let eye = self.position + Vec3::new(0.0, f64::from(self.eye_height), 0.0);
        bounds.distance_to_sqr(eye) < range * range
    }
    /// Port of ServerPlayer#updatePlayerAttributes's creative range modifiers.
    pub(super) fn update_interaction_range_attributes(&mut self) {
        let creative = self.game_mode == GameType::Creative;
        let modifiers = self.interaction_range_modifiers.clone();
        for (attribute, modifier) in [
            Attribute::BLOCK_INTERACTION_RANGE,
            Attribute::ENTITY_INTERACTION_RANGE,
        ]
        .into_iter()
        .zip(modifiers)
        {
            if let Some(mut instance) = self.attributes.get_instance(attribute) {
                if creative {
                    instance.add_or_update_transient_modifier(modifier);
                } else {
                    instance.remove_modifier(&modifier.id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::player::inventory;
    use ferraloom_data::Item;
    use ferraloom_util::Uuid;
    fn player() -> ServerPlayer {
        ServerPlayer::new(
            Uuid::new(1, 2),
            "interaction".into(),
            1,
            Vec3::new(0.0, 64.0, 0.0),
            GameType::Survival,
        )
    }
    #[test]
    fn instabuild_is_mutable_independent_of_mode_and_persists_native_abilities() {
        let mut actual = Vec::new();
        for mode in [
            GameType::Survival,
            GameType::Creative,
            GameType::Adventure,
            GameType::Spectator,
        ] {
            for flying in [false, true] {
                let mut player = player();
                player.game_mode = mode;
                player.abilities.flying = flying;
                player.update_game_mode_abilities();
                let applied = player.instabuild();
                player.abilities.instabuild = !applied;
                let prevents = player.prevents_block_drops();
                let mut tag = player
                    .save(&crate::entity::tests::persistence_level())
                    .unwrap();
                let saved = tag
                    .get_compound("abilities")
                    .unwrap()
                    .get_boolean("instabuild")
                    .unwrap();
                let packet_flag = player.instabuild();
                tag.remove("recipeBook");
                player.try_load(tag).unwrap();
                actual.push(format!("player-instabuild\t{}\t{flying}\t{applied},{prevents},{saved},{packet_flag},{}",mode.id(),player.instabuild()));
            }
        }
        let expected: Vec<_> = include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("player-instabuild\t"))
            .collect();
        assert_eq!(actual, expected);
    }
    #[test]
    fn native_hands_input_flags_and_strict_eye_to_block_range() {
        let mut player = player();
        player.inventory.set_selected_slot(5);
        player.set_item_in_hand(
            InteractionHand::MainHand,
            ItemStack::new(Item::STONE, 3, Default::default()),
        );
        player.set_item_in_hand(
            InteractionHand::OffHand,
            ItemStack::new(Item::SHIELD, 1, Default::default()),
        );
        let mut actual = Vec::new();
        for shift in [false, true] {
            player.last_client_input.shift = !shift;
            player.set_shared_flag(1, shift);
            actual.push(format!(
                "player-interaction\t{shift}\t{},{},{},{}",
                player.is_secondary_use_active(),
                player.inventory.selected_slot(),
                inventory::count(player.get_item_in_hand(InteractionHand::MainHand)),
                player.get_item_in_hand(InteractionHand::OffHand).item() == Item::SHIELD,
            ));
        }
        for (i, range) in [4.5, 5.0, 5.000000000000001, 0.0, f64::NAN]
            .into_iter()
            .enumerate()
        {
            player
                .attributes
                .get_instance(Attribute::BLOCK_INTERACTION_RANGE)
                .unwrap()
                .set_base_value(range);
            for buffer in [0.0, 1.0, 4.0, -1.0] {
                for pos in [
                    BlockPos::new(5, 65, 0),
                    BlockPos::new(0, 70, 0),
                    BlockPos::new(0, 65, 0),
                ] {
                    actual.push(format!(
                        "player-range\t{i}\t{buffer:.1}\t{},{},{}\t{}",
                        pos.x,
                        pos.y,
                        pos.z,
                        player.is_within_block_interaction_range(pos, buffer)
                    ));
                }
            }
        }
        let expected: Vec<_> = include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| {
                line.starts_with("player-range\t") || line.starts_with("player-interaction\t")
            })
            .collect();
        assert_eq!(actual, expected);
        assert!(!player.is_text_filtering_enabled());
        assert!(!player.has_client_loaded());
        player.text_filtering_enabled = true;
        player.loaded = true;
        assert!(player.is_text_filtering_enabled());
        assert!(player.has_client_loaded());
    }
    #[test]
    fn creative_ranges_keep_modifier_identity_are_transient_and_follow_game_mode() {
        let mut player = player();
        player
            .attributes
            .get_instance(Attribute::BLOCK_INTERACTION_RANGE)
            .unwrap()
            .set_base_value(6.25);
        player
            .attributes
            .get_instance(Attribute::ENTITY_INTERACTION_RANGE)
            .unwrap()
            .set_base_value(7.5);
        player.game_mode = GameType::Creative;
        player.update_interaction_range_attributes();
        assert_eq!(player.block_interaction_range(), 6.75);
        assert_eq!(player.entity_interaction_range(), 9.5);
        player.attributes.clear_attributes_to_sync();
        player.update_interaction_range_attributes();
        assert_eq!(player.attributes.attributes_to_sync().count(), 0);
        for attribute in [
            Attribute::BLOCK_INTERACTION_RANGE,
            Attribute::ENTITY_INTERACTION_RANGE,
        ] {
            assert!(
                player
                    .attributes
                    .get_instance(attribute)
                    .unwrap()
                    .permanent_modifiers()
                    .is_empty()
            );
        }
        for mode in [GameType::Survival, GameType::Adventure, GameType::Spectator] {
            player.game_mode = mode;
            player.update_interaction_range_attributes();
            assert_eq!(player.block_interaction_range(), 6.25);
            assert_eq!(player.entity_interaction_range(), 7.5);
        }
    }
}
