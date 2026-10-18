use super::{PathType, walk_node_evaluator};
use crate::{
    Behaviors,
    block::{LevelReaderRef, UpdateFlags},
    level::{Level, LevelContext, LevelError, Region, chunk::LevelChunk},
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager, TagSet, load_tags},
    registries::fluid::Id,
};
use ferraloom_util::{BlockPos, ChunkPos};
use serde_json::Value;
use std::{
    io::Read,
    sync::{Arc, OnceLock},
};

static EMPTY: Behaviors = Behaviors::EMPTY;
fn level() -> Level {
    type Tags = (Arc<TagSet<Block>>, Arc<TagSet<Id>>);
    static TAGS: OnceLock<Tags> = OnceLock::new();
    let (blocks, fluids) = TAGS.get_or_init(|| {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla jar");
        let resources =
            ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
        let blocks = load_tags(&resources, "block", |id, _| {
            Block::from_name(&id.to_string())
        });
        let fluids = load_tags(&resources, "fluid", |id, _| Id::from_name(&id.to_string()));
        assert!(blocks.diagnostics.is_empty());
        assert!(fluids.diagnostics.is_empty());
        (
            Arc::new(TagSet::from_tags(blocks, ferraloom_data::BLOCKS.len()).unwrap()),
            Arc::new(
                TagSet::from_tags(fluids, ferraloom_data::registries::fluid::ALL.len()).unwrap(),
            ),
        )
    });
    let mut level = Level::new(Region::new(&EMPTY), LevelContext::fixture(0, 63));
    level.set_block_tags(blocks.clone());
    level.set_fluid_tags(fluids.clone());
    ensure_chunk(&mut level, BlockPos::new(8, 5, 8));
    level
}
fn ensure_chunk(level: &mut Level, pos: BlockPos) {
    let chunk = ChunkPos::containing(pos);
    if level.chunk(chunk).is_none() {
        level
            .add_chunk(LevelChunk::new(
                chunk,
                0..1,
                ferraloom_chunk::LevelChunkSection::new(
                    ferraloom_chunk::Strategy::block_states(),
                    ferraloom_chunk::Strategy::biomes(Arc::new(
                        ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                    )),
                    ferraloom_chunk::BiomeId(0),
                ),
            ))
            .unwrap();
    }
}
fn pos(value: &Value) -> BlockPos {
    BlockPos::new(
        value[0].as_i64().unwrap() as i32,
        value[1].as_i64().unwrap() as i32,
        value[2].as_i64().unwrap() as i32,
    )
}
fn cached_type(
    level: &mut Level,
    pos: BlockPos,
    reads: &mut Vec<BlockPos>,
) -> Result<PathType, LevelError> {
    if let Some(path_type) = level.path_type_cache().get(pos) {
        return Ok(path_type);
    }
    reads.push(pos);
    let path_type =
        walk_node_evaluator::get_path_type_from_state(&LevelReaderRef::Server(level), pos)?;
    level.path_type_cache_mut().record_computed(pos, path_type);
    Ok(path_type)
}

#[test]
fn native_raw_path_type_for_every_block_state() {
    let mut level = level();
    let pos = BlockPos::new(8, 5, 8);
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/pathfinder/path-types.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    let mut count = 0;
    for line in text.lines() {
        let (state, expected) = line.split_once('\t').unwrap();
        let state = BlockState::from_id(state.parse().unwrap()).unwrap();
        level.set_block_state(pos, state).unwrap();
        let actual =
            walk_node_evaluator::get_path_type_from_state(&LevelReaderRef::Server(&level), pos)
                .unwrap();
        assert_eq!(actual.id(), expected.parse::<u8>().unwrap(), "{state:?}");
        count += 1;
    }
    assert_eq!(count, 35_723);
}

#[test]
fn native_cache_hits_neighbor_staleness_exact_invalidation_and_hash_collisions() {
    let oracle: Value = serde_json::from_str(include_str!(
        "../../../../tests/java/pathfinder/path-type-cache.json"
    ))
    .unwrap();
    let mut level = level();
    let min_y = oracle["min_y"].as_i64().unwrap() as i32;
    for step in oracle["steps"].as_array().unwrap() {
        let p = pos(&step["pos"]);
        ensure_chunk(&mut level, p);
        let mut reads = Vec::new();
        let result = match step["op"].as_str().unwrap() {
            "set" => {
                level
                    .set_block_state(
                        p,
                        BlockState::from_id(step["state"].as_u64().unwrap() as u16).unwrap(),
                    )
                    .unwrap();
                None
            }
            "invalidate" => {
                level.path_type_cache_mut().invalidate(p);
                None
            }
            "raw" => Some(cached_type(&mut level, p, &mut reads).unwrap()),
            "static" => Some(
                walk_node_evaluator::get_path_type_static(p, min_y, |p| {
                    cached_type(&mut level, p, &mut reads)
                })
                .unwrap(),
            ),
            op => panic!("unknown oracle operation {op}"),
        };
        if let Some(result) = result {
            assert_eq!(result.id(), step["type"].as_u64().unwrap() as u8, "{step}");
        }
        let expected: Vec<_> = step["reads"].as_array().unwrap().iter().map(pos).collect();
        assert_eq!(reads, expected, "{step}");
    }
}

#[test]
fn sent_block_update_invalidates_cache_without_a_ready_holder() {
    use crate::level::distance_manager::FullChunkStatus;
    let mut level = level();
    let pos = BlockPos::new(8, 5, 8);
    level
        .set_block_state(pos, Block::STONE.default_state())
        .unwrap();
    assert_eq!(
        cached_type(&mut level, pos, &mut Vec::new()).unwrap(),
        PathType::Blocked
    );
    level
        .set_chunk_loading_status(
            ChunkPos::containing(pos),
            FullChunkStatus::BlockTicking,
            false,
        )
        .unwrap();
    assert!(
        level
            .set_block(
                pos,
                Block::AIR.default_state(),
                UpdateFlags::CLIENTS | UpdateFlags::SKIP_ALL_SIDEEFFECTS,
                512
            )
            .unwrap()
    );
    assert!(level.take_block_update_chunks().is_empty());
    assert_eq!(level.path_type_cache().get(pos), None);
    assert_eq!(
        cached_type(&mut level, pos, &mut Vec::new()).unwrap(),
        PathType::Open
    );
}
