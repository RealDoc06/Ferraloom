use super::*;
use crate::{
    Behaviors, ServerShared,
    block::entity::{self, BlockEntityDataView, chiseled_bookshelf::ChiseledBookShelfBlockEntity},
    entity::{EntityData, EntityRef, EntityState, player::GameType},
    level::{
        LevelContext, Region, RegionId,
        chunk::LevelChunk,
        effects::LevelEffect,
        game_event::test_support::{self as events, Observation},
    },
};
use ferraloom_data::{
    Block,
    datapack::{Pack, ResourceManager, TagSet},
};
use ferraloom_nbt::{Tag, parse_compound};
use ferraloom_protocol::{
    codec::Holder,
    component::{ItemStack, Persistent},
    play::clientbound::Packet,
};
use ferraloom_util::{ChunkPos, Uuid};
use std::{
    io::Read,
    sync::{Arc, LazyLock},
};

const POS: BlockPos = BlockPos::new(8, 64, 8);
const PLAYER: EntityRef = EntityRef::fixture(RegionId::FIXTURE, 1);
static BEHAVIORS: Behaviors = Behaviors::EMPTY;
struct TestTags {
    blocks: Arc<TagSet<Block>>,
    items: Arc<TagSet<Item>>,
}
static TAGS: LazyLock<TestTags> = LazyLock::new(|| {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla jar");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    TestTags {
        blocks: Arc::new(TagSet::load(&resources).unwrap()),
        items: Arc::new(TagSet::load(&resources).unwrap()),
    }
});
fn vectors() -> String {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/bookshelf-hooks.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
}
fn state(mask: u8) -> BlockState {
    [
        p::SLOT_0_OCCUPIED,
        p::SLOT_1_OCCUPIED,
        p::SLOT_2_OCCUPIED,
        p::SLOT_3_OCCUPIED,
        p::SLOT_4_OCCUPIED,
        p::SLOT_5_OCCUPIED,
    ]
    .into_iter()
    .enumerate()
    .fold(
        Block::CHISELED_BOOKSHELF.default_state(),
        |state, (slot, property)| state.with(property, mask & (1 << slot) != 0).unwrap(),
    )
}
fn fixture(mask: u8) -> (Level, BlockEntityRef) {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let biome = ferraloom_chunk::BiomeId(0);
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            0..6,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
                )),
                biome,
            ),
        ))
        .unwrap();
    level
        .set_chunk_loading_status(
            ChunkPos::new(0, 0),
            crate::level::distance_manager::FullChunkStatus::BlockTicking,
            true,
        )
        .unwrap();
    level.set_block_tags(TAGS.blocks.clone());
    level.set_item_tags(TAGS.items.clone());
    level.install_shared(Box::<ServerShared>::default());
    let state = state(mask);
    level.set_block_state(POS, state).unwrap();
    let mut shelf = entity::create(POS, state).unwrap();
    for (slot, item) in ChiseledBookShelfBlockEntity::view_mut(&mut shelf)
        .unwrap()
        .items
        .iter_mut()
        .enumerate()
    {
        if mask & (1 << slot) != 0 {
            *item = ItemStack::new(Item::BOOK, slot as i32 + 1, Default::default());
        }
    }
    level.set_block_entity(shelf).unwrap();
    let handle = level.block_entity_ref(POS).unwrap().unwrap();
    events::observe(&mut level);
    events::take(&mut level);
    (level, handle)
}
fn tag(stack: &ItemStack) -> Tag {
    if stack.is_empty() {
        Tag::Compound(Default::default())
    } else {
        stack.to_nbt().unwrap()
    }
}
fn expected(snbt: &str) -> Tag {
    Tag::Compound(parse_compound(snbt).unwrap())
}
fn saved(level: &Level, handle: BlockEntityRef) -> Tag {
    Tag::Compound(
        level
            .block_entity_state(handle)
            .unwrap()
            .save_custom_only()
            .unwrap(),
    )
}
fn trace(level: &mut Level) -> String {
    events::take(level)
        .into_iter()
        .filter_map(|event| match event {
            Observation::Effect(LevelEffect::BlockUpdated { pos, new, .. }) if pos == POS => {
                Some(format!("u:{}", new.id()))
            }
            Observation::GameEvent {
                event,
                position,
                source,
                affected_state,
            } => {
                assert_eq!(position, center(POS));
                Some(format!(
                    "g:{}:{}:{}",
                    event.protocol_id(),
                    source.map_or(0, |entity| level
                        .entity::<EntityData>(entity)
                        .unwrap()
                        .network_id),
                    affected_state.map_or(-1, |state| i32::from(state.id()))
                ))
            }
            Observation::Effect(LevelEffect::PlayerPacket { packet, .. }) => match *packet {
                Packet::Sound(sound) => {
                    let Holder::Reference(name) = sound.sound.0 else {
                        panic!("registered sound")
                    };
                    assert_eq!(sound.source, SoundSource::Blocks);
                    Some(format!(
                        "s:{name}:{:.1},{:.1},{:.1}:{:.1}:{:.1}",
                        f64::from(sound.x) / 8.0,
                        f64::from(sound.y) / 8.0,
                        f64::from(sound.z) / 8.0,
                        sound.volume,
                        sound.pitch
                    ))
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(";")
}
fn hit(face: Direction, x: f32, y: f32, z: f32) -> BlockHitResult {
    BlockHitResult {
        block_pos: POS,
        direction: face,
        offset_x: x,
        offset_y: y,
        offset_z: z,
        inside: false,
        world_border_hit: false,
    }
}
#[test]
fn native_bookshelf_hit_pixels_and_analog_slot_values_match() {
    let mut counts = [0, 0];
    for row in vectors().lines() {
        let f: Vec<_> = row.split('\t').collect();
        match f[0] {
            "shelf-hit" => {
                let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
                let face = Direction::VALUES[f[2].parse::<usize>().unwrap()];
                let horizontal = f32::from_bits(u32::from_str_radix(f[3], 16).unwrap());
                let vertical = f32::from_bits(u32::from_str_radix(f[4], 16).unwrap());
                assert_eq!(
                    get_hit_slot(
                        &hit(face, horizontal, vertical, horizontal),
                        super::super::menu::facing(state)
                    )
                    .map_or(-1, |slot| slot as i32),
                    f[5].parse::<i32>().unwrap(),
                    "{row}"
                );
                counts[0] += 1;
            }
            "shelf-signal" => {
                let (mut level, handle) = fixture(0);
                level
                    .block_entity_state_mut(handle)
                    .unwrap()
                    .load_custom_only(
                        &parse_compound(&format!("{{last_interacted_slot:{}}}", f[1])).unwrap(),
                        crate::level::LevelContext::fixture(0, 63)
                            .registry_context()
                            .unwrap(),
                    );
                assert_eq!(
                    BOOKSHELF_BLOCK
                        .analog_output_signal(
                            state(0),
                            POS,
                            Direction::VALUES[f[2].parse::<usize>().unwrap()],
                            &mut level
                        )
                        .unwrap(),
                    f[3].parse::<i32>().unwrap(),
                    "{row}"
                );
                counts[1] += 1;
            }
            _ => {}
        }
    }
    assert_eq!(counts, [6936, 42]);
}
#[test]
fn native_bookshelf_list_hooks_preserve_whole_receivers_and_notification_order() {
    let mut checked = 0;
    for row in vectors()
        .lines()
        .filter(|row| row.starts_with("shelf-hook\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let mask = f[1].parse::<u8>().unwrap();
        let slot = f[2].parse::<usize>().unwrap();
        let action = f[3].parse::<usize>().unwrap();
        let (mut level, handle) = fixture(mask);
        let mut operation = StackOperation::new(&mut level);
        let original = operation.pin(address(handle, slot)).unwrap();
        let was_empty = operation.level.stack(original).unwrap().is_empty();
        let returned = match action {
            0..=4 => {
                let (item, count) = match action {
                    0 => (Item::BOOK, 3),
                    1 => (Item::ENCHANTED_BOOK, 2),
                    2 => (Item::STONE, 1),
                    3 => (Item::BOOK, 0),
                    _ => (Item::AIR, 0),
                };
                set_item_value(
                    handle,
                    slot,
                    StackValue::New(ItemStack::new(item, count, Default::default())),
                    operation.level,
                )
                .unwrap();
                Tag::Compound(Default::default())
            }
            5..=8 => {
                let StackValue::Existing(returned) =
                    remove_item_value(handle, slot, [-7, 0, 1, 99][action - 5], &mut operation)
                        .unwrap()
                else {
                    panic!("whole receiver")
                };
                assert_eq!(returned, original, "{row}");
                if !was_empty {
                    assert_eq!(f[6], "true", "{row}");
                }
                tag(operation.level.stack(returned).unwrap())
            }
            9 => tag(
                &super::super::access::remove_item_no_update(handle, slot, operation.level)
                    .unwrap(),
            ),
            _ => unreachable!(),
        };
        assert_eq!(returned, expected(f[5]), "{row}");
        assert_eq!(saved(operation.level, handle), expected(f[4]), "{row}");
        assert_eq!(
            operation.level.get_block_state(POS).unwrap().id(),
            f[7].parse::<u16>().unwrap(),
            "{row}"
        );
        assert_eq!(trace(operation.level), f[8], "{row}");
        checked += 1;
    }
    assert_eq!(checked, 3840);
}
#[test]
fn native_bookshelf_insert_remove_stats_sounds_and_live_inventory_match() {
    let mut checked = 0;
    for row in vectors()
        .lines()
        .filter(|row| row.starts_with("shelf-use\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let removing = f[1] == "true";
        let slot = f[2].parse::<usize>().unwrap();
        let mask = if f[3] == "true" { 1 << slot } else { 0 };
        let (mut level, handle) = fixture(mask);
        let mut actor = ServerPlayer::new(
            Uuid::new(1, 2),
            "bookshelf".into(),
            1,
            Vec3::new(8.5, 65., 10.5),
            GameType::Survival,
        );
        actor.abilities.instabuild = f[5] == "true";
        let item = match f[4] {
            "0" => Item::BOOK,
            "1" => Item::ENCHANTED_BOOK,
            "2" => Item::STONE,
            _ => Item::AIR,
        };
        actor.set_item_in_hand(
            InteractionHand::MainHand,
            ItemStack::new(item, 3, Default::default()),
        );
        level
            .add_entity(PLAYER, EntityState::Player(Box::new(actor)))
            .unwrap();
        events::take(&mut level);
        let context = UseContext {
            player: PLAYER,
            hand: InteractionHand::MainHand,
            hit: hit(
                if f[6] == "true" {
                    Direction::South
                } else {
                    Direction::North
                },
                1. - ((slot % 3) as f32 + 0.5) / 3.,
                if slot < 3 { 0.75 } else { 0.25 },
                0.,
            ),
        };
        let before = level.get_block_state(POS).unwrap();
        let result = if removing {
            BOOKSHELF_BLOCK.use_without_item(before, POS, &context, &mut level)
        } else {
            BOOKSHELF_BLOCK.use_item_on(before, POS, &context, &mut level)
        }
        .unwrap_or_else(|e| panic!("{row}: {e:?}"));
        let actual = if result == InteractionResult::SUCCESS {
            "success"
        } else if result == InteractionResult::CONSUME {
            "consume"
        } else if result == InteractionResult::TryWithEmptyHand {
            "empty-hand"
        } else if result == InteractionResult::Pass {
            "pass"
        } else {
            panic!("{result:?}")
        };
        assert_eq!(actual, f[7], "{row}");
        assert_eq!(saved(&level, handle), expected(f[8]), "{row}");
        let actor = level.entity::<ServerPlayer>(PLAYER).unwrap();
        assert_eq!(tag(actor.get_main_hand_item()), expected(f[9]), "{row}");
        assert_eq!(
            actor.stats.get_value(Stat::Used(Item::BOOK))
                + actor.stats.get_value(Stat::Used(Item::ENCHANTED_BOOK)),
            f[10].parse::<i32>().unwrap(),
            "{row}"
        );
        assert_eq!(
            level.get_block_state(POS).unwrap().id(),
            f[11].parse::<u16>().unwrap(),
            "{row}"
        );
        for (actual, expected_item) in actor.inventory.items.iter().zip(f[13].split('#')) {
            assert_eq!(tag(actual), expected(expected_item), "{row}");
        }
        assert_eq!(trace(&mut level), f[12], "{row}");
        checked += 1;
    }
    assert_eq!(checked, 384);
}
