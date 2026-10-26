use super::*;
use ferraloom_data::Item;
use ferraloom_nbt::parse_compound;
use ferraloom_protocol::component::{ItemStack, Persistent, PotDecorations};
use ferraloom_util::{
    Direction,
    random::{LegacyRandomSource, RandomSource},
};
use std::io::Read;

const POS: BlockPos = BlockPos::new(8, 64, 8);

fn prime(entity: &mut BlockEntityState) {
    match entity {
        BlockEntityState::Campfire(entity) => {
            entity.items[0] = ItemStack::new(Item::DIAMOND, 3, DataComponentPatch::default());
            entity.cooking_progress = [91, 92, 93, 94];
            entity.cooking_time = [201, 202, 203, 204];
        }
        BlockEntityState::Bell(entity) => {
            entity.last_ring_timestamp = 91;
            entity.ticks = 7;
            entity.shaking = true;
            entity.click_direction = Some(Direction::West);
            entity.nearby_entities = Some(Vec::new());
            entity.resonating = true;
            entity.resonation_ticks = 8;
        }
        BlockEntityState::Lectern(entity) => {
            entity.book = ItemStack::new(Item::DIAMOND, 3, DataComponentPatch::default());
            entity.page = 7;
            entity.page_count = 9;
        }
        BlockEntityState::DecoratedPot(entity) => {
            entity.wobble_started_at_tick = 77;
            entity.last_wobble_style = Some(decorated_pot::WobbleStyle::Negative);
            entity.item = ItemStack::new(Item::DIAMOND, 3, DataComponentPatch::default());
            let brick = ferraloom_protocol::component::ItemStackTemplate::new(
                Item::BRICK,
                1,
                DataComponentPatch::default(),
            )
            .unwrap();
            entity.decorations = PotDecorations {
                back: Some(brick.clone()),
                left: Some(brick.clone()),
                right: Some(brick.clone()),
                front: Some(brick),
            };
        }
        _ => unreachable!(),
    }
}
fn scalar(entity: &BlockEntityState) -> String {
    match entity {
        BlockEntityState::Campfire(entity) => {
            format!("{:?};{:?}", entity.cooking_progress, entity.cooking_time).replace(' ', "")
        }
        BlockEntityState::Bell(entity) => format!(
            "{},{},{},{},{},{},{}",
            entity.last_ring_timestamp,
            entity.ticks,
            entity.shaking,
            entity.click_direction.map_or_else(
                || "null".into(),
                |direction| format!("{direction:?}").to_lowercase()
            ),
            entity
                .nearby_entities
                .as_ref()
                .map_or_else(|| "null".into(), |entities| entities.len().to_string()),
            entity.resonating,
            entity.resonation_ticks
        ),
        BlockEntityState::Lectern(entity) => format!(
            "{},{},{},{}",
            entity.page,
            entity.page_count,
            entity.has_book(),
            entity.redstone_signal()
        ),
        BlockEntityState::DecoratedPot(entity) => format!(
            "{},{},{}",
            entity.wobble_started_at_tick,
            match entity.last_wobble_style {
                None => "null",
                Some(decorated_pot::WobbleStyle::Positive) => "POSITIVE",
                Some(decorated_pot::WobbleStyle::Negative) => "NEGATIVE",
            },
            entity.loot_table_seed
        ),
        _ => unreachable!(),
    }
}

