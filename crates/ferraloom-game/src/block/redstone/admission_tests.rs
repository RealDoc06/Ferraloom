//! Native ListBackedContainer admission over the Level's live item tags.
use crate::{
    block::{
        UnportedBehavior,
        containers::access,
        entity::{self, BlockEntityState},
    },
    entity::player::inventory::set_count,
    level::{Level, LevelContext, LevelError, Region},
};
use ferraloom_data::{
    Block, Item,
    datapack::{Pack, ResourceManager, TagSet},
};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::BlockPos;
use std::{
    io::Read,
    sync::{Arc, LazyLock},
};

static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/redstone/admission.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
});
static TAGS: LazyLock<Vec<Arc<TagSet<Item>>>> = LazyLock::new(|| {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies native item tags");
    let baseline = Arc::new(
        TagSet::load(&ResourceManager::new(vec![
            Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap(),
        ]))
        .unwrap(),
    );
    let mut snapshots = vec![Arc::clone(&baseline)];
    for values in [
        vec!["minecraft:stone"],
        vec![],
        vec!["minecraft:air", "minecraft:book"],
        vec!["minecraft:stone", "minecraft:book"],
    ] {
        let mut patch = Pack::new("live-bookshelf-admission");
        patch.insert(
            "tags/item/bookshelf_books.json".parse().unwrap(),
            serde_json::to_vec(&serde_json::json!({"replace":true,"values":values})).unwrap(),
        );
        snapshots.push(Arc::new(
            TagSet::load(&ResourceManager::new(vec![
                Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap(),
                patch,
            ]))
            .unwrap(),
        ));
    }
    snapshots.push(baseline);
    snapshots
});
fn stack(item: Item, count: i32) -> ItemStack {
    let mut stack = ItemStack::new(item, 1, Default::default());
    set_count(&mut stack, count);
    stack
}
fn level() -> Level {
    // These predicates read services and standalone states, without spawning
    // entities, creating menus or making world callbacks.
    Level::new(
        Region::new(&crate::Behaviors::EMPTY),
        LevelContext::fixture(0, 63),
    )
}

#[test]
fn bookshelf_admission_matches_native_live_tag_reload_capacity_and_retained_empty_receivers() {
    let mut level = level();
    let mut state =
        entity::create(BlockPos::ZERO, Block::CHISELED_BOOKSHELF.default_state()).unwrap();
    let mut current_input = None;
    let mut current_slot = None;
    let mut incoming = ItemStack::default();
    let mut matched = 0;
    for line in VECTORS.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let item = f[0].parse::<u16>().unwrap();
        let count = f[1].parse::<i32>().unwrap();
        if current_input != Some((item, count)) {
            incoming = stack(Item::from_protocol_id(item).unwrap(), count);
            current_input = Some((item, count));
        }
        let held = f[2].parse::<usize>().unwrap();
        let native_slot = f[3].parse::<i32>().unwrap();
        // The invalid negative native index is represented by an invalid usize;
        // rejection must still happen after acceptsItemType's short circuit.
        let slot = usize::try_from(native_slot).unwrap_or(usize::MAX);
        if current_slot != Some((held, slot)) {
            let BlockEntityState::ChiseledBookShelf(shelf) = &mut state else {
                unreachable!()
            };
            shelf.items.fill(ItemStack::default());
            let (held_item, held_count) = [
                (Item::AIR, 0),
                (Item::BOOK, -1),
                (Item::BOOK, 0),
                (Item::BOOK, 1),
                (Item::BOOK, 2),
                (Item::STONE, 1),
                (Item::STONE, 0),
                (Item::STONE, -2),
            ][held];
            shelf.items[if slot < 6 { slot } else { 0 }] = stack(held_item, held_count);
            current_slot = Some((held, slot));
        }
        level.set_item_tags(Arc::clone(&TAGS[f[4].parse::<usize>().unwrap()]));
        let result = match access::can_place_item(&state, slot, &incoming, &level) {
            Ok(allowed) => allowed.to_string(),
            Err(LevelError::Inventory("bookshelf slot out of range")) => "slot-error".into(),
            Err(error) => panic!("unexpected admission error {error:?}: {line}"),
        };
        assert_eq!(result, f[5], "{line}");
        assert_eq!(result, f[6], "native HopperBlockEntity admission: {line}");
        matched += 1;
    }
    assert_eq!(matched, 15360);
}

#[test]
fn tag_admission_requires_the_live_service_without_widening_pure_container_queries() {
    let mut level = level();
    let book = stack(Item::BOOK, 1);
    let shelf = entity::create(BlockPos::ZERO, Block::CHISELED_BOOKSHELF.default_state()).unwrap();
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        level.set_unported_behavior(policy);
        assert_eq!(
            access::can_place_item(&shelf, 0, &book, &level),
            Err(LevelError::MissingItemTags)
        );
    }
    for block in [
        Block::CHEST,
        Block::HOPPER,
        Block::DISPENSER,
        Block::FURNACE,
        Block::CRAFTER,
    ] {
        let receiver = entity::create(BlockPos::ZERO, block.default_state()).unwrap();
        assert_eq!(
            access::can_place_item(&receiver, 0, &book, &level),
            Ok(true),
            "{block:?}"
        );
    }
}
