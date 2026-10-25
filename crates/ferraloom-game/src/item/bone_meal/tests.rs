use super::*;
use crate::{entity::player::inventory, inventory::StackOperation, item::stack::tests::level};
use ferraloom_data::Item;
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
use std::io::Read;

#[test]
fn crop_item_dispatch_matches_native_growth_vectors_and_skips_mature_rng() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/plants/growth.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let (mut level, player) = level();
    let pos = BlockPos::new(8, 5, 8);
    let mut cases = 0;
    for line in vectors.lines().filter(|line| line.starts_with("grow\t")) {
        let fields: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        let seed = fields[2].parse::<i64>().unwrap();
        level
            .set_block_state(pos.below(1), Block::FARMLAND.default_state())
            .unwrap();
        level.set_block_state(pos, state).unwrap();
        *level.random() = Random::Legacy(LegacyRandomSource::new(seed));
        let mut operation = StackOperation::new(&mut level);
        let receiver = operation
            .own(
                player,
                ItemStack::new(Item::BONE_MEAL, 4, Default::default()),
            )
            .unwrap();
        let max = match state.block() {
            Block::BEETROOTS => 3,
            Block::TORCHFLOWER_CROP => 2,
            _ => 7,
        };
        let valid = state.get(p::AGE).unwrap() < max;
        assert_eq!(
            grow_crop(receiver, pos, &mut operation).unwrap(),
            valid,
            "{line}"
        );
        let expected = if valid {
            fields[3].parse::<u16>().unwrap()
        } else {
            state.id()
        };
        assert_eq!(
            operation.level.get_block_state(pos).unwrap().id(),
            expected,
            "{line}"
        );
        assert_eq!(
            inventory::count(operation.level.stack(receiver).unwrap()),
            if valid { 3 } else { 4 }
        );
        let random = if valid {
            fields[4].parse::<i64>().unwrap()
        } else {
            LegacyRandomSource::new(seed).next_long()
        };
        assert_eq!(operation.level.random().next_long(), random, "{line}");
        cases += 1;
    }
    assert_eq!(cases, 448);
}