#[test]
fn native_campfire_bell_lectern_pot_data_factories_and_components_match() {
    use ferraloom_worldgen::region::WorldgenBlockEntity;
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/entities/factory-batch-two.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let context = crate::level::LevelContext::fixture(0, 63);
    let registries = context.registry_context().unwrap();
    let mut checked = [0; 3];
    for row in vectors.lines() {
        let f: Vec<_> = row.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        assert!(supports(state.block()), "{row}");
        assert!(supports_creation(state.block()), "{row}");
        let mut entity = new_block_entity(POS, state).unwrap().unwrap();
        match f[0] {
            "factory2-data" => {
                if f[2] != "@new" {
                    prime(&mut entity);
                    entity.load_with_components(&parse_compound(f[2]).unwrap(), registries);
                }
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(f[3]).unwrap(),
                    "save {row}"
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
                assert_eq!(scalar(&entity), f[7], "scalar {row}");
                assert_eq!(
                    entity.has_server_ticker(Some(true)).unwrap(),
                    f[8] == "true",
                    "ticker {row}"
                );
                let saved = entity.save_with_full_metadata().unwrap();
                let round = load_static(POS, state, &saved, registries)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    round.save_with_full_metadata().unwrap(),
                    parse_compound(f[9]).unwrap(),
                    "round trip {row}"
                );
                checked[0] += 1;
            }
            "factory2-components" => {
                prime(&mut entity);
                entity.load_with_components(&parse_compound("{LootTable:'chests/simple_dungeon',LootTableSeed:5L,CookingTimes:[I;1,2,3,4],CookingTotalTimes:[I;11,12,13,14]}").unwrap(),registries);
                let item =
                    ItemStack::from_nbt(&Tag::Compound(parse_compound(f[2]).unwrap())).unwrap();
                entity.apply_components_from_item_stack(&item).unwrap();
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(f[3]).unwrap(),
                    "save {row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    components::load(Some(&parse_compound(f[4]).unwrap()), registries),
                    "components {row}"
                );
                let mut stripped = entity.save_custom_only().unwrap();
                entity.remove_components_from_tag(&mut stripped);
                assert_eq!(stripped, parse_compound(f[5]).unwrap(), "stripped {row}");
                assert_eq!(scalar(&entity), f[6], "scalar {row}");
                checked[1] += 1;
            }
            "factory2-loot" => {
                prime(&mut entity);
                assert!(entity.is_randomizable());
                let seed = f[3].parse().unwrap();
                let mut random = LegacyRandomSource::new(seed);
                let selected_seed = if f[2] == "loot" {
                    random.next_long()
                } else {
                    seed
                };
                WorldgenBlockEntity::set_loot_table(
                    &mut entity,
                    &"minecraft:chests/simple_dungeon".parse().unwrap(),
                    selected_seed,
                )
                .unwrap();
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(f[4]).unwrap(),
                    "loot {row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    components::load(Some(&parse_compound(f[5]).unwrap()), registries),
                    "loot components {row}"
                );
                assert_eq!(
                    random.next_long(),
                    f[6].parse::<i64>().unwrap(),
                    "RNG {row}"
                );
                checked[2] += 1;
            }
            _ => panic!("unknown native factory observation {row}"),
        }
    }
    assert_eq!(checked, [62, 14, 4]);
}

#[test]
fn live_lectern_resolution_and_unported_server_tickers_are_required() {
    use crate::{
        Behaviors,
        level::{LevelContext, Region, chunk::LevelChunk},
    };
    use std::sync::Arc;
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let biome = ferraloom_chunk::BiomeId(0);
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            0..8,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
                )),
                biome,
            ),
        ))
        .unwrap();
    for block in [Block::CAMPFIRE, Block::SOUL_CAMPFIRE, Block::BELL] {
        level.set_block_state(POS, block.default_state()).unwrap();
        level
            .set_block_entity(create(POS, block.default_state()).unwrap())
            .unwrap();
        let handle = level.block_entity_ref(POS).unwrap().unwrap();
        assert!(
            level
                .block_entity_state(handle)
                .unwrap()
                .has_server_ticker(None)
                .unwrap()
        );
        let behavior = level.block_entity_state(handle).unwrap().behavior();
        assert_eq!(
            behavior.tick(handle, &mut level),
            Err(LevelError::UnsupportedBlockEntity(block))
        );
        if block != Block::BELL {
            assert_eq!(
                behavior.pre_remove_side_effects(handle, &mut level),
                Err(LevelError::UnsupportedBlockEntity(block))
            );
        }
    }
    level
        .set_block_state(POS, Block::LECTERN.default_state())
        .unwrap();
    level
        .set_block_entity(create(POS, Block::LECTERN.default_state()).unwrap())
        .unwrap();
    let handle = level.block_entity_ref(POS).unwrap().unwrap();
    let before = level
        .block_entity_state(handle)
        .unwrap()
        .save_with_full_metadata()
        .unwrap();
    let unresolved=parse_compound("{Book:{id:'written_book',count:1,components:{'minecraft:written_book_content':{title:'native',author:'a',pages:['literal']}}}}").unwrap();
    assert_eq!(
        handle.load_with_components(&unresolved, &mut level),
        Err(LevelError::UnsupportedBlockEntity(Block::LECTERN))
    );
    assert_eq!(
        level
            .block_entity_state(handle)
            .unwrap()
            .save_with_full_metadata()
            .unwrap(),
        before
    );
    let resolved=parse_compound("{Book:{id:'written_book',count:1,components:{'minecraft:written_book_content':{title:'done',author:'a',pages:['literal'],resolved:true}}}}").unwrap();
    handle.load_with_components(&resolved, &mut level).unwrap();
    assert!(
        level
            .block_entity::<lectern::LecternBlockEntity>(handle)
            .unwrap()
            .has_book()
    );
    level
        .set_block_state(POS, Block::DECORATED_POT.default_state())
        .unwrap();
    level
        .set_block_entity(create(POS, Block::DECORATED_POT.default_state()).unwrap())
        .unwrap();
    assert_eq!(
        crate::block::containers::access::block_container_at(POS, &level),
        Err(LevelError::UnsupportedBlockEntity(Block::DECORATED_POT))
    );
}
