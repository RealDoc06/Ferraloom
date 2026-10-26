use super::*;
use crate::{
    Behaviors,
    block::{
        containers::storage::BaseContainerBlockEntity,
        entity::{self, chiseled_bookshelf::ChiseledBookShelfBlockEntity},
    },
    level::{
        LevelContext, Region,
        chunk::LevelChunk,
        effects::LevelEffect,
        game_event::test_support::{self as events, Observation},
    },
};
use ferraloom_data::{
    Item,
    datapack::{Pack, ResourceManager, TagSet},
};
use ferraloom_nbt::{Tag, parse_compound};
use std::sync::{Arc, LazyLock};

const POS: BlockPos = BlockPos::new(8, 64, 8);
static BEHAVIORS: Behaviors = Behaviors::EMPTY;
static TAGS: LazyLock<(Arc<TagSet<Block>>, Arc<TagSet<Item>>)> = LazyLock::new(|| {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla jar");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    (
        Arc::new(TagSet::load(&resources).unwrap()),
        Arc::new(TagSet::load(&resources).unwrap()),
    )
});
fn fixture() -> Level {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let biome = ferraloom_chunk::BiomeId(0);
    level
        .add_chunk(LevelChunk::new(
            ferraloom_util::ChunkPos::new(0, 0),
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
    level.set_block_tags(TAGS.0.clone());
    level.set_item_tags(TAGS.1.clone());
    level
}
fn install(level: &mut Level, pos: BlockPos, block: Block) -> BlockEntityRef {
    let state = block.default_state();
    level.set_block_state(pos, state).unwrap();
    level
        .set_block_entity(entity::create(pos, state).unwrap())
        .unwrap();
    level.block_entity_ref(pos).unwrap().unwrap()
}
fn address(handle: BlockEntityRef, slot: usize) -> ContainerSlot {
    ContainerSlot {
        container: ContainerHandle::BlockEntity(handle),
        slot,
    }
}
fn stack_tag(stack: &ItemStack) -> Tag {
    if stack.is_empty() {
        Tag::Compound(Default::default())
    } else {
        stack.to_nbt().unwrap()
    }
}
fn bytes(tag: &Tag) -> Vec<u8> {
    let mut bytes = Vec::new();
    ferraloom_nbt::NbtIo::write_any_tag(&mut bytes, tag).unwrap();
    bytes
}
fn expected(value: &str) -> Tag {
    Tag::Compound(parse_compound(value).unwrap())
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
fn changes(line: &str, y: i32) -> u32 {
    line.split(';')
        .filter(|entry| *entry == format!("changed:{y}"))
        .count() as u32
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
                source,
                affected_state,
                ..
            } => {
                assert!(source.is_none());
                Some(format!(
                    "g:{}:0:{}",
                    event.protocol_id(),
                    affected_state.map_or(-1, |state| i32::from(state.id()))
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(";")
}
fn observable_trace(native: &str) -> String {
    native
        .split(';')
        .filter(|entry| entry.starts_with("u:") || entry.starts_with("g:"))
        .collect::<Vec<_>>()
        .join(";")
}

#[test]
fn native_bookshelf_container_view_and_whole_receiver_transfers() {
    let mut checked = [0, 0, 0];
    for line in include_str!("../../../tests/java/blocks/entities/bookshelf-container.tsv").lines()
    {
        let fields: Vec<_> = line.split('\t').collect();
        let mut level = fixture();
        match fields[0] {
            "shelf-admit" => {
                let handle = install(&mut level, POS, Block::CHISELED_BOOKSHELF);
                let stored = fields[1].parse().unwrap();
                for stack in &mut level
                    .block_entity_mut::<ChiseledBookShelfBlockEntity>(handle)
                    .unwrap()
                    .items
                {
                    *stack = ItemStack::new(Item::BOOK, stored, Default::default());
                }
                let view = access::block_container_at(POS, &level).unwrap().unwrap();
                assert_eq!(view, ContainerRef::Single(handle), "{line}");
                assert_eq!(
                    view.size(&level).unwrap(),
                    fields[3].parse::<usize>().unwrap()
                );
                assert_eq!(
                    view.max_stack_size(&level).unwrap(),
                    fields[4].parse::<i32>().unwrap()
                );
                assert_eq!(
                    view.is_empty(&mut level).unwrap(),
                    fields[5] == "true",
                    "{line}"
                );
                let incoming =
                    ItemStack::new(Item::from_name(fields[2]).unwrap(), 1, Default::default());
                assert_eq!(
                    access::can_place_item(
                        level.block_entity_state(handle).unwrap(),
                        0,
                        &incoming,
                        &level
                    )
                    .unwrap(),
                    fields[6] == "true",
                    "{line}"
                );
                for face in Direction::VALUES {
                    assert_eq!(
                        access::slots_for_face(level.block_entity_state(handle).unwrap(), face),
                        vec![0, 1, 2, 3, 4, 5]
                    );
                }
                checked[0] += 1;
            }
            "shelf-take" => {
                let source = install(&mut level, POS, Block::CHISELED_BOOKSHELF);
                let hopper = install(&mut level, POS.below(1), Block::HOPPER);
                level
                    .block_entity_mut::<ChiseledBookShelfBlockEntity>(source)
                    .unwrap()
                    .items[0] =
                    ItemStack::new(Item::BOOK, fields[1].parse().unwrap(), Default::default());
                for stack in &mut level
                    .block_entity_mut::<BaseContainerBlockEntity>(hopper)
                    .unwrap()
                    .items
                {
                    *stack =
                        ItemStack::new(Item::BOOK, fields[2].parse().unwrap(), Default::default());
                }
                let destination = ContainerRef::Single(hopper);
                events::observe(&mut level);
                events::take(&mut level);
                let mut operation = StackOperation::new(&mut level);
                let original = operation.pin(address(source, 0)).unwrap();
                assert_eq!(
                    access::can_take_item(source, 0, original, &destination, operation.level)
                        .unwrap(),
                    fields[3] == "true",
                    "{line}"
                );
                let before = operation
                    .level
                    .block_entity::<BaseContainerBlockEntity>(hopper)
                    .unwrap()
                    .times_changed;
                assert_eq!(
                    suck_in_items(operation.level, hopper).unwrap(),
                    fields[4] == "true",
                    "{line}"
                );
                assert_eq!(
                    saved(operation.level, source),
                    expected(fields[5]),
                    "{line}"
                );
                assert_eq!(
                    saved(operation.level, hopper),
                    expected(fields[6]),
                    "{line}"
                );
                assert_eq!(
                    stack_tag(operation.level.stack(original).unwrap()),
                    expected(fields[7]),
                    "{line}"
                );
                let current = operation.level.stack_ref(address(hopper, 0)).unwrap();
                assert_eq!(
                    std::ptr::eq(
                        operation.level.stack(original).unwrap(),
                        operation.level.stack(current).unwrap()
                    ),
                    fields[8] == "true",
                    "{line}"
                );
                let after = operation
                    .level
                    .block_entity::<BaseContainerBlockEntity>(hopper)
                    .unwrap()
                    .times_changed;
                assert_eq!(
                    after.wrapping_sub(before),
                    changes(fields[9], POS.y - 1),
                    "{line}"
                );
                assert_eq!(
                    trace(operation.level),
                    observable_trace(fields[9]),
                    "{line}"
                );
                if fields[8] == "true" {
                    set_count(operation.level.stack_mut(original).unwrap(), 2);
                    assert_eq!(count(operation.level.stack(current).unwrap()), 2);
                }
                checked[1] += 1;
            }
            "ordinary-transfer" => {
                // Owned legacy and value-aware entrypoints must produce the
                // same native persisted data, remainder and callback count.
                let mut outcomes = Vec::new();
                for owned in [true, false] {
                    let mut level = fixture();
                    let target = install(
                        &mut level,
                        POS.below(1),
                        Block::from_name(&format!("minecraft:{}", fields[1])).unwrap(),
                    );
                    for stack in &mut level
                        .block_entity_mut::<BaseContainerBlockEntity>(target)
                        .unwrap()
                        .items
                    {
                        *stack = ItemStack::new(
                            Item::STONE,
                            fields[2].parse().unwrap(),
                            Default::default(),
                        );
                    }
                    let before = level
                        .block_entity::<BaseContainerBlockEntity>(target)
                        .unwrap()
                        .times_changed;
                    let item =
                        ItemStack::new(Item::from_name(fields[3]).unwrap(), 7, Default::default());
                    let destination = ContainerRef::Single(target);
                    let remainder = if owned {
                        add_item(None, &destination, item, None, &mut level).unwrap()
                    } else {
                        match add_item_value(
                            None,
                            &destination,
                            StackValue::New(item),
                            None,
                            &mut level,
                        )
                        .unwrap()
                        {
                            StackValue::New(item) => item,
                            _ => unreachable!(),
                        }
                    };
                    let tag = saved(&level, target);
                    let after = level
                        .block_entity::<BaseContainerBlockEntity>(target)
                        .unwrap()
                        .times_changed;
                    assert_eq!(tag, expected(fields[4]), "{line}");
                    assert_eq!(stack_tag(&remainder), expected(fields[5]), "{line}");
                    assert_eq!(
                        after.wrapping_sub(before),
                        changes(fields[6], POS.y - 1),
                        "{line}"
                    );
                    outcomes.push((
                        bytes(&tag),
                        bytes(&stack_tag(&remainder)),
                        after.wrapping_sub(before),
                    ));
                }
                assert_eq!(outcomes[0], outcomes[1], "{line}");
                checked[2] += 1;
            }
            _ => panic!("native vector kind"),
        }
    }
    assert_eq!(checked, [16, 20, 12]);
}
