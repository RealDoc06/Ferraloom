use super::*;
use ferraloom_protocol::component::{DataComponent, ItemStack};

fn portal() -> BlockEntityState {
    create(BlockPos::new(17, -4, -1), Block::END_PORTAL.default_state()).unwrap()
}
#[test]
fn registry_validity_matches_real_vanilla_types_and_every_block() {
    let mut types = 0;
    for line in include_str!("../../../tests/java/blocks/entities/vectors.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields[0] != "registry" {
            continue;
        }
        let kind = BlockEntityType::from_name(fields[1]).unwrap();
        let expected: Vec<_> = fields[2]
            .split(',')
            .map(|name| Block::from_name(name).unwrap())
            .collect();
        assert_eq!(registry::valid_blocks(kind), expected);
        for &block in ferraloom_data::registries::block::ALL {
            assert_eq!(
                registry::is_valid(kind, block.default_state()),
                expected.contains(&block),
                "{} {}",
                kind.name(),
                block.name()
            );
        }
        types += 1;
    }
    assert_eq!(
        types,
        ferraloom_data::registries::block_entity_type::ALL.len()
    );
}
#[test]
fn base_save_full_metadata_and_update_match_vanilla() {
    let entity = portal();
    let fixtures = include_str!("../../../tests/java/blocks/entities/vectors.tsv");
    for (name, actual) in [
        ("base-save", entity.save_without_metadata().unwrap()),
        ("base-full", entity.save_with_full_metadata().unwrap()),
        ("base-update", entity.get_update_tag().unwrap()),
    ] {
        let line = fixtures
            .lines()
            .find(|line| line.starts_with(&format!("{name}\t")))
            .unwrap();
        assert_eq!(
            actual,
            ferraloom_nbt::parse_compound(line.split('\t').nth(1).unwrap()).unwrap()
        );
    }
    assert!(entity.get_update_packet().unwrap().is_none());
    let info = entity.chunk_packet_info().unwrap();
    assert_eq!(info.packed_xz, 31);
    assert_eq!(info.y, -4);
    assert!(info.tag.0.is_none());
}
#[test]
fn components_keep_only_unconsumed_added_patch_values() {
    let mut entity = portal();
    let stack = ItemStack::new(ferraloom_data::Item::STONE, 1, Default::default());
    let mut patch = DataComponentPatch::default();
    patch.set(DataComponent::MaxStackSize(
        ferraloom_protocol::codec::VarInt(7),
    ));
    patch.set(DataComponent::CustomName(Box::new(
        ferraloom_protocol::chat::Component::literal("kept"),
    )));
    patch.set(DataComponent::BlockState(
        ferraloom_protocol::component::BlockItemStateProperties(Vec::new()),
    ));
    patch.remove(ComponentTypeId::DAMAGE);
    entity.apply_components(&stack.components().unwrap(), &patch);
    let line = include_str!("../../../tests/java/blocks/entities/vectors.tsv")
        .lines()
        .find(|line| line.starts_with("base-components\t"))
        .unwrap();
    let saved = entity.save_without_metadata().unwrap();
    assert_eq!(
        saved,
        ferraloom_nbt::parse_compound(line.split_once('\t').unwrap().1).unwrap()
    );
    let mut loaded = portal();
    loaded.load_with_components(&saved);
    assert_eq!(loaded.collect_components(), entity.collect_components());
}
#[test]
fn wrong_chunk_positions_follow_vanilla_local_coordinate_fixup() {
    for line in include_str!("../../../tests/java/blocks/entities/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("position\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let coords = |text: &str| {
            text.split(", ")
                .map(|n| n.parse::<i32>().unwrap())
                .collect::<Vec<_>>()
        };
        let p = coords(fields[1]);
        let expected = coords(fields[2]);
        let mut tag = CompoundTag::new();
        for (key, value) in ["x", "y", "z"].into_iter().zip(p) {
            tag.insert(key, Tag::Int(value));
        }
        assert_eq!(
            get_pos_from_tag(ChunkPos::new(2, -3), &tag),
            BlockPos::new(expected[0], expected[1], expected[2])
        );
    }
}
#[test]
fn unsupported_and_mismatched_factories_do_not_fabricate_data() {
    assert!(create(BlockPos::new(0, 0, 0), Block::SPAWNER.default_state()).is_err());
    let mut tag = CompoundTag::new();
    tag.insert("id", Tag::String("minecraft:chest".into()));
    assert!(
        load_static(
            BlockPos::new(0, 0, 0),
            Block::END_PORTAL.default_state(),
            &tag
        )
        .unwrap()
        .is_none()
    );
    tag.insert("id", Tag::String("DUMMY".into()));
    let entity = load_static(
        BlockPos::new(0, 0, 0),
        Block::END_PORTAL.default_state(),
        &tag,
    )
    .unwrap()
    .unwrap();
    assert_eq!(entity.data().kind, BlockEntityType::END_PORTAL);
}
#[test]
fn ticker_add_remove_and_rebind_preserve_phase_membership() {
    use ticking::BlockEntityTickers;
    let mut tickers = BlockEntityTickers::default();
    let a = BlockEntityRef(BlockPos::new(0, 0, 0));
    let b = BlockEntityRef(BlockPos::new(1, 0, 0));
    let c = BlockEntityRef(BlockPos::new(2, 0, 0));
    let first = tickers.bind(a, BlockEntityType::FURNACE);
    let old = tickers.bind(b, BlockEntityType::FURNACE);
    tickers.begin_phase();
    assert_eq!(tickers.next_ticker().unwrap().entity, a);
    tickers.unbind(b.0);
    tickers.bind(b, BlockEntityType::SMOKER);
    tickers.bind(c, BlockEntityType::FURNACE);
    assert!(tickers.next_ticker().is_none());
    assert!(tickers.wrapper(old).is_none());
    tickers.end_phase();
    tickers.begin_phase();
    assert_eq!(tickers.next_ticker().unwrap().entity, a);
    assert_eq!(
        tickers.bind(b, BlockEntityType::BLAST_FURNACE),
        tickers.bind(b, BlockEntityType::SMOKER)
    );
    assert_eq!(tickers.next_ticker().unwrap().kind, BlockEntityType::SMOKER);
    assert_eq!(tickers.next_ticker().unwrap().entity, c);
    assert!(tickers.next_ticker().is_none());
    tickers.end_phase();
    assert!(tickers.wrapper(first).is_some());
}
