use super::*;
use crate::{
    Behaviors,
    entity::{EntityData, EntityState, animal::Cow, item::ItemEntity, player::ServerPlayer},
    level::{Level, Region, entity::Visibility},
};
use ferraloom_data::EntityType;
use ferraloom_protocol::chat::{Component, Contents};
use ferraloom_util::ChunkPos;

#[derive(serde::Deserialize)]
struct Definition {
    name: String,
    kind: String,
    uuid: String,
    x: f64,
    alive: bool,
    tag: String,
}
#[derive(serde::Deserialize)]
struct Vector {
    selector: String,
    source_entity: u64,
    players_only: bool,
    selected: Vec<u64>,
}
#[derive(serde::Deserialize)]
struct Vectors {
    entities: Vec<Definition>,
    vectors: Vec<Vector>,
}
struct Query {
    level: Level,
    players: Vec<EntityRef>,
}
impl Query {
    fn state(&self, entity: EntityRef) -> &EntityState {
        self.level.entity_state(entity).unwrap()
    }
}
impl EntityQuery for Query {
    fn candidates(&self, players_only: bool) -> Vec<EntityRef> {
        if players_only {
            self.players.clone()
        } else {
            self.level.all_entities()
        }
    }
    fn candidates_in(&self, players_only: bool, bounds: Option<Aabb>) -> Vec<EntityRef> {
        if players_only {
            self.players.clone()
        } else if let Some(bounds) = bounds {
            self.level.entities_in(bounds, |_, _| true)
        } else {
            self.level.all_entities()
        }
    }
    fn entity_by_uuid(&self, uuid: Uuid, players_only: bool) -> Option<EntityRef> {
        if players_only {
            self.players
                .iter()
                .copied()
                .find(|entity| self.state(*entity).as_entity().uuid == uuid)
        } else {
            self.level.entity_by_uuid(uuid)
        }
    }
    fn info(&self, entity: EntityRef) -> Option<SelectorEntity<'_>> {
        let state = self.level.entity_state(entity).ok()?;
        let base = state.as_entity();
        let name = match state {
            EntityState::Player(player) => player.name.as_str(),
            _ => {
                let component = base
                    .entity_data
                    .get(crate::entity::synched_data::CUSTOM_NAME)
                    .as_ref()
                    .unwrap();
                let Contents::Text(name) = &component.contents else {
                    panic!("fixture custom name must be literal");
                };
                name.as_str()
            }
        };
        Some(SelectorEntity {
            name: name.into(),
            uuid: base.uuid,
            player: matches!(state, EntityState::Player(_)),
            dimension: "minecraft:overworld",
            position: base.position,
            width: f64::from(base.dimensions.width),
            height: f64::from(base.dimensions.height),
        })
    }
    fn bounding_box(&self, entity: EntityRef) -> Option<Aabb> {
        Some(self.state(entity).as_entity().bounding_box)
    }
    fn matches(
        &mut self,
        entity: EntityRef,
        predicate: &SelectorPredicate,
    ) -> Result<bool, CommandSyntaxException> {
        let state = self.state(entity);
        let base = state.as_entity();
        Ok(match predicate {
            SelectorPredicate::Alive => state.as_living().map_or(!base.removed, |living| {
                !base.removed && living.health() > 0.
            }),
            SelectorPredicate::Name { value, inverted } => {
                (self.info(entity).unwrap().name == value.as_str()) != *inverted
            }
            SelectorPredicate::Type {
                value,
                inverted,
                tag,
            } => {
                assert!(!tag);
                (state.entity_type().name() == value) != *inverted
            }
            SelectorPredicate::Tag { value, inverted } => base.tags.contains(value) != *inverted,
            _ => panic!("fixture predicate not expected: {predicate:?}"),
        })
    }
    fn shuffle(&mut self, _: &mut [(EntityRef, f64)]) {
        panic!("mixed order fixture excludes random order");
    }
}
#[test]
fn mixed_native_selectors_use_player_lookup_and_section_iteration() {
    let vectors: Vectors =
        serde_json::from_str(include_str!("../../vanilla/mixed-selector-vectors.json")).unwrap();
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = Level::new(
        Region::new(&BEHAVIORS),
        crate::level::LevelContext::fixture(0, 63),
    );
    for x in 0..3 {
        level
            .update_entity_chunk_status(ChunkPos::new(x, 0), Visibility::Ticking)
            .unwrap();
    }
    for (index, definition) in vectors.entities.into_iter().enumerate() {
        let kind = EntityType::from_name(&definition.kind).unwrap();
        let handle = EntityRef(index as u64 + 1);
        let uuid = parse_uuid(&definition.uuid).unwrap();
        let mut state = match kind {
            EntityType::PLAYER => EntityState::Player(Box::new(ServerPlayer::new(
                uuid,
                definition.name.clone(),
                index as i32 + 1,
                Vec3::ZERO,
                GameType::Survival,
            ))),
            EntityType::COW => EntityState::Cow(Box::new(Cow::with_parent(EntityData::new(
                kind,
                index as i32 + 1,
                0,
            )))),
            EntityType::ITEM => EntityState::Item(Box::new(ItemEntity::new(index as i32 + 1, 0))),
            _ => panic!("unexpected fixture kind"),
        };
        let base = state.as_entity_mut();
        base.uuid = uuid;
        base.set_pos(Vec3::new(definition.x, 0., 0.));
        base.bounding_box = Aabb::new(definition.x - 0.3, 0., -0.3, definition.x + 0.3, 1.8, 0.3);
        base.entity_data.set(
            crate::entity::synched_data::CUSTOM_NAME,
            Some(Box::new(Component::literal(definition.name))),
        );
        base.tags.insert(definition.tag);
        if !definition.alive {
            state.as_living_mut().unwrap().set_health(0.);
        }
        level.add_entity(handle, state).unwrap();
    }
    let mut query = Query {
        level,
        players: vec![EntityRef(5), EntityRef(1)],
    };
    for vector in vectors.vectors {
        let source =
            CommandSourceStack::server(4, Vec3::ZERO).with_entity(EntityRef(vector.source_entity));
        let selector =
            EntitySelector::parse(&mut StringReader::new(&vector.selector), true).unwrap();
        let selected = selector
            .find(&source, &mut query, vector.players_only)
            .unwrap();
        assert_eq!(
            selected
                .into_iter()
                .map(|entity| entity.0)
                .collect::<Vec<_>>(),
            vector.selected,
            "{} (players_only={}, source={})",
            vector.selector,
            vector.players_only,
            vector.source_entity
        );
    }
}
