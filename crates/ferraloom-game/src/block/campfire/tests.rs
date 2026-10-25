use super::*;
use crate::{
    Behaviors,
    block::BlockBehavior,
    level::{
        Level, LevelContext, Region,
        chunk::{LevelChunk, LevelChunkSection},
    },
    predicates::{LocationPredicate, PredicateContext},
};
use ferraloom_data::{
    Block,
    datapack::{Pack, ResourceManager, TagSet},
};
use ferraloom_util::{ChunkPos, Vec3};
use std::sync::{Arc, LazyLock};

const POS: BlockPos = BlockPos::new(8, 80, 8);
const VECTORS: &str = include_str!("../../../tests/java/blocks/campfire/smokey.tsv");
static EMPTY: Behaviors = Behaviors::EMPTY;
static TAGS: LazyLock<Arc<TagSet<Block>>> = LazyLock::new(|| {
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla tags");
    Arc::new(
        TagSet::load(&ResourceManager::new(vec![
            Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap(),
        ]))
        .unwrap(),
    )
});
fn fixture(behaviors: &'static Behaviors) -> Level {
    let mut level = Level::new(Region::new(behaviors), LevelContext::fixture(0, 63));
    let biome = ferraloom_chunk::BiomeId(0);
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            -4..20,
            LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
                )),
                biome,
            ),
        ))
        .unwrap();
    level.set_block_tags(Arc::clone(&TAGS));
    level
}
struct ShapeProbe;
impl BlockBehavior for ShapeProbe {
    fn collision_shape(
        &self,
        _: BlockState,
        pos: BlockPos,
        context: &BlockCollisionContext<'_>,
        _: &LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        assert_eq!(
            pos, POS,
            "native collision uses the caller's original position"
        );
        assert!(matches!(context, BlockCollisionContext::Empty(_)));
        Ok(shapes::block())
    }
}
static PROBE: Behaviors = Behaviors::EMPTY.with_block(Block::SCAFFOLDING, &ShapeProbe);
#[test]
fn native_smokey_scan_matches_cached_collision_masks_limit_and_extra_below_read() {
    let mut level = fixture(&EMPTY);
    let mut checked = 0;
    for row in VECTORS.lines() {
        let fields: Vec<_> = row.split('\t').collect();
        if fields[0] == "original-position-empty-context" || fields[0].starts_with("location-") {
            continue;
        }
        for (index, id) in fields[1].split(',').enumerate() {
            level
                .set_block_state(
                    POS.below(index as i32 + 1),
                    BlockState::from_id(id.parse().unwrap()).unwrap(),
                )
                .unwrap();
        }
        let expected = fields[2].parse::<bool>().unwrap();
        assert_eq!(
            is_smokey_pos(&LevelReaderRef::Server(&level), POS).unwrap(),
            expected,
            "{row}"
        );
        assert_eq!(
            PredicateContext::smokey(&level, POS).unwrap(),
            expected,
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 82);
}
#[test]
fn native_smokey_collision_uses_the_original_pos_and_empty_context() {
    let row = VECTORS
        .lines()
        .find(|row| row.starts_with("original-position-empty-context\t"))
        .unwrap();
    let fields: Vec<_> = row.split('\t').collect();
    assert_eq!(fields[3], "1,shape@0:empty,2");
    let mut level = fixture(&PROBE);
    for (index, id) in fields[1].split(',').enumerate() {
        level
            .set_block_state(
                POS.below(index as i32 + 1),
                BlockState::from_id(id.parse().unwrap()).unwrap(),
            )
            .unwrap();
    }
    assert!(
        Block::SCAFFOLDING
            .default_state()
            .collision_shape()
            .is_none()
    );
    assert_eq!(
        is_smokey_pos(&LevelReaderRef::Server(&level), POS).unwrap(),
        fields[2].parse::<bool>().unwrap()
    );
}
#[test]
fn smokey_location_predicate_keeps_required_reads_and_smokey_before_block_matching() {
    let mut level = fixture(&EMPTY);
    level
        .set_block_state(POS, Block::GRASS_BLOCK.default_state())
        .unwrap();
    for row in VECTORS.lines().filter(|row| row.starts_with("location-")) {
        let fields: Vec<_> = row.split('\t').collect();
        let predicate = LocationPredicate::decode(
            &serde_json::from_str::<serde_json::Value>(fields[1]).unwrap(),
        )
        .unwrap();
        assert_eq!(
            predicate
                .matches(Vec3::new(8.5, 80.0, 8.5), &level)
                .unwrap(),
            fields[2].parse::<bool>().unwrap(),
            "{row}"
        );
        assert_eq!(
            fields[3],
            if fields[0] == "location-true" {
                "1,2,3,4,5"
            } else {
                "1,2,3,4,5,0"
            }
        );
    }
    let missing = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    assert!(matches!(
        is_smokey_pos(&LevelReaderRef::Server(&missing), POS),
        Err(LevelError::MissingChunk(ChunkPos { x: 0, z: 0 }))
    ));
    // Missing terrain remains required at the campfire helper boundary.
}
