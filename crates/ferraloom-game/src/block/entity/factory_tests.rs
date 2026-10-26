use super::*;
use ferraloom_data::Item;
use ferraloom_nbt::parse_compound;
use ferraloom_protocol::component::{DyeColor, Persistent};
use std::io::Read;

fn seed(entity: &mut BlockEntityState) {
    match entity {
        BlockEntityState::EnderChest(e) => {
            e.open_count = 3;
            e.max_interaction_range = 9.5;
            e.openness = 0.6;
            e.old_openness = 0.2;
            e.should_be_open = true;
        }
        BlockEntityState::EnchantingTable(e) => {
            e.time = 123;
            e.flip = 0.75;
            e.old_flip = 0.75;
            e.flip_target = 0.75;
            e.flip_acceleration = 0.75;
            e.open = 0.75;
            e.old_open = 0.75;
            e.rotation = 0.75;
            e.old_rotation = 0.75;
            e.target_rotation = 0.75;
        }
        BlockEntityState::BrewingStand(e) => {
            e.ingredient = Some(Item::DIAMOND);
            e.last_potion_count = Some([true, false, true]);
        }
        BlockEntityState::Banner(_) => {}
        _ => unreachable!(),
    }
}
fn transient_state(entity: &BlockEntityState) -> String {
    match entity {
        BlockEntityState::Banner(e) => match e.base_color().to_nbt().unwrap() {
            Tag::String(color) => color.to_string_lossy(),
            _ => unreachable!(),
        },
        BlockEntityState::EnderChest(e) => format!(
            "{},{:.1},{},{:x},{:x}",
            e.open_count,
            e.max_interaction_range,
            e.should_be_open,
            e.openness.to_bits(),
            e.old_openness.to_bits()
        ),
        BlockEntityState::EnchantingTable(e) => {
            let mut fields = vec![e.time.to_string()];
            fields.extend(
                [
                    e.flip,
                    e.old_flip,
                    e.flip_target,
                    e.flip_acceleration,
                    e.open,
                    e.old_open,
                    e.rotation,
                    e.old_rotation,
                    e.target_rotation,
                ]
                .map(|v| format!("{:x}", v.to_bits())),
            );
            fields.join(",")
        }
        BlockEntityState::BrewingStand(e) => {
            let potions = e.last_potion_count.map_or_else(
                || "null".into(),
                |values| format!("[{},{},{}]", values[0], values[1], values[2]),
            );
            format!("{},{}", e.ingredient.map_or("null", Item::name), potions)
        }
        _ => unreachable!(),
    }
}

#[test]
fn native_banner_ender_enchanting_brewing_factories_and_registry_partials_match() {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/entities/factory-batch.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let context = crate::level::LevelContext::fixture(0, 63);
    let registries = context.registry_context().unwrap();
    let mut cases = 0;
    for row in text.lines().filter(|row| row.starts_with("factory-data\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let state = Block::from_name(f[1]).unwrap().default_state();
        assert!(supports(state.block()));
        let pos = BlockPos::new(8, 64, 8);
        let mut entity = create(pos, state).unwrap();
        if f[2] != "@new" {
            seed(&mut entity);
            entity.load_with_components(&parse_compound(f[2]).unwrap(), registries);
        }
        assert_eq!(
            entity.save_with_full_metadata().unwrap(),
            parse_compound(f[3]).unwrap(),
            "saved {row}"
        );
        assert_eq!(
            entity.get_update_tag().unwrap(),
            parse_compound(f[4]).unwrap(),
            "update {row}"
        );
        assert_eq!(
            entity.get_update_packet().unwrap().is_some(),
            f[5] == "true",
            "packet {row}"
        );
        assert_eq!(
            entity.collect_components(),
            components::load(Some(&parse_compound(f[6]).unwrap()), registries),
            "components {row}"
        );
        assert_eq!(transient_state(&entity), f[7], "transient {row}");
        assert_eq!(
            entity.has_server_ticker(Some(true)).unwrap(),
            f[8] == "true",
            "ticker {row}"
        );
        if f[9] != "-" {
            let name = match &entity {
                BlockEntityState::Banner(e) => e.get_name(),
                BlockEntityState::EnchantingTable(e) => e.get_name(),
                BlockEntityState::BrewingStand(e) => {
                    e.parent.custom_name.clone().unwrap_or_else(|| {
                        ferraloom_protocol::chat::Component::translatable(
                            "container.brewing",
                            Vec::new(),
                        )
                    })
                }
                _ => unreachable!(),
            };
            let expected =
                ferraloom_protocol::chat::Component::from_nbt(ferraloom_nbt::parse(f[9]).unwrap())
                    .unwrap();
            assert_eq!(name, expected, "name {row}");
        }
        let saved = entity.save_with_full_metadata().unwrap();
        let reloaded = load_static(pos, state, &saved, registries)
            .unwrap()
            .unwrap();
        assert_eq!(
            reloaded.save_with_full_metadata().unwrap(),
            saved,
            "round trip {row}"
        );
        if let BlockEntityState::Banner(e) = &entity {
            // Color is constructor state and never comes from a persistent tag.
            assert_eq!(
                e.base_color(),
                DyeColor::from_nbt(&Tag::String(
                    state.block().as_item().data().banner_color.unwrap().into()
                ))
                .unwrap()
            );
        }
        cases += 1;
    }
    assert_eq!(cases, 504);
}

#[test]
fn banner_holder_binding_uses_the_supplied_registry_view_without_builtin_fallback() {
    use ferraloom_protocol::{Identifier, codec::RegistryContext};
    struct WithoutCross<'a>(&'a dyn RegistryContext);
    impl RegistryContext for WithoutCross<'_> {
        fn entry_id(&self, registry: &Identifier, entry: &Identifier) -> Option<i32> {
            if registry.as_str() == "minecraft:banner_pattern"
                && entry.as_str() == "minecraft:cross"
            {
                None
            } else {
                self.0.entry_id(registry, entry)
            }
        }
        fn entry_key(&self, registry: &Identifier, id: i32) -> Option<Identifier> {
            self.0.entry_key(registry, id)
        }
        fn has_tag(&self, registry: &Identifier, tag: &Identifier) -> bool {
            self.0.has_tag(registry, tag)
        }
    }
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/entities/factory-batch.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let row = text
        .lines()
        .find(|row| row.starts_with("factory-registry\t"))
        .unwrap();
    let f: Vec<_> = row.split('\t').collect();
    let context = crate::level::LevelContext::fixture(0, 63);
    let mut entity = create(BlockPos::new(8, 64, 8), Block::WHITE_BANNER.default_state()).unwrap();
    entity.load_with_components(
        &parse_compound(f[1]).unwrap(),
        &WithoutCross(context.registry_context().unwrap()),
    );
    assert_eq!(
        entity.save_with_full_metadata().unwrap(),
        parse_compound(f[2]).unwrap()
    );
}
