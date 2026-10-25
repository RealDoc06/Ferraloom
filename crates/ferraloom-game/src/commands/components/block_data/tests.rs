use super::*;
use crate::{
    Behaviors,
    commands::{
        arguments::selectors::{EntityQuery, SelectorEntity, SelectorPredicate},
        components::{ComponentResolver, ResolutionContext, resolve},
        source::Rotation,
    },
    entity::{EntityRef, EntityState},
    level::{LevelContext, Region, chunk::LevelChunk},
};
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::Block;
use ferraloom_protocol::{
    chat::{Component, DataSource},
    codec::VarInt,
    component::{DataComponent, DataComponentMap},
};
use ferraloom_util::{ChunkPos, Vec3};
use std::sync::Arc;

struct Resolver<'a>(&'a Level);
impl EntityQuery for Resolver<'_> {
    fn candidates(&self, _: bool) -> Vec<EntityRef> {
        Vec::new()
    }
    fn info(&self, _: EntityRef) -> Option<SelectorEntity<'_>> {
        None
    }
    fn matches(
        &mut self,
        _: EntityRef,
        _: &SelectorPredicate,
    ) -> Result<bool, CommandSyntaxException> {
        panic!("block NBT vectors have no selectors")
    }
    fn shuffle(&mut self, _: &mut [(EntityRef, f64)]) {
        panic!("block NBT vectors have no selectors")
    }
}
impl ComponentResolver for Resolver<'_> {
    fn nbt_data(
        &mut self,
        source: &CommandSourceStack,
        data: &DataSource,
    ) -> Result<Vec<CompoundTag>, ResolutionError> {
        let DataSource::Block(coordinates) = data else {
            panic!("block NBT vector requires a block source")
        };
        block_data(self.0, source, coordinates)
    }
}
#[derive(serde::Deserialize)]
struct Vector {
    coordinates: String,
    eyes: bool,
    state: String,
    position: [i32; 3],
    data: Vec<String>,
    component: serde_json::Value,
    resolved: serde_json::Value,
}
#[test]
fn loaded_block_entity_metadata_and_component_streams_match_java() {
    let vectors: Vec<Vector> =
        serde_json::from_str(include_str!("../../vanilla/block-data-source-vectors.json")).unwrap();
    assert_eq!(vectors.len(), 12);
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    for vector in vectors {
        let mut level = {
            let mut fixture_level =
                Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
            fixture_level.install_shared(Box::default());
            fixture_level
        };
        let [x, y, z] = vector.position;
        let pos = BlockPos::new(x, y, z);
        let chunk_pos = ChunkPos::containing(pos);
        if vector.state != "unloaded" {
            let mut chunk = LevelChunk::new(
                chunk_pos,
                -2..2,
                LevelChunkSection::new(
                    Strategy::block_states(),
                    Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
                    BiomeId(0),
                ),
            );
            if vector.state == "present" {
                chunk.set_block_state(pos, Block::END_PORTAL.default_state());
            }
            level.add_chunk(chunk).unwrap();
            if vector.state == "present" {
                let mut receiver =
                    crate::block::entity::create(pos, Block::END_PORTAL.default_state()).unwrap();
                receiver.data_mut().components = DataComponentMap {
                    entries: vec![
                        DataComponent::CustomName(Box::new(Component::literal("Stored portal"))),
                        DataComponent::MaxStackSize(VarInt(7)),
                    ],
                };
                level.set_block_entity(receiver).unwrap();
            }
        }
        let entity = EntityRef(1);
        let mut state = EntityState::Cow(Box::default());
        state.as_entity_mut().eye_height = 1.25;
        level.add_entity(entity, state).unwrap();
        let source = CommandSourceStack::server(4, Vec3::new(16.25, 7.75, -8.5))
            .with_entity(entity)
            .with_rotation(Rotation {
                pitch: -19.25,
                yaw: 37.25,
            })
            .with_anchor(if vector.eyes {
                EntityAnchor::Eyes
            } else {
                EntityAnchor::Feet
            });
        let data = block_data(&level, &source, &vector.coordinates).unwrap();
        let expected: Vec<_> = vector
            .data
            .iter()
            .map(|value| ferraloom_nbt::parse_compound(value).unwrap())
            .collect();
        assert_eq!(
            data, expected,
            "{} {} eyes={}",
            vector.state, vector.coordinates, vector.eyes
        );
        let input = Component::from_json(&vector.component.to_string()).unwrap();
        let mut context = ResolutionContext::new(Some(&source));
        let actual = resolve(&mut context, &input, &mut Resolver(&level)).unwrap();
        assert_eq!(
            actual,
            Component::from_json(&vector.resolved.to_string()).unwrap()
        );
        if vector.state == "unloaded" {
            assert!(
                level.chunk(chunk_pos).is_none(),
                "block source cannot acquire chunks"
            );
        }
    }
}
