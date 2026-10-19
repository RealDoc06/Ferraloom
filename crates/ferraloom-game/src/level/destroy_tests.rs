use super::*;
#[cfg(test)]
use crate::level::game_event::test_support::{self as game_events, Observation};
use crate::{
    Behaviors,
    block::{BlockBehavior, BlockMethod, drops::BlockLootContext, entity::create},
    entity::{
        EntityRef, EntityState,
        item::item_entity::ItemEntity,
        player::{GameType, ServerPlayer},
    },
};
use ferraloom_data::{Block, Item};
use ferraloom_protocol::{component::ItemStack, play::clientbound as cb};
use ferraloom_util::{
    Uuid, Vec3,
    random::{LegacyRandomSource, Random, RandomSource},
};

const POS: BlockPos = BlockPos::new(8, 64, 8);
static EMPTY: Behaviors = Behaviors::EMPTY;
fn level(behaviors: &'static Behaviors) -> Level {
    let mut level = Level::new(Region::new(behaviors), LevelContext::fixture(0, 63));
    for x in -1..=1 {
        for z in -1..=1 {
            let pos = ChunkPos::new(x, z);
            let section = chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                )),
                ferraloom_chunk::BiomeId(0),
            );
            level
                .add_chunk(chunk::LevelChunk::new(pos, -4..20, section))
                .unwrap();
            level
                .set_chunk_loading_status(
                    pos,
                    distance_manager::FullChunkStatus::BlockTicking,
                    true,
                )
                .unwrap();
            level
                .update_entity_chunk_status(pos, entity::Visibility::Ticking)
                .unwrap();
        }
    }
    for request in level.take_entity_storage_requests() {
        let entity::EntityStorageRequest::Load { id, .. } = request else {
            unreachable!()
        };
        level.complete_entity_chunk_load(id, Ok(None)).unwrap();
    }
    level.process_pending_entity_loads().unwrap();
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources = ferraloom_data::datapack::ResourceManager::new(vec![
        ferraloom_data::datapack::Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap(),
    ]);
    level.set_block_tags(Arc::new(
        ferraloom_data::datapack::TagSet::load(&resources).unwrap(),
    ));
    level.set_fluid_tags(Arc::new(
        ferraloom_data::datapack::TagSet::load(&resources).unwrap(),
    ));
    drop_fixtures::install(&mut level);
    level.take_effects();
    level
}
fn vectors() -> impl Iterator<Item = (&'static str, &'static str)> {
    include_str!("../../tests/java/level/destroy-block.tsv")
        .lines()
        .map(|line| line.split_once('\t').unwrap())
}
fn bits(value: f64) -> String {
    format!("{:x}", value.to_bits())
}
fn vector(value: Vec3) -> String {
    format!("{},{},{}", bits(value.x), bits(value.y), bits(value.z))
}
fn item_trace(item: &ItemEntity) -> String {
    let stack = item.item().0.as_ref().unwrap();
    format!(
        "item:{}:{}:{}:{}",
        stack.item.name(),
        stack.count,
        vector(item.parent.position),
        if stack.item == Item::DIAMOND {
            vector(item.parent.velocity)
        } else {
            "private-rng".into()
        }
    )
}

