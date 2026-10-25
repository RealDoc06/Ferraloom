use super::*;
use crate::level::{LevelContext, Region, chunk::LevelChunk};
use ferraloom_worldgen::structure::start::StructureStart;
static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
fn chunk(pos: ChunkPos) -> LevelChunk {
    let section = super::super::chunk::LevelChunkSection::new(
        ferraloom_chunk::Strategy::block_states(),
        ferraloom_chunk::Strategy::biomes(std::sync::Arc::new(
            ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
        )),
        ferraloom_chunk::BiomeId(0),
    );
    LevelChunk::new(pos, 0..16, section)
}
#[test]
fn publication_is_newer_only_dirty_and_keeps_existing_readers_stable() {
    let pos = ChunkPos::new(-2, 3);
    let id = "minecraft:mineshaft".parse().unwrap();
    let mut payload = ChunkStructures::new([-2, 3]);
    payload.starts.insert(id, StructureStart::invalid());
    let mut chunk = chunk(pos);
    chunk.set_structures(payload, 7).unwrap();
    assert!(chunk.try_mark_saved());
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    level.add_chunk(chunk).unwrap();
    let old = level.chunk(pos).unwrap().structures();
    assert_eq!(old.starts.len(), 1);
    for revision in 8..40 {
        assert!(
            level
                .publish_chunk_structures(pos, revision, ChunkStructures::new([-2, 3]))
                .unwrap()
        );
    }
    let chunk = level.chunk(pos).unwrap();
    assert!(chunk.structures().starts.is_empty());
    assert_eq!(chunk.structures_revision(), 39);
    assert_eq!(old.starts.len(), 1);
    assert!(chunk.is_unsaved());
    assert!(level.try_mark_chunk_saved(pos).unwrap());
    assert!(
        !level
            .publish_chunk_structures(pos, 39, ChunkStructures::new([-2, 3]))
            .unwrap()
    );
    assert!(
        !level
            .publish_chunk_structures(pos, 1, ChunkStructures::new([-2, 3]))
            .unwrap()
    );
    assert!(!level.chunk(pos).unwrap().is_unsaved());
    assert_eq!(
        level.publish_chunk_structures(pos, 40, ChunkStructures::new([0, 0])),
        Err(LevelError::InvalidChunkStructures(pos))
    );
    assert_eq!(
        level.publish_chunk_structures(ChunkPos::new(9, 9), 1, ChunkStructures::new([9, 9])),
        Err(LevelError::MissingChunk(ChunkPos::new(9, 9)))
    );
    level.collect_chunk_structure_versions();
    assert_eq!(level.chunk(pos).unwrap().structures.0.len(), 1);
}

use crate::{block::UnportedBehavior, predicates::PredicateContext};
use ferraloom_data::registries::worldgen_structure_type;
use ferraloom_worldgen::structure::{
    BoundingBox, TerrainAdjustment, piece::StructurePieceData, start::StructurePiece,
    structures::buried_treasure::BuriedTreasurePiece,
};

