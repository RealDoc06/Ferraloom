//! Brushable block storage. Brushing, retraction and archaeology item release
//! remain block/entity behavior ports; worldgen only attaches native loot data.
use super::{BlockEntity, BlockEntityData, BlockEntityState};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Identifier,
    component::{ItemStack, Persistent},
};
use ferraloom_util::Direction;

/// Port of BrushableBlockEntity. Brush counters are transient and loadAdditional
/// leaves their existing values intact; hit direction is update-only on save.
#[derive(Clone, Debug)]
pub struct BrushableBlockEntity {
    pub parent: BlockEntityData,
    pub brush_count: i32,
    pub brush_count_resets_at_tick: i64,
    pub cool_down_ends_at_tick: i64,
    pub item: ItemStack,
    pub hit_direction: Option<Direction>,
    pub loot_table: Option<Identifier>,
    pub loot_table_seed: i64,
}
impl BrushableBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            brush_count: 0,
            brush_count_resets_at_tick: 0,
            cool_down_ends_at_tick: 0,
            item: ItemStack::default(),
            hit_direction: None,
            loot_table: None,
            loot_table_seed: 0,
        }
    }
    /// Port of BrushableBlockEntity#setLootTable. No RNG, setChanged or item reset.
    pub fn set_loot_table(&mut self, table: Identifier, seed: i64) {
        self.loot_table = Some(table);
        self.loot_table_seed = seed;
    }
}
pub(super) struct BrushableBehavior;
pub(super) static BRUSHABLE_ENTITY: BrushableBehavior = BrushableBehavior;
impl BlockEntity for BrushableBehavior {
    fn load_additional(&self, state: &mut BlockEntityState, tag: &CompoundTag) {
        let BlockEntityState::Brushable(entity) = state else {
            unreachable!()
        };
        entity.loot_table = tag
            .get_string("LootTable")
            .and_then(|value| value.to_string_checked().ok())
            .and_then(|value| Identifier::parse(&value).ok());
        entity.loot_table_seed = tag.get_long_or("LootTableSeed", 0);
        entity.item = if entity.loot_table.is_some() {
            ItemStack::default()
        } else {
            tag.get("item")
                .and_then(|value| ItemStack::from_nbt(value).ok())
                .unwrap_or_default()
        };
        entity.hit_direction = tag
            .get_byte("hit_direction")
            .map(|value| Direction::from_3d_data_value(i32::from(value)));
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Brushable(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        if let Some(table) = &entity.loot_table {
            tag.insert("LootTable", Tag::String(table.as_str().into()));
            if entity.loot_table_seed != 0 {
                tag.insert("LootTableSeed", Tag::Long(entity.loot_table_seed));
            }
        } else if !entity.item.is_empty() {
            tag.insert("item", entity.item.to_nbt()?);
        }
        Ok(tag)
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Brushable(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        if let Some(direction) = entity.hit_direction {
            tag.insert("hit_direction", Tag::Byte(direction as i8));
        }
        if !entity.item.is_empty() {
            tag.insert("item", entity.item.to_nbt()?);
        }
        Ok(tag)
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::entity::{create, load_static};
    use ferraloom_data::Block;
    use ferraloom_nbt::parse_compound;
    use ferraloom_util::BlockPos;

    #[test]
    fn native_brushable_load_save_updates_and_explicit_seed_preserve_transient_fields() {
        let pos = BlockPos::new(8, 64, 8);
        let mut cases = 0;
        for row in include_str!("../../../tests/java/blocks/entities/brushable.tsv")
            .lines()
            .filter(|row| row.starts_with("brushable\t"))
        {
            let f: Vec<_> = row.split('\t').collect();
            let state = Block::from_name(f[1]).unwrap().default_state();
            let mut entity = create(pos, state).unwrap();
            let BlockEntityState::Brushable(data) = &mut entity else {
                unreachable!()
            };
            data.brush_count = 5;
            data.brush_count_resets_at_tick = 311;
            data.cool_down_ends_at_tick = 83;
            entity.load_with_components(&parse_compound(f[3]).unwrap());
            if f[2] != "load" {
                let BlockEntityState::Brushable(data) = &mut entity else {
                    unreachable!()
                };
                data.set_loot_table(
                    Identifier::parse("minecraft:archaeology/desert_well").unwrap(),
                    if f[2] == "set-zero" { 0 } else { pos.as_long() },
                );
            }
            let saved = entity.save_with_full_metadata().unwrap();
            assert_eq!(saved, parse_compound(f[4]).unwrap(), "{row}");
            assert_eq!(
                entity.get_update_tag().unwrap(),
                parse_compound(f[5]).unwrap(),
                "{row}"
            );
            let again = load_static(pos, state, &saved).unwrap().unwrap();
            assert_eq!(
                again.get_update_tag().unwrap(),
                parse_compound(f[6]).unwrap(),
                "{row}"
            );
            let BlockEntityState::Brushable(data) = &entity else {
                unreachable!()
            };
            assert_eq!(
                format!(
                    "{},{},{}",
                    data.brush_count, data.brush_count_resets_at_tick, data.cool_down_ends_at_tick
                ),
                f[7]
            );
            assert_eq!(
                entity.get_update_packet().unwrap().is_some(),
                f[8] == "true"
            );
            assert!(entity.randomizable().is_none());
            cases += 1;
        }
        assert_eq!(cases, 60);
    }
}