#[test]
fn destroy_block_drops_removal_contents_and_level_rng_match_real_server_level() {
    let mut count = 0;
    for (name, expected) in vectors().filter(|(name, _)| !name.starts_with("audience:")) {
        let fields: Vec<_> = name.split(':').collect();
        let block = match fields[0] {
            "planks" => Block::OAK_PLANKS,
            "chest" => Block::CHEST,
            "air" => Block::AIR,
            "water" => Block::WATER,
            "lava" => Block::LAVA,
            "chain" => Block::IRON_CHAIN,
            "fire" => Block::FIRE,
            "stone" => Block::STONE,
            _ => unreachable!(),
        };
        let mut level = level(&EMPTY);
        game_events::observe(&mut level);
        let state = if block == Block::IRON_CHAIN {
            block
                .default_state()
                .with(ferraloom_data::properties::WATERLOGGED, true)
                .unwrap()
        } else {
            block.default_state()
        };
        level.set_block_state(POS, state).unwrap();
        if block == Block::FIRE {
            level
                .set_block_state(
                    POS.relative(ferraloom_util::Direction::Down, 1),
                    Block::STONE.default_state(),
                )
                .unwrap();
        }
        if block == Block::CHEST {
            level.set_block_entity(create(POS, state).unwrap()).unwrap();
            let handle = level.block_entity_ref(POS).unwrap().unwrap();
            level
                .block_entity_state_mut(handle)
                .unwrap()
                .container_mut()
                .unwrap()
                .items[0] = ItemStack::new(Item::DIAMOND, 3, Default::default());
        }
        level.rules_mut().block_drops = Some(fields[2].parse().unwrap());
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(fields[3].parse().unwrap())),
            0,
        );
        game_events::take(&mut level);
        let actual = level
            .destroy_block(POS, fields[1].parse().unwrap(), 7)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let expected: Vec<_> = expected.split('|').collect();
        assert_eq!(actual.to_string(), expected[0], "{name}");
        assert_eq!(
            level.get_block_state(POS).unwrap().id().to_string(),
            expected[1],
            "{name}"
        );
        assert_eq!(level.random().next_int().to_string(), expected[2], "{name}");
        let items = level
            .all_entities()
            .into_iter()
            .filter_map(|id| level.entity::<ItemEntity>(id).ok())
            .map(|item| {
                assert_eq!(
                    item.pickup_delay,
                    if item.item().0.as_ref().unwrap().item == Item::DIAMOND {
                        0
                    } else {
                        10
                    },
                    "{name}"
                );
                item_trace(item)
            })
            .collect::<Vec<_>>();
        let expected_items = expected[3]
            .split(';')
            .filter(|item| item.starts_with("item:"))
            .collect::<Vec<_>>();
        assert_eq!(items, expected_items, "{name}");
        let effects = game_events::take(&mut level)
            .into_iter()
            .filter_map(|effect| match effect {
                Observation::Effect(effects::LevelEffect::PlayerPacket { packet, .. }) => {
                    match *packet {
                        cb::Packet::LevelEvent(event) => {
                            Some(format!("event:{}:none:{}", event.event_type, event.data))
                        }
                        _ => None,
                    }
                }
                Observation::GameEvent {
                    event,
                    source,
                    affected_state,
                    ..
                } => {
                    assert_eq!(source, None);
                    Some(format!(
                        "game:{}:none:{}",
                        event.name(),
                        affected_state.unwrap().id()
                    ))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            effects,
            expected[3]
                .split(';')
                .filter(|item| item.starts_with("event:") || item.starts_with("game:"))
                .collect::<Vec<_>>(),
            "{name}"
        );
        count += 1;
    }
    assert_eq!(count, 30);
}

#[test]
fn destroy_particles_audience_matches_vanilla_and_breaker_is_only_game_event_context() {
    for (name, expected) in vectors().filter(|(name, _)| name.starts_with("audience:")) {
        let mut level = level(&EMPTY);
        game_events::observe(&mut level);
        let mut players = Vec::new();
        for (index, offset) in [0.0, 63.999, 64.0, -64.0].into_iter().enumerate() {
            let handle = level.next_entity_ref();
            let position = Vec3::new(8.0 + offset, 64.0, 8.0);
            let name = format!("player{index}");
            let player = ServerPlayer::new(
                Uuid::offline_player(&name),
                name,
                level.next_entity_id(),
                position,
                GameType::Survival,
            );
            level
                .add_entity(handle, EntityState::Player(Box::new(player)))
                .unwrap();
            players.push(handle);
        }
        level
            .set_block_state(POS, Block::STONE.default_state())
            .unwrap();
        game_events::take(&mut level);
        match name {
            "audience:destroy" => {
                level
                    .destroy_block_with_breaker(POS, false, Some(players[0]), 7)
                    .unwrap();
            }
            "audience:player-particles" => level
                .required_block_behavior(Block::STONE, BlockMethod::SpawnDestroyByEntityParticles)
                .unwrap()
                .spawn_destroy_by_entity_particles(
                    Block::STONE.default_state(),
                    POS,
                    Some(players[0]),
                    &mut level,
                )
                .unwrap(),
            operation => {
                let block = if operation == "audience:straw-particles" {
                    Block::STRAW_BED
                } else {
                    Block::FIRE
                };
                level
                    .required_block_behavior(block, BlockMethod::SpawnDestroyParticles)
                    .unwrap()
                    .spawn_destroy_particles(block.default_state(), POS, &mut level)
                    .unwrap();
            }
        }
        let mut actual = Vec::new();
        for effect in game_events::take(&mut level) {
            match effect {
                Observation::Effect(effects::LevelEffect::PlayerPacket { recipients, packet }) => {
                    if let cb::Packet::LevelEvent(event) = *packet {
                        assert!(!event.global_event);
                        for player in recipients {
                            actual.push(format!(
                                "{}:{}",
                                players.iter().position(|id| *id == player).unwrap(),
                                event.event_type
                            ));
                        }
                    }
                }
                Observation::GameEvent {
                    source,
                    affected_state,
                    ..
                } => {
                    assert_eq!(source, Some(players[0]));
                    assert_eq!(affected_state, Some(Block::STONE.default_state()));
                }
                _ => (),
            }
        }
        assert_eq!(actual.join(","), expected, "{name}");
    }
}