fn start(
    id: &Identifier,
    source: ChunkPos,
    marker: i32,
    valid: bool,
    terrain: TerrainAdjustment,
) -> StructureStart {
    let pieces = if valid {
        [[0, 60, 0, 2, 62, 2], [8, 60, 0, 10, 62, 2]]
            .into_iter()
            .map(|[x, y, z, a, b, c]| {
                StructurePiece::BuriedTreasure(BuriedTreasurePiece {
                    data: StructurePieceData::new(BoundingBox::new([x, y, z], [a, b, c]), 0, None),
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    StructureStart::new(
        id.clone(),
        worldgen_structure_type::Id::BURIED_TREASURE,
        [source.x, source.z],
        marker,
        terrain,
        pieces,
    )
}
fn fixture() -> Level {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let id: Identifier = "minecraft:buried_treasure".parse().unwrap();
    let sources = [
        ChunkPos::new(0, 0),
        ChunkPos::new(1, 0),
        ChunkPos::new(-1, 1),
        ChunkPos::new(2, -2),
        ChunkPos::new(-3, -1),
    ];
    for (index, source) in sources.into_iter().enumerate() {
        let mut payload = ChunkStructures::new([source.x, source.z]);
        if index != 3 {
            payload.starts.insert(
                id.clone(),
                start(
                    &id,
                    source,
                    10 + index as i32,
                    index != 2,
                    TerrainAdjustment::None,
                ),
            );
        }
        if index == 0 {
            payload.references.insert(
                id.clone(),
                StructureReferences::from_array(&sources.map(ChunkPos::pack)),
            );
        }
        let mut chunk = chunk(source);
        chunk.set_structures(payload, 1).unwrap();
        level.add_chunk(chunk).unwrap();
    }
    // Native getChunk serves the negative-coordinate query with an empty chunk.
    level.add_chunk(chunk(ChunkPos::new(-1, 0))).unwrap();
    level
}
#[test]
fn native_reference_start_piece_and_holder_queries_match_vanilla() {
    let level = fixture();
    let id: Identifier = "minecraft:buried_treasure".parse().unwrap();
    let direct = HolderSet::Direct(vec![id.to_string().parse().unwrap()]);
    let tag = HolderSet::Named("minecraft:on_treasure_maps".parse().unwrap());
    let other = HolderSet::Named("minecraft:on_mineshaft_maps".parse().unwrap());
    let origin = ChunkPos::new(0, 0);
    let mut rows = 0;
    for row in include_str!("../../tests/java/level/structure-manager.tsv").lines() {
        let fields: Vec<_> = row.split('\t').collect();
        let manager = level.structure_manager();
        match fields[0] {
            "refs" => assert_eq!(
                manager
                    .get_references_for_structure(origin, &id)
                    .unwrap()
                    .unwrap()
                    .iter()
                    .map(|key| key.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                fields[1]
            ),
            "starts" => assert_eq!(
                manager
                    .starts_for_structure(origin, &id)
                    .unwrap()
                    .iter()
                    .map(|start| start.references.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                fields[1]
            ),
            "start-nbt" => assert_eq!(
                manager
                    .get_start_for_structure(origin, &id)
                    .unwrap()
                    .unwrap()
                    .to_nbt([0, 0])
                    .unwrap(),
                ferraloom_nbt::parse_compound(fields[1]).unwrap()
            ),
            "save-nbt" => assert_eq!(
                level.chunk(origin).unwrap().structures().to_nbt().unwrap(),
                ferraloom_nbt::parse_compound(fields[1]).unwrap()
            ),
            "reload-nbt" => {}
            "query" => {
                let p: Vec<i32> = fields[1].split(',').map(|v| v.parse().unwrap()).collect();
                let pos = BlockPos::new(p[0], p[1], p[2]);
                let actual = [
                    manager.get_structure_at(pos, &id).unwrap().is_some(),
                    manager
                        .get_structure_with_piece_at(pos, &id)
                        .unwrap()
                        .is_some(),
                    manager.get_structure_at_in(pos, &direct).unwrap().is_some(),
                    manager
                        .get_structure_with_piece_at_in(pos, &direct)
                        .unwrap()
                        .is_some(),
                    manager
                        .get_structure_with_piece_at_in(pos, &tag)
                        .unwrap()
                        .is_some(),
                    manager
                        .get_structure_with_piece_at_in(pos, &other)
                        .unwrap()
                        .is_some(),
                ];
                assert_eq!(actual.map(|b| b.to_string()).join(","), fields[2], "{row}");
                assert_eq!(
                    PredicateContext::in_structure(&level, pos, &tag).unwrap(),
                    actual[1],
                    "{row}"
                );
            }
            "empty-value" => {
                let mut payload = ChunkStructures::new([0, 0]);
                payload
                    .references
                    .insert(id.clone(), StructureReferences::default());
                level.publish_chunk_structures(origin, 2, payload).unwrap();
                let manager = level.structure_manager();
                assert_eq!(
                    format!(
                        "{},{}",
                        manager.has_any_structure_at(BlockPos::ZERO).unwrap(),
                        manager.starts_for_structure(origin, &id).unwrap().len()
                    ),
                    fields[1]
                );
            }
            "empty-map" => {
                level
                    .publish_chunk_structures(origin, 3, ChunkStructures::new([0, 0]))
                    .unwrap();
                let manager = level.structure_manager();
                assert_eq!(
                    format!(
                        "{},{}",
                        manager.has_any_structure_at(BlockPos::ZERO).unwrap(),
                        manager
                            .get_structure_with_piece_at_in(BlockPos::new(0, 60, 0), &tag)
                            .unwrap()
                            .is_some()
                    ),
                    fields[1]
                );
            }
            "adapted" => {
                let id: Identifier = "minecraft:pillager_outpost".parse().unwrap();
                let mut payload = ChunkStructures::new([0, 0]);
                payload.starts.insert(
                    id.clone(),
                    start(&id, origin, 20, true, TerrainAdjustment::BeardThin),
                );
                payload
                    .references
                    .insert(id.clone(), StructureReferences::from_array(&[0]));
                level.publish_chunk_structures(origin, 4, payload).unwrap();
                let manager = level.structure_manager();
                let pos = BlockPos::new(4, 50, 1);
                assert_eq!(
                    format!(
                        "{},{}",
                        manager.get_structure_at(pos, &id).unwrap().is_some(),
                        manager
                            .get_structure_with_piece_at(pos, &id)
                            .unwrap()
                            .is_some()
                    ),
                    fields[1]
                );
            }
            _ => panic!("unknown native vector: {row}"),
        }
        rows += 1;
    }
    assert_eq!(rows, 18);
}
#[test]
fn missing_referenced_chunk_is_required_even_when_an_earlier_start_matches() {
    let mut level = fixture();
    let id = "minecraft:buried_treasure".parse().unwrap();
    let pos = BlockPos::new(0, 60, 0);
    let origin = ChunkPos::new(0, 0);
    let set = HolderSet::Named("minecraft:on_treasure_maps".parse().unwrap());
    // Native eagerly materializes all starts before testing any bounding box.
    let references = StructureReferences::from_array(&[0, ChunkPos::new(9, 0).pack()]);
    assert_eq!(references.iter().next(), Some(0));
    let mut payload = ChunkStructures::new([0, 0]);
    payload.starts.insert(
        id,
        start(
            &"minecraft:buried_treasure".parse().unwrap(),
            origin,
            0,
            true,
            TerrainAdjustment::None,
        ),
    );
    payload
        .references
        .insert("minecraft:buried_treasure".parse().unwrap(), references);
    level.publish_chunk_structures(origin, 2, payload).unwrap();
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        level.set_unported_behavior(policy);
        assert!(
            matches!(level.structure_manager().get_structure_with_piece_at_in(pos,&set),Err(LevelError::MissingChunk(p)) if p == ChunkPos::new(9,0))
        );
        assert!(PredicateContext::in_structure(&level, pos, &set).is_err());
    }
    assert_eq!(
        level
            .structure_manager()
            .starts_for_structures(
                origin,
                &HolderSet::Named("minecraft:missing".parse().unwrap())
            )
            .err(),
        Some(LevelError::InvalidStructureHolderSet)
    );
    assert_eq!(
        level
            .structure_manager()
            .starts_for_structures(
                origin,
                &HolderSet::Direct(vec!["minecraft:missing".parse().unwrap()])
            )
            .err(),
        Some(LevelError::InvalidStructureHolderSet)
    );
}
#[test]
fn holder_order_uses_identifier_keys_and_retains_native_reference_value_order() {
    let level = fixture();
    let origin = ChunkPos::new(0, 0);
    let a: Identifier = "minecraft:buried_treasure".parse().unwrap();
    let b: Identifier = "minecraft:mineshaft".parse().unwrap();
    let mut payload = ChunkStructures::new([0, 0]);
    payload.starts.insert(
        a.clone(),
        start(&a, origin, 1, true, TerrainAdjustment::None),
    );
    payload.starts.insert(
        b.clone(),
        start(&b, origin, 2, true, TerrainAdjustment::None),
    );
    for id in [&b, &a] {
        payload
            .references
            .insert(id.clone(), StructureReferences::from_array(&[0]));
    }
    level.publish_chunk_structures(origin, 2, payload).unwrap();
    let set = HolderSet::Direct(vec![
        b.to_string().parse().unwrap(),
        a.to_string().parse().unwrap(),
    ]);
    let manager = level.structure_manager();
    assert_eq!(
        manager
            .starts_for_structures(origin, &set)
            .unwrap()
            .iter()
            .map(|start| start.references)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    let old = manager
        .get_structure_with_piece_at_in(BlockPos::new(1, 61, 1), &set)
        .unwrap()
        .unwrap();
    assert_eq!(old.references, 1);
    level
        .publish_chunk_structures(origin, 3, ChunkStructures::new([0, 0]))
        .unwrap();
    assert_eq!(old.references, 1);
    assert!(
        manager
            .get_structure_with_piece_at_in(BlockPos::new(1, 61, 1), &set)
            .unwrap()
            .is_none()
    );
}

#[test]
fn structure_payload_survives_existing_seed_aware_storage_codec() {
    use ferraloom_data::datapack::{Pack, ResourceManager};
    use ferraloom_worldgen::structure::template::StructureTemplateManager;
    let level = fixture();
    let native = level.chunk(ChunkPos::ZERO).unwrap().structures();
    let saved = native.to_nbt().unwrap();
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    // Buried-treasure pieces carry geometry directly and require no templates.
    let restored = ChunkStructures::load_from_nbt(
        [0, 0],
        level.seed(),
        &saved,
        &level.context().structures.registry,
        &StructureTemplateManager::default(),
        &resources,
        &mut |_| {
            Err(ferraloom_worldgen::Error(
                "no inline features in this fixture".into(),
            ))
        },
    )
    .unwrap();
    let expected = include_str!("../../tests/java/level/structure-manager.tsv")
        .lines()
        .find_map(|row| row.strip_prefix("reload-nbt\t"))
        .map(|tag| ferraloom_nbt::parse_compound(tag).unwrap())
        .unwrap();
    assert_eq!(restored.to_nbt().unwrap(), expected);
    let id: Identifier = "minecraft:buried_treasure".parse().unwrap();
    assert_eq!(
        restored.starts[&id].pieces[0].bounds(),
        native.starts[&id].pieces[0].bounds()
    );
}
#[test]
fn location_predicate_uses_piece_membership_and_loaded_gate() {
    use crate::predicates::LocationPredicate;
    use ferraloom_util::Vec3;
    let level = fixture();
    let predicate =
        LocationPredicate::decode(&serde_json::json!({"structures":"#minecraft:on_treasure_maps"}))
            .unwrap();
    assert!(
        predicate
            .matches(Vec3::new(1.5, 61.0, 1.5), &level)
            .unwrap()
    );
    assert!(
        !predicate
            .matches(Vec3::new(4.5, 61.0, 1.5), &level)
            .unwrap()
    );
    assert!(
        !predicate
            .matches(Vec3::new(160.5, 61.0, 1.5), &level)
            .unwrap()
    );
}

use super::super::acquisition::{
    AcquireDriver, AcquireHandle, AcquireProgress, AcquireResult, ChunkReference, ticket_level,
};
use std::cell::RefCell;

struct ProtoStructuresDriver {
    sources: RefCell<BTreeMap<i64, (u64, Arc<ChunkStructures>)>>,
    unknown: RefCell<Vec<(ChunkPos, i32)>>,
    requested: RefCell<Vec<(ChunkPos, ChunkStatus)>>,
}
impl AcquireDriver for ProtoStructuresDriver {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn is_owner_thread(&self) -> bool {
        true
    }
    fn visible_ticket_level(&self, _: ChunkPos) -> Option<i32> {
        Some(33)
    }
    fn add_unknown_ticket(&self, pos: ChunkPos, status: ChunkStatus) -> Result<(), LevelError> {
        self.unknown.borrow_mut().push((pos, ticket_level(status)));
        Ok(())
    }
    fn run_distance_manager_updates(&self, _: &Level) -> Result<bool, LevelError> {
        panic!("eligible proto fixture needs no distance pass")
    }
    fn request_status(
        &self,
        _: &Level,
        pos: ChunkPos,
        status: ChunkStatus,
    ) -> Result<AcquireHandle, LevelError> {
        self.requested.borrow_mut().push((pos, status));
        Ok(AcquireHandle {
            pos,
            status,
            request_id: status as u64 + 1,
            holder_epoch: self
                .sources
                .borrow()
                .get(&pos.pack())
                .map_or(0, |source| source.0),
        })
    }
    fn acquisition_result(&self, handle: AcquireHandle) -> Result<AcquireResult, LevelError> {
        Ok(AcquireResult::Complete(Some(ChunkReference {
            pos: handle.pos,
            generation: handle.holder_epoch,
            kind: ChunkKind::Proto,
        })))
    }
    fn poll_acquisition_step(&self, _: &Level) -> Result<AcquireProgress, LevelError> {
        panic!("a settled proto future must not execute a FULL callback")
    }
    fn wait_for_progress(&self) -> Result<(), LevelError> {
        panic!("settled proto")
    }
    fn chunk_if_present(&self, _: ChunkPos, _: ChunkStatus) -> Option<ChunkReference> {
        None
    }
    fn chunk_if_present_unchecked(&self, _: ChunkPos, _: ChunkStatus) -> Option<ChunkReference> {
        None
    }
    fn acquire_on_owner(
        &self,
        _: ChunkPos,
        _: ChunkStatus,
        _: bool,
    ) -> Result<Option<ChunkReference>, LevelError> {
        panic!("owner fixture")
    }
    fn proto_chunk_structures(
        &self,
        chunk: ChunkReference,
    ) -> Result<Arc<ChunkStructures>, LevelError> {
        self.sources
            .borrow()
            .get(&chunk.pos.pack())
            .filter(|source| source.0 == chunk.generation)
            .map(|source| Arc::clone(&source.1))
            .ok_or(LevelError::ChunkAcquisition(
                chunk.pos,
                ChunkStatus::StructureStarts,
                AcquireFailure::InvalidHandle,
            ))
    }
}

#[test]
fn proto_queries_acquire_reference_then_source_status_and_retain_source_versions() {
    let query = ChunkPos::ZERO;
    let source = ChunkPos::new(1, 0);
    let id: Identifier = "minecraft:buried_treasure".parse().unwrap();
    let mut references = ChunkStructures::new([0, 0]);
    references.references.insert(
        id.clone(),
        StructureReferences::from_array(&[source.pack()]),
    );
    let make_start = |marker| {
        let mut data = ChunkStructures::new([1, 0]);
        data.starts.insert(
            id.clone(),
            start(&id, source, marker, true, TerrainAdjustment::None),
        );
        Arc::new(data)
    };
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    level
        .install_acquire_driver(
            Box::new(ProtoStructuresDriver {
                sources: RefCell::new(
                    [
                        (query.pack(), (1, Arc::new(references))),
                        (source.pack(), (2, make_start(7))),
                    ]
                    .into(),
                ),
                unknown: RefCell::new(Vec::new()),
                requested: RefCell::new(Vec::new()),
            }),
            0..256,
        )
        .unwrap();
    let pos = BlockPos::new(1, 61, 1);
    let held = level
        .structure_manager()
        .get_structure_with_piece_at(pos, &id)
        .unwrap()
        .unwrap();
    assert_eq!(held.references, 7);
    assert!(level.chunk(query).is_none() && level.chunk(source).is_none());
    let driver = level
        .acquire_driver()
        .unwrap()
        .as_any()
        .downcast_ref::<ProtoStructuresDriver>()
        .unwrap();
    assert_eq!(
        *driver.requested.borrow(),
        [
            (query, ChunkStatus::StructureReferences),
            (source, ChunkStatus::StructureStarts)
        ]
    );
    assert_eq!(*driver.unknown.borrow(), [(query, 36), (source, 44)]);
    driver
        .sources
        .borrow_mut()
        .insert(source.pack(), (2, make_start(8)));
    let newer = level
        .structure_manager()
        .get_structure_with_piece_at(pos, &id)
        .unwrap()
        .unwrap();
    assert_eq!((held.references, newer.references), (7, 8));
    // A cache entry identifies a holder incarnation, not a recreated source.
    driver
        .sources
        .borrow_mut()
        .insert(source.pack(), (3, make_start(9)));
    assert_eq!(
        level
            .chunk_structures(source, ChunkStatus::StructureStarts)
            .unwrap_err(),
        LevelError::ChunkAcquisition(
            source,
            ChunkStatus::StructureStarts,
            AcquireFailure::InvalidHandle
        )
    );
    assert_eq!(held.references, 7);
}
