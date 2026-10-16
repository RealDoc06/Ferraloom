use std::any::Any;

use crate::entity::player::Player;
use crate::item::{ItemBehaviour, ItemMetadata};
use pumpkin_data::data_component_impl::InstrumentImpl;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::instrument::Instrument;
use pumpkin_data::item::Item;
use pumpkin_data::sound::SoundCategory;
use pumpkin_data::statistic::StatisticCategory;
use pumpkin_util::Hand;

#[cfg(test)]
mod tests;

pub struct GoatHornItem;

impl ItemMetadata for GoatHornItem {
    fn ids() -> Box<[u16]> {
        Box::new([Item::GOAT_HORN.id])
    }
}

impl ItemBehaviour for GoatHornItem {
    fn normal_use_with_hand(
        &self,
        _item: &Item,
        player: &Player,
        _yaw: f32,
        _pitch: f32,
        hand: Hand,
    ) {
        Self::use_in_hand(player, hand);
    }

    fn get_use_duration(&self) -> i32 {
        Instrument::PonderGoatHorn.use_duration_ticks() as i32
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl GoatHornItem {
    pub const USE_DURATION: i32 = Instrument::PonderGoatHorn.use_duration_ticks() as i32;

    /// Resolves the actual hand after cancellable events. Only successful horn
    /// use owns its statistic and cooldown; the generic item path skips both.
    pub(crate) fn use_in_hand(player: &Player, hand: Hand) -> bool {
        let stack = player.inventory().get_stack_in_hand(hand);
        if player.is_spectator() || stack.is_empty() || stack.item.id != Item::GOAT_HORN.id {
            return false;
        }
        let Some(component) = stack.get_data_component::<InstrumentImpl>() else {
            return false;
        };
        let instrument = component.instrument;
        let cooldown_group = stack
            .get_use_cooldown()
            .and_then(|cooldown| cooldown.cooldown_group.clone())
            .unwrap_or_else(|| "minecraft:goat_horn".into());
        if player.is_on_cooldown(&cooldown_group) {
            return false;
        }

        let duration = (instrument.use_duration_seconds() * 20.0).floor() as i32;
        player.living_entity.set_active_hand(hand, stack, duration);
        let world = player.world();
        let position = player.position();
        world.play_sound_fine(
            instrument.sound(),
            SoundCategory::Records,
            &position,
            instrument.range() / 16.0,
            1.0,
        );
        // This is the existing game-event hook, not a vibration implementation.
        world.emit_game_event(GameEvent::InstrumentPlay.name(), position);
        // All eight supported builtins have durability_damage = 0. Inline
        // instruments with nonzero damage remain unsupported by the codec.
        if instrument.use_duration_seconds() > 0.0 {
            player.start_cooldown(cooldown_group, duration);
        }
        player.increment_stat(StatisticCategory::Used, i32::from(Item::GOAT_HORN.id), 1);
        true
    }
}