struct ReplaceDuringDrops;
impl BlockBehavior for ReplaceDuringDrops {
    fn get_drops(
        &self,
        _: BlockState,
        context: BlockLootContext,
        level: &mut Level,
    ) -> Result<Vec<ItemStack>, LevelError> {
        assert!(context.params.tool.as_ref().unwrap().is_empty());
        assert_eq!(context.params.this_entity, Some(EntityRef(123)));
        let original = context.block_entity.unwrap();
        assert_eq!(original, level.block_entity_ref(POS)?.unwrap());
        level.set_block_entity(create(POS, Block::CHEST.default_state())?)?;
        assert_ne!(Some(original), level.block_entity_ref(POS)?);
        assert!(level.block_entity_state(original)?.data().removed);
        level.set_block_state(POS, Block::WATER.default_state())?;
        Ok(Vec::new())
    }
}
#[test]
fn destroy_block_keeps_original_receiver_and_reads_fluid_after_reentrant_drops() {
    static REPLACE: ReplaceDuringDrops = ReplaceDuringDrops;
    static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_block(Block::CHEST, &REPLACE);
    let mut level = level(&BEHAVIORS);
    game_events::observe(&mut level);
    level
        .set_block_state(POS, Block::CHEST.default_state())
        .unwrap();
    level
        .set_block_entity(create(POS, Block::CHEST.default_state()).unwrap())
        .unwrap();
    assert!(
        !level
            .destroy_block_with_breaker(POS, true, Some(EntityRef(123)), 7)
            .unwrap()
    );
    assert_eq!(level.get_block_state(POS).unwrap().block(), Block::WATER);
    assert!(
        !game_events::take(&mut level)
            .iter()
            .any(|effect| matches!(effect, Observation::GameEvent { .. }))
    );
}

struct ShapeBudget;
impl BlockBehavior for ShapeBudget {
    fn update_indirect_neighbor_shapes(
        &self,
        _: BlockState,
        pos: BlockPos,
        flags: crate::block::UpdateFlags,
        limit: i32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        assert_eq!(flags, crate::block::UpdateFlags::CLIENTS);
        level.emit(effects::LevelEffect::LevelEvent {
            pos,
            id: 999,
            data: limit,
        });
        Ok(())
    }
}
#[test]
fn destroy_block_forwards_update_limit_and_required_drop_errors_in_both_policies() {
    static SHAPES: ShapeBudget = ShapeBudget;
    static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_block(Block::AIR, &SHAPES);
    for limit in [0, 1, 7] {
        let mut level = level(&BEHAVIORS);
        game_events::observe(&mut level);
        level
            .set_block_state(POS, Block::OAK_PLANKS.default_state())
            .unwrap();
        assert!(level.destroy_block(POS, false, limit).unwrap());
        let budgets = game_events::take(&mut level)
            .into_iter()
            .filter_map(|effect| match effect {
                Observation::Effect(effects::LevelEffect::LevelEvent { id: 999, data, .. }) => {
                    Some(data)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(budgets, if limit > 0 { vec![limit - 1] } else { vec![] });
    }
    for policy in [
        crate::block::UnportedBehavior::Strict,
        crate::block::UnportedBehavior::Warn,
    ] {
        let mut level = level(&EMPTY);
        game_events::observe(&mut level);
        level.set_unported_behavior(policy);
        level.rules_mut().block_drops = None;
        level
            .set_block_state(POS, Block::OAK_PLANKS.default_state())
            .unwrap();
        assert_eq!(
            level.destroy_block(POS, true, 7),
            Err(LevelError::UnsupportedGameRule("minecraft:block_drops"))
        );
        assert_eq!(
            level.get_block_state(POS).unwrap().block(),
            Block::OAK_PLANKS
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        let effects = game_events::take(&mut level);
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Observation::Effect(effects::LevelEffect::PlayerPacket { .. })
        )));
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Observation::GameEvent { .. }))
        );
    }
}
