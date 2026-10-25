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
    // Native DataComponentMap equality ignores its backing-map traversal order.
    // Compare the loaded persistent map, rather than the Rust entry vector.
    assert_eq!(loaded.save_without_metadata().unwrap(), saved);
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
    assert!(create(BlockPos::new(0, 0, 0), Block::TRIAL_SPAWNER.default_state()).is_err());
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
    let a = BlockEntityRef {
        pos: BlockPos::new(0, 0, 0),
        generation: 1,
    };
    let b = BlockEntityRef {
        pos: BlockPos::new(1, 0, 0),
        generation: 2,
    };
    let c = BlockEntityRef {
        pos: BlockPos::new(2, 0, 0),
        generation: 3,
    };
    let first = tickers.bind(a, BlockEntityType::FURNACE);
    let old = tickers.bind(b, BlockEntityType::FURNACE);
    tickers.begin_phase();
    assert_eq!(tickers.next_ticker().unwrap().entity, a);
    tickers.unbind(b.pos);
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

#[test]
fn container_and_sign_persistence_matches_real_block_entities() {
    use crate::inventory::Container;
    for line in include_str!("../../../tests/java/blocks/entities/vectors.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if !matches!(
            fields[0],
            "default-save"
                | "filled-save"
                | "update-tag"
                | "update-packet"
                | "loot-save"
                | "loot-zero"
        ) {
            continue;
        }
        let block = Block::from_name(fields[1]).unwrap();
        let mut entity = create(BlockPos::new(1, 64, 2), block.default_state()).unwrap();
        match fields[0] {
            "filled-save" => {
                if let Some(container) = entity.container_mut() {
                    let size = container.size();
                    container.set_item(
                        0,
                        ItemStack::new(ferraloom_data::Item::STONE, 31, Default::default()),
                    );
                    container.set_item(
                        size - 1,
                        ItemStack::new(ferraloom_data::Item::DIAMOND, 3, Default::default()),
                    );
                }
                let expected = ferraloom_nbt::parse_compound(fields[2]).unwrap();
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    expected,
                    "{line}"
                );
                let mut loaded = create(entity.data().pos, block.default_state()).unwrap();
                loaded.load_with_components(&expected);
                assert_eq!(
                    loaded.save_with_full_metadata().unwrap(),
                    expected,
                    "roundtrip {}",
                    fields[1]
                );
            }
            "loot-save" | "loot-zero" => {
                let container = entity.randomizable_mut().unwrap();
                container.loot_table = Some(
                    ferraloom_protocol::Identifier::parse("minecraft:chests/simple_dungeon")
                        .unwrap(),
                );
                container.loot_table_seed = if fields[0] == "loot-zero" {
                    0
                } else {
                    -9123456789
                };
                let expected = ferraloom_nbt::parse_compound(fields[2]).unwrap();
                assert_eq!(entity.save_without_metadata().unwrap(), expected, "{line}");
                entity.load_with_components(&expected);
                assert!(entity.container().unwrap().is_empty());
            }
            "default-save" => assert_eq!(
                entity.save_without_metadata().unwrap(),
                ferraloom_nbt::parse_compound(fields[2]).unwrap(),
                "{line}"
            ),
            "update-tag" => assert_eq!(
                entity.get_update_tag().unwrap(),
                ferraloom_nbt::parse_compound(fields[2]).unwrap(),
                "{line}"
            ),
            "update-packet" => assert_eq!(
                entity.get_update_packet().unwrap().is_some(),
                fields[2] == "true",
                "{line}"
            ),
            _ => unreachable!(),
        }
    }
}
#[test]
fn shulker_items_and_names_are_components_in_the_dropped_stack() {
    use crate::inventory::Container;
    let mut entity = create(BlockPos::ZERO, Block::SHULKER_BOX.default_state()).unwrap();
    let container = entity.container_mut().unwrap();
    container.custom_name = Some(ferraloom_protocol::chat::Component::literal("Box"));
    container.set_item(
        26,
        ItemStack::new(ferraloom_data::Item::DIAMOND, 7, Default::default()),
    );
    let components = entity.collect_components();
    let Some(DataComponent::Container(items)) = components.get(ComponentTypeId::CONTAINER) else {
        panic!("container component");
    };
    assert_eq!(items.0.len(), 27);
    assert_eq!(items.0[26].as_ref().unwrap().count, 7);
    let mut patch = DataComponentPatch::default();
    for component in components.entries {
        patch.set(component);
    }
    let mut placed = create(BlockPos::ZERO, Block::SHULKER_BOX.default_state()).unwrap();
    placed.apply_components(&DataComponentMap::default(), &patch);
    assert_eq!(placed.collect_components(), entity.collect_components());
    assert!(placed.data().components.entries.is_empty());
}
#[test]
fn native_ticker_additions_wait_and_removed_tickers_skip_even_when_frozen() {
    use ticking::BlockEntityTickers;
    let mut tickers = BlockEntityTickers::default();
    let a = BlockEntityRef {
        pos: BlockPos::ZERO,
        generation: 1,
    };
    let b = BlockEntityRef {
        pos: BlockPos::new(1, 0, 0),
        generation: 2,
    };
    let c = BlockEntityRef {
        pos: BlockPos::new(2, 0, 0),
        generation: 3,
    };
    tickers.bind(a, BlockEntityType::FURNACE);
    tickers.bind(b, BlockEntityType::FURNACE);
    let mut traces = Vec::new();
    tickers.begin_phase();
    let mut first = Vec::new();
    while let Some(ticker) = tickers.next_ticker() {
        first.push(if ticker.entity == a { "a" } else { "b" });
        tickers.unbind(b.pos);
        tickers.bind(c, BlockEntityType::FURNACE);
    }
    tickers.end_phase();
    traces.push(first.join(","));
    tickers.begin_phase();
    let mut second = Vec::new();
    while let Some(ticker) = tickers.next_ticker() {
        second.push(if ticker.entity == a { "a" } else { "c" });
    }
    tickers.end_phase();
    traces.push(second.join(","));
    tickers.unbind(a.pos);
    tickers.begin_phase();
    while tickers.next_ticker().is_some() {}
    tickers.end_phase();
    traces.push(String::new());
    let expected: Vec<_> = include_str!("../../../tests/java/blocks/entities/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("ticker-phase\t"))
        .map(|line| line.split('\t').nth(2).unwrap())
        .collect();
    assert_eq!(traces, expected);
}

#[test]
fn sign_edits_obey_wax_editor_and_filtering_like_vanilla() {
    use super::super::containers::sign::SignBlockEntity;
    for line in include_str!("../../../tests/java/blocks/entities/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("sign-edit\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let waxed = fields[1] == "true";
        let allowed = fields[2] == "true";
        let filtered = fields[3] == "true";
        let mut entity = create(BlockPos::ZERO, Block::OAK_SIGN.default_state()).unwrap();
        entity.data_mut().has_level = true;
        let player = ferraloom_protocol::Uuid { most: 1, least: 2 };
        let sign = SignBlockEntity::view_mut(&mut entity).unwrap();
        sign.is_waxed = waxed;
        sign.allowed_player_editor = Some(if allowed {
            player
        } else {
            ferraloom_protocol::Uuid { most: 3, least: 4 }
        });
        let lines = [
            ("first".into(), "first".into()),
            ("second".into(), String::new()),
            ("third".into(), "third".into()),
            ("fourth".into(), "fourth".into()),
        ];
        assert_eq!(
            sign.update_sign_text(player, true, &lines, filtered),
            !waxed && allowed
        );
        assert_eq!(sign.allowed_player_editor.is_none(), fields[5] == "true");
        assert_eq!(
            entity.save_without_metadata().unwrap(),
            ferraloom_nbt::parse_compound(fields[4]).unwrap(),
            "{line}"
        );
    }
}
