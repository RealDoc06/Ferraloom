//! PotentSulfurBlockEntity's canonical countdown and placement reset.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::level::{Level, LevelError};
use ferraloom_data::{Block, properties as p};
use ferraloom_nbt::{CompoundTag, Tag};

/// Port of PotentSulfurBlockEntity. Eruption time is transient; only the waiting
/// countdown is saved, and resetting it does not dirty or notify the level.
#[derive(Clone, Debug)]
pub struct PotentSulfurBlockEntity {
    pub parent: BlockEntityData,
    pub waiting_countdown: i32,
    pub eruption_tick: i64,
}
impl PotentSulfurBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            waiting_countdown: -1,
            eruption_tick: -1,
        }
    }
    /// Port of PotentSulfurBlockEntity#setLevel, after the base level assignment.
    pub fn set_level(&mut self, game_time: i64) {
        self.parent.has_level = true;
        if self.eruption_tick == -1 {
            self.eruption_tick = game_time;
        }
    }
    /// Port of PotentSulfurBlockEntity#resetCountdown.
    pub fn reset_countdown(&mut self) {
        self.waiting_countdown = -1;
    }
    /// Port of PotentSulfurBlock#getTicker's server-side null ticker decision.
    pub fn has_server_ticker(&self) -> bool {
        self.parent.block_state.get(p::POTENT_SULFUR_STATE) != Some(p::PotentSulfurState::Dry)
    }
}
impl BlockEntityRef {
    /// Invoke resetCountdown on this original generation after the caller's
    /// native instanceof check. No receiver is recaptured by position.
    pub fn reset_potent_sulfur_countdown(self, level: &mut Level) -> Result<(), LevelError> {
        level
            .block_entity_mut::<PotentSulfurBlockEntity>(self)?
            .reset_countdown();
        Ok(())
    }
}
pub(super) struct PotentSulfurBehavior;
pub(super) static POTENT_SULFUR_ENTITY: PotentSulfurBehavior = PotentSulfurBehavior;
impl BlockEntity for PotentSulfurBehavior {
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        let BlockEntityState::PotentSulfur(entity) = state else {
            unreachable!()
        };
        if let Some(countdown) = tag.get_int("countdown") {
            entity.waiting_countdown = countdown;
        }
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::PotentSulfur(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        tag.insert("countdown", Tag::Int(entity.waiting_countdown));
        Ok(tag)
    }
    fn tick(&self, _entity: BlockEntityRef, _level: &mut Level) -> Result<(), LevelError> {
        // Non-DRY states require actual nausea/countdown/launch behavior; the
        // native DRY null ticker is excluded before binding, not ticked as a no-op.
        Err(LevelError::UnsupportedBlockEntity(Block::POTENT_SULFUR))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::entity::{self, BlockEntityData, BlockEntityDataView};
    use ferraloom_util::{
        BlockPos,
        random::{LegacyRandomSource, Random, RandomSource},
    };

    const POS: BlockPos = BlockPos::new(8, 5, 8);

    /// Actual native integer coercion, missing/bad-tag preservation, reset,
    /// transient eruption time and state-dependent null ticker selection.
    #[test]
    fn countdown_persistence_reset_and_attachment_match_native() {
        let mut counts = [0; 3];
        let mut level = super::super::placement_tests::bare_level();
        for line in include_str!("../../../tests/java/blocks/entities/potent-sulfur.tsv").lines() {
            let f: Vec<_> = line.split('\t').collect();
            let state = ferraloom_data::BlockState::from_id(f[1].parse().unwrap()).unwrap();
            let mut entity = entity::create(POS, state).unwrap();
            match f[0] {
                "default" => {
                    assert_eq!(
                        entity.save_without_metadata().unwrap(),
                        ferraloom_nbt::parse_compound(f[2]).unwrap(),
                        "{line}"
                    );
                    assert_eq!(
                        PotentSulfurBlockEntity::view(&entity)
                            .unwrap()
                            .eruption_tick
                            .to_string(),
                        f[3]
                    );
                    assert_eq!(entity.has_server_ticker(), f[4] == "true");
                    assert_eq!(
                        entity.get_update_tag().unwrap(),
                        ferraloom_nbt::parse_compound(f[5]).unwrap()
                    );
                    assert_eq!(
                        entity.get_update_packet().unwrap().is_some(),
                        f[6] == "true"
                    );
                    counts[0] += 1;
                }
                "load-reset" => {
                    let receiver = PotentSulfurBlockEntity::view_mut(&mut entity).unwrap();
                    receiver.waiting_countdown = f[2].parse().unwrap();
                    receiver.eruption_tick = 123;
                    entity.load_with_components(&ferraloom_nbt::parse_compound(f[3]).unwrap());
                    assert_eq!(
                        PotentSulfurBlockEntity::view(&entity)
                            .unwrap()
                            .waiting_countdown
                            .to_string(),
                        f[4],
                        "{line}"
                    );
                    assert_eq!(
                        entity.save_without_metadata().unwrap(),
                        ferraloom_nbt::parse_compound(f[5]).unwrap(),
                        "{line}"
                    );
                    level.set_block_state(POS, state).unwrap();
                    level.set_block_entity(entity).unwrap();
                    let handle = level.block_entity_ref(POS).unwrap().unwrap();
                    level.take_effects();
                    level.set_random_streams(Random::Legacy(LegacyRandomSource::new(263777)), 0);
                    handle.reset_potent_sulfur_countdown(&mut level).unwrap();
                    let receiver = level
                        .block_entity::<PotentSulfurBlockEntity>(handle)
                        .unwrap();
                    assert_eq!(receiver.waiting_countdown.to_string(), f[6]);
                    assert_eq!(receiver.eruption_tick.to_string(), f[7]);
                    assert!(level.take_effects().is_empty());
                    assert_eq!(level.random().next_int().to_string(), f[8]);
                    counts[1] += 1;
                }
                "set-level" => {
                    let receiver = PotentSulfurBlockEntity::view_mut(&mut entity).unwrap();
                    receiver.eruption_tick = f[2].parse().unwrap();
                    receiver.waiting_countdown = 7;
                    entity.set_level(f[3].parse().unwrap());
                    assert_eq!(
                        PotentSulfurBlockEntity::view(&entity)
                            .unwrap()
                            .eruption_tick
                            .to_string(),
                        f[5],
                        "{line}"
                    );
                    entity.set_level(f[4].parse().unwrap());
                    let receiver = PotentSulfurBlockEntity::view(&entity).unwrap();
                    assert_eq!(receiver.eruption_tick.to_string(), f[6], "{line}");
                    assert_eq!(receiver.waiting_countdown.to_string(), f[7]);
                    assert_eq!(receiver.parent.has_level, f[8] == "true");
                    counts[2] += 1;
                }
                _ => unreachable!(),
            }
        }
        assert_eq!(counts, [5, 325, 75]);
    }

    /// A reset through a retained generation cannot change its replacement;
    /// attachment uses the current level clock once, and wet ticking is required.
    #[test]
    fn retained_reset_keeps_identity_and_non_dry_ticks_are_explicit() {
        let mut level = super::super::placement_tests::bare_level();
        level.set_game_time(42);
        let state = Block::POTENT_SULFUR.default_state();
        level.set_block_state(POS, state).unwrap();
        level
            .set_block_entity(entity::create(POS, state).unwrap())
            .unwrap();
        let old = level.block_entity_ref(POS).unwrap().unwrap();
        assert_eq!(
            level
                .block_entity::<PotentSulfurBlockEntity>(old)
                .unwrap()
                .eruption_tick,
            42
        );
        level
            .block_entity_mut::<PotentSulfurBlockEntity>(old)
            .unwrap()
            .waiting_countdown = 9;
        level.retain_block_entity(old).unwrap();
        level.set_game_time(99);
        level
            .set_block_entity(entity::create(POS, state).unwrap())
            .unwrap();
        let replacement = level.block_entity_ref(POS).unwrap().unwrap();
        level
            .block_entity_mut::<PotentSulfurBlockEntity>(replacement)
            .unwrap()
            .waiting_countdown = 17;
        old.reset_potent_sulfur_countdown(&mut level).unwrap();
        let original = level.block_entity::<PotentSulfurBlockEntity>(old).unwrap();
        assert_eq!(
            (original.waiting_countdown, original.eruption_tick),
            (-1, 42)
        );
        let current = level
            .block_entity::<PotentSulfurBlockEntity>(replacement)
            .unwrap();
        assert_eq!((current.waiting_countdown, current.eruption_tick), (17, 99));
        assert!(
            !level
                .block_entity_state(replacement)
                .unwrap()
                .has_server_ticker()
        );
        let wet = state
            .with(p::POTENT_SULFUR_STATE, p::PotentSulfurState::Wet)
            .unwrap();
        level
            .block_entity_mut::<BlockEntityData>(replacement)
            .unwrap()
            .set_block_state(wet)
            .unwrap();
        let current = level.block_entity_state(replacement).unwrap();
        assert!(current.has_server_ticker());
        assert!(matches!(
            current.behavior().tick(replacement, &mut level),
            Err(LevelError::UnsupportedBlockEntity(Block::POTENT_SULFUR))
        ));
        level.release_block_entity(old);
    }
}
