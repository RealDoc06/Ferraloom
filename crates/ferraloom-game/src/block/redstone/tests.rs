use super::*;
#[cfg(test)]
use crate::level::game_event::test_support::{self as game_events, Observation};
use crate::{
    Behaviors,
    block::{LevelReaderRef, UPDATE_LIMIT, UpdateFlags},
    level::{
        Level, Region,
        chunk::{LevelChunk, LevelChunkSection},
        distance_manager::FullChunkStatus,
        effects::LevelEffect,
    },
};
use ferraloom_data::BlockState;
use ferraloom_util::{
    BlockPos, ChunkPos, Direction,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::sync::Arc;
const ORIGIN: BlockPos = BlockPos::new(8, 0, 8);
static EMPTY: Behaviors = Behaviors::EMPTY;
fn level() -> Level {
    level_in_radius(1)
}
fn level_in_radius(radius: i32) -> Level {
    level_with_behaviors(radius, &EMPTY)
}
pub(super) fn level_with_behaviors(radius: i32, behaviors: &'static Behaviors) -> Level {
    let mut level = Level::new(
        Region::new(behaviors),
        crate::level::LevelContext::fixture(0, 63),
    );
    for x in -radius..=radius {
        for z in -radius..=radius {
            let pos = ChunkPos::new(x, z);
            level
                .add_chunk(LevelChunk::new(
                    pos,
                    -4..20,
                    LevelChunkSection::new(
                        ferraloom_chunk::Strategy::block_states(),
                        ferraloom_chunk::Strategy::biomes(Arc::new(
                            ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                        )),
                        ferraloom_chunk::BiomeId(0),
                    ),
                ))
                .unwrap();
            level
                .set_chunk_loading_status(pos, FullChunkStatus::BlockTicking, true)
                .unwrap();
            level
                .update_entity_chunk_status(pos, crate::level::entity::Visibility::Ticking)
                .unwrap();
            level.set_chunk_block_ticking(pos, true).unwrap();
        }
    }
    for request in level.take_entity_storage_requests() {
        if let crate::level::entity::EntityStorageRequest::Load { id, .. } = request {
            level.complete_entity_chunk_load(id, Ok(None)).unwrap();
        }
    }
    level.process_pending_entity_loads().unwrap();
    static BLOCK_TAGS: std::sync::LazyLock<Arc<ferraloom_data::datapack::TagSet<Block>>> =
        std::sync::LazyLock::new(|| {
            use ferraloom_data::datapack::{Pack, ResourceManager, TagSet};
            let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla jar");
            let resources =
                ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
            Arc::new(TagSet::load(&resources).unwrap())
        });
    level.set_block_tags(Arc::clone(&BLOCK_TAGS));
    crate::level::drop_fixtures::install(&mut level);
    level.set_random_streams(Random::Legacy(LegacyRandomSource::new(0)), 0);
    level
}
fn cases(prefix: &str) -> impl Iterator<Item = (&str, &str)> {
    include_str!("../../../tests/java/blocks/redstone/vectors.tsv")
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(move |(name, _)| name.starts_with(prefix))
}
fn positions(positions: &[BlockPos]) -> String {
    positions
        .iter()
        .map(|p| format!("{}, {}, {}", p.x, p.y, p.z))
        .collect::<Vec<_>>()
        .join(";")
}
fn snapshot(level: &Level, positions: &[BlockPos]) -> String {
    positions
        .iter()
        .map(|p| {
            format!(
                "{}, {}, {}={}",
                p.x,
                p.y,
                p.z,
                level.get_block_state(*p).unwrap().id()
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn state_mutations(level: &mut Level) -> String {
    level
        .take_effects()
        .into_iter()
        .filter_map(|effect| match effect {
            LevelEffect::BlockUpdated { pos, new, .. } => {
                Some(format!("{}, {}, {}={}", pos.x, pos.y, pos.z, new.id()))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(";")
}
fn native_mutations(value: &str) -> String {
    value
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.split_once('/').unwrap().0)
        .collect::<Vec<_>>()
        .join(";")
}

#[test]
fn repeater_delays_locks_priorities_and_tick_dedup_match_vanilla() {
    use crate::block::entity::{BlockEntityData, BlockEntityState, BlockEntityType};
    use ferraloom_data::properties as p;
    for (name, expected) in cases("repeater-schedule:") {
        let mut args = name.split(':').skip(1);
        let state = BlockState::from_id(args.next().unwrap().parse().unwrap()).unwrap();
        let input = args.next().unwrap().parse::<bool>().unwrap();
        let side = args.next().unwrap().parse::<u8>().unwrap();
        let output = args.next().unwrap().parse::<u8>().unwrap();
        let direction = state::facing(state);
        let side_direction = direction.clockwise();
        let side_state = match side {
            1 => Block::REDSTONE_BLOCK.default_state(),
            2..=4 => state::with_facing(
                Block::REPEATER.default_state(),
                if side == 3 {
                    side_direction.opposite()
                } else {
                    side_direction
                },
            )
            .with(p::POWERED, side != 4)
            .unwrap(),
            5 => state::with_facing(Block::COMPARATOR.default_state(), side_direction)
                .with(p::POWERED, true)
                .unwrap(),
            _ => Block::AIR.default_state(),
        };
        let output_state = if output == 0 {
            Block::AIR.default_state()
        } else {
            state::with_facing(
                Block::REPEATER.default_state(),
                if output == 1 {
                    direction.opposite()
                } else {
                    direction
                },
            )
        };
        let mut level = level();
        level.set_game_time(37);
        for (pos, state) in [
            (ORIGIN, state),
            (ORIGIN.below(1), Block::STONE.default_state()),
            (
                ORIGIN.relative(direction, 1),
                if input {
                    Block::REDSTONE_BLOCK
                } else {
                    Block::AIR
                }
                .default_state(),
            ),
            (ORIGIN.relative(side_direction, 1), side_state),
            (ORIGIN.relative(direction.opposite(), 1), output_state),
        ] {
            level.set_block_state(pos, state).unwrap();
        }
        if side == 5 {
            let pos = ORIGIN.relative(side_direction, 1);
            let mut entity = comparator::ComparatorBlockEntity::new(
                BlockEntityData::new(BlockEntityType::COMPARATOR, pos, side_state).unwrap(),
            );
            entity.output = 7;
            level
                .set_block_entity(BlockEntityState::Comparator(Box::new(entity)))
                .unwrap();
        }
        let locked = diode::locked(&LevelReaderRef::Server(&level), ORIGIN, state).unwrap();
        level.take_effects();
        level.neighbor_changed(ORIGIN, Block::STONE, None).unwrap();
        level.neighbor_changed(ORIGIN, Block::STONE, None).unwrap();
        level.set_game_time(38);
        level
            .set_block_state(
                ORIGIN.relative(direction, 1),
                if input {
                    Block::AIR
                } else {
                    Block::REDSTONE_BLOCK
                }
                .default_state(),
            )
            .unwrap();
        level.neighbor_changed(ORIGIN, Block::STONE, None).unwrap();
        level
            .schedule_block_tick_after(ORIGIN.above(3), Block::STONE, 19)
            .unwrap();
        let changes = state_mutations(&mut level);
        let mut data = ferraloom_storage::SerializableChunkData::default();
        level
            .save_chunk_ticks(ChunkPos::containing(ORIGIN), &mut data)
            .unwrap();
        let ticks = data
            .block_ticks
            .unwrap()
            .into_iter()
            .map(|tick| {
                let block =
                    Block::from_name(&tick.id.unwrap().to_string_checked().unwrap()).unwrap();
                format!(
                    "{}, {}, {}/{}/{}/{}",
                    tick.x.unwrap(),
                    tick.y.unwrap(),
                    tick.z.unwrap(),
                    block.protocol_id(),
                    38 + tick.delay.unwrap(),
                    tick.priority.unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let mut expected = expected.split('|');
        assert_eq!(locked.to_string(), expected.next().unwrap(), "{name} lock");
        assert_eq!(ticks.join(";"), expected.next().unwrap(), "{name} ticks");
        assert_eq!(
            changes,
            native_mutations(expected.next().unwrap()),
            "{name} changes"
        );
    }
}

#[test]
fn target_placement_preserves_scheduled_pulses_and_resets_unscheduled_power() {
    use ferraloom_data::properties as p;
    for (name, expected) in cases("target-place:") {
        let mut args = name.split(':').skip(1);
        let power = args.next().unwrap().parse().unwrap();
        let scheduled = args.next().unwrap().parse::<bool>().unwrap();
        let same = args.next().unwrap().parse::<bool>().unwrap();
        let mut level = level();
        let state = Block::TARGET.default_state().with(p::POWER, power).unwrap();
        level.set_block_state(ORIGIN, state).unwrap();
        if scheduled {
            level
                .schedule_block_tick_after(ORIGIN, Block::TARGET, 8)
                .unwrap();
        }
        level.take_effects();
        level
            .required_block_behavior(Block::TARGET, BlockMethod::OnPlace)
            .unwrap()
            .on_place(
                state,
                ORIGIN,
                if same { Block::TARGET } else { Block::AIR }.default_state(),
                false,
                &mut level,
            )
            .unwrap();
        let mut expected = expected.split('|');
        assert_eq!(
            level.get_block_state(ORIGIN).unwrap().id().to_string(),
            expected.next().unwrap(),
            "{name} state"
        );
        assert_eq!(
            state_mutations(&mut level),
            native_mutations(expected.next().unwrap()),
            "{name} state changes"
        );
        assert_eq!(
            level
                .has_scheduled_block_tick(ORIGIN, Block::TARGET)
                .to_string(),
            expected.next().unwrap(),
            "{name} pulse scheduling"
        );
    }
}

#[test]
fn piston_push_reactions_entities_height_and_border_rules_match_vanilla() {
    let mut level = level();
    let mut border = crate::level::border::WorldBorder::default();
    border.set_center(8.0, 8.0);
    border.set_size(10.0);
    level.set_entity_world_border(border);
    for (name, expected) in cases("piston-pushable:") {
        let mut args = name.split(':').skip(1);
        let state = BlockState::from_id(args.next().unwrap().parse().unwrap()).unwrap();
        let direction = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let connection = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let allow = args.next().unwrap().parse().unwrap();
        let pos = BlockPos::new(
            args.next().unwrap().parse().unwrap(),
            args.next().unwrap().parse().unwrap(),
            args.next().unwrap().parse().unwrap(),
        );
        assert_eq!(
            piston::is_pushable(state, &level, pos, direction, allow, connection).to_string(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn pressure_plate_sensitivity_weighted_rounding_and_release_ticks_match_vanilla() {
    use crate::entity::{
        EntityData, EntityRef, EntityState,
        item::PrimedTnt,
        player::{GameType, ServerPlayer},
    };
    use ferraloom_data::properties as p;
    use ferraloom_util::{Uuid, Vec3};
    for (name, expected) in cases("plate:") {
        let mut args = name.split(':').skip(1);
        let block = Block::from_protocol_id(args.next().unwrap().parse().unwrap()).unwrap();
        let kind = args.next().unwrap().parse::<u8>().unwrap();
        let count = args.next().unwrap().parse::<u64>().unwrap();
        let mut level = level();
        game_events::observe(&mut level);
        level
            .set_block_state(ORIGIN.below(1), Block::STONE.default_state())
            .unwrap();
        let state = if block.class() == BlockClass::WeightedPressurePlateBlock {
            block
                .default_state()
                .with(p::POWER, if count == 0 { 15 } else { 0 })
                .unwrap()
        } else {
            block.default_state().with(p::POWERED, count == 0).unwrap()
        };
        level.set_block_state(ORIGIN, state).unwrap();
        let mut actors = Vec::new();
        for i in 0..count {
            let position = Vec3::new(
                f64::from(ORIGIN.x) + 0.5,
                f64::from(ORIGIN.y) + 0.1,
                f64::from(ORIGIN.z) + 0.5,
            );
            let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, i + 1);
            let mut entity = if kind == 0 {
                EntityState::PrimedTnt(Box::new(PrimedTnt::new(i as i32 + 1, 123 + i as i64)))
            } else {
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(1, i as i64 + 2),
                    format!("plate{i}"),
                    i as i32 + 1,
                    position,
                    if kind == 2 {
                        GameType::Spectator
                    } else {
                        GameType::Survival
                    },
                )))
            };
            entity.as_entity_mut().uuid = Uuid::new(1, i as i64 + 2);
            entity.as_entity_mut().set_pos(position);
            level.add_entity(id, entity).unwrap();
            actors.push(id);
        }
        let mut samples = Vec::new();
        for tick in 0..=24 {
            level.set_game_time(tick);
            game_events::take(&mut level);
            if tick == 0 {
                if count == 0 {
                    switches::SWITCHES.tick(state, ORIGIN, &mut level).unwrap();
                } else {
                    switches::SWITCHES
                        .entity_inside(state, ORIGIN, actors[0], &mut level)
                        .unwrap();
                }
            }
            if tick == 7 {
                for &actor in &actors {
                    level.remove_entity(actor).unwrap();
                }
            }
            level.tick_scheduled_blocks(tick, 65536).unwrap();
            let mut mutations = Vec::new();
            let mut gameplay = Vec::new();
            for effect in game_events::take(&mut level) {
                match effect {
                    Observation::Effect(LevelEffect::BlockUpdated { pos, new, .. }) => {
                        mutations.push(format!("{}, {}, {}={}", pos.x, pos.y, pos.z, new.id()));
                    }
                    Observation::GameEvent {
                        event,
                        source,
                        affected_state,
                        ..
                    } => {
                        gameplay.push(format!(
                            "G:{}:{}:{}",
                            event.protocol_id(),
                            source
                                .map_or(0, |id| level.entity::<EntityData>(id).unwrap().network_id),
                            affected_state.map_or(-1, |state| i32::from(state.id()))
                        ));
                    }
                    _ => {}
                }
            }
            samples.push(format!(
                "{}/{}/{}/{}",
                level.get_block_state(ORIGIN).unwrap().id(),
                level.has_scheduled_block_tick(ORIGIN, block),
                mutations.join(";"),
                gameplay.join(";")
            ));
        }
        let expected = expected
            .split('|')
            .map(|sample| {
                let mut parts = sample.splitn(3, '/');
                let state = parts.next().unwrap();
                let scheduled = parts.next().unwrap();
                // Mutation records contain their own /flags suffixes. The
                // final slash separates those records from game events.
                let (mutations, gameplay) = parts.next().unwrap().rsplit_once('/').unwrap();
                format!(
                    "{}/{}/{}/{}",
                    state,
                    scheduled,
                    native_mutations(mutations),
                    gameplay
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(samples.join("|"), expected, "{name}");
    }
}

#[test]
fn switches_and_openings_keep_native_click_results_updates_and_event_sources() {
    use crate::{
        block::{InteractionResult, UseContext},
        entity::{
            EntityData, EntityRef, EntityState,
            player::{GameType, ServerPlayer},
        },
    };
    use ferraloom_data::{properties as p, registries::sound_event::Id as SoundEvent};
    use ferraloom_protocol::{
        codec::Holder,
        play::{BlockHitResult, InteractionHand, clientbound::Packet},
    };
    use ferraloom_util::{Uuid, Vec3};
    for (name, expected) in cases("use:") {
        let mut args = name.split(':').skip(1);
        let state = BlockState::from_id(args.next().unwrap().parse().unwrap()).unwrap();
        let yaw = args.next().unwrap().parse().unwrap();
        let mut level = level();
        game_events::observe(&mut level);
        level
            .set_block_state(ORIGIN.below(1), Block::STONE.default_state())
            .unwrap();
        let block = state.block();
        if block.is_instance_of(BlockClass::DoorBlock) {
            level
                .set_block_state(
                    ORIGIN.above(1),
                    state.with(p::HALF, p::Half::Upper).unwrap(),
                )
                .unwrap();
        }
        if matches!(
            block.class(),
            BlockClass::LeverBlock | BlockClass::ButtonBlock
        ) {
            level
                .set_block_state(
                    ORIGIN.relative(sources::connected_direction(state).opposite(), 1),
                    Block::STONE.default_state(),
                )
                .unwrap();
        }
        level.set_block_state(ORIGIN, state).unwrap();
        let actor = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        let player = ServerPlayer::new(
            Uuid::new(1, 2),
            "redstone".into(),
            1,
            Vec3::new(
                ORIGIN.x as f64 + 0.5,
                ORIGIN.y as f64 + 0.5,
                ORIGIN.z as f64 + 0.5,
            ),
            GameType::Survival,
        );
        level
            .add_entity(actor, EntityState::Player(Box::new(player)))
            .unwrap();
        level.entity_mut::<EntityData>(actor).unwrap().yaw = yaw;
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(123)), 0);
        let context = UseContext {
            player: actor,
            hand: InteractionHand::MainHand,
            hit: BlockHitResult {
                block_pos: ORIGIN,
                direction: Direction::Up,
                offset_x: 0.5,
                offset_y: 0.5,
                offset_z: 0.5,
                inside: false,
                world_border_hit: false,
            },
        };
        for (click, expected) in expected.split('#').enumerate() {
            game_events::take(&mut level);
            let current = level.get_block_state(ORIGIN).unwrap();
            let result = level
                .required_block_behavior(block, BlockMethod::UseWithoutItem)
                .unwrap()
                .use_without_item(current, ORIGIN, &context, &mut level)
                .unwrap();
            let result = if result == InteractionResult::SUCCESS {
                "success"
            } else if result == InteractionResult::CONSUME {
                "consume"
            } else if result == InteractionResult::Pass {
                "pass"
            } else {
                panic!("unexpected result {result:?}");
            };
            let mut mutations = Vec::new();
            let mut gameplay = Vec::new();
            let mut sounds = Vec::new();
            for effect in game_events::take(&mut level) {
                match effect {
                    Observation::Effect(LevelEffect::BlockUpdated { pos, new, .. }) => {
                        mutations.push(format!("{}, {}, {}={}", pos.x, pos.y, pos.z, new.id()));
                    }
                    Observation::GameEvent {
                        event,
                        source,
                        affected_state,
                        ..
                    } => {
                        gameplay.push(format!(
                            "G:{}:{}:{}",
                            event.protocol_id(),
                            source
                                .map_or(0, |id| level.entity::<EntityData>(id).unwrap().network_id),
                            affected_state.map_or(-1, |state| i32::from(state.id()))
                        ));
                    }
                    Observation::Effect(LevelEffect::PlayerPacket { packet, recipients }) => {
                        if let Packet::Sound(sound) = *packet {
                            let Holder::Reference(id) = sound.sound.0 else {
                                panic!("registered block sound");
                            };
                            sounds.push(format!(
                                "{}:{}:{}:{}:{}",
                                SoundEvent::from_name(id.as_str()).unwrap().protocol_id(),
                                sound.source as i32,
                                sound.volume.to_bits(),
                                sound.pitch.to_bits(),
                                i32::from(!recipients.contains(&actor))
                            ));
                        }
                    }
                    _ => {}
                }
            }
            let actual = format!(
                "{result}|{},{}|{}|{}|{}|{}|{}",
                level.get_block_state(ORIGIN).unwrap().id(),
                level.get_block_state(ORIGIN.above(1)).unwrap().id(),
                mutations.join(";"),
                gameplay.join(";"),
                sounds.join(";"),
                level.has_scheduled_block_tick(ORIGIN, block),
                level.random().next_long()
            );
            let mut expected = expected.split('|').map(str::to_owned).collect::<Vec<_>>();
            expected[2] = native_mutations(&expected[2]);
            assert_eq!(actual, expected.join("|"), "{name} click {click}");
        }
    }
}

fn note_sounds(level: &mut Level) -> String {
    use ferraloom_data::registries::sound_event::Id as SoundEvent;
    use ferraloom_protocol::{codec::Holder, play::clientbound::Packet};
    game_events::take(level)
        .into_iter()
        .filter_map(|effect| {
            if let Observation::Effect(LevelEffect::PlayerPacket { packet, .. }) = effect
                && let Packet::Sound(sound) = *packet
            {
                let Holder::Reference(id) = sound.sound.0 else {
                    panic!("registered note sound")
                };
                assert_eq!(
                    (sound.x, sound.y, sound.z),
                    ((ORIGIN.x * 8) + 4, (ORIGIN.y * 8) + 4, (ORIGIN.z * 8) + 4)
                );
                Some(format!(
                    "{}:{}:{}:{}:{}",
                    SoundEvent::from_name(id.as_str()).unwrap().protocol_id(),
                    sound.source as i32,
                    sound.volume.to_bits(),
                    sound.pitch.to_bits(),
                    sound.seed
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}
fn note_instrument(name: &str) -> ferraloom_data::properties::Instrument {
    use ferraloom_data::{PropertyValue, properties::Instrument};
    (0..=u8::MAX)
        .filter_map(Instrument::from_index)
        .find(|v| v.as_str() == name)
        .unwrap()
}
#[test]
fn redstone_post_placement_updates_match_vanilla() {
    use ferraloom_data::properties as p;
    for (name, expected) in cases("placed-by:") {
        let mut args = name.split(':').skip(1);
        let state = BlockState::from_id(args.next().unwrap().parse().unwrap()).unwrap();
        let layout = args.next().unwrap().parse::<u8>().unwrap();
        let block = state.block();
        let direction = state
            .get(p::FACING)
            .map(|_| state::facing(state))
            .unwrap_or(Direction::East);
        let mut level = level_in_radius(3);
        level
            .set_block_state(ORIGIN.below(1), Block::STONE.default_state())
            .unwrap();
        level
            .set_block_state(
                ORIGIN.relative(direction.opposite(), 1),
                Block::STONE.default_state(),
            )
            .unwrap();
        if block == Block::TRIPWIRE_HOOK {
            for i in 1..4 {
                level
                    .set_block_state(
                        ORIGIN.relative(direction, i),
                        Block::TRIPWIRE.default_state(),
                    )
                    .unwrap();
            }
            level
                .set_block_state(
                    ORIGIN.relative(direction, 4),
                    state::with_facing(Block::TRIPWIRE_HOOK.default_state(), direction.opposite()),
                )
                .unwrap();
            level
                .set_block_state(ORIGIN.relative(direction, 5), Block::STONE.default_state())
                .unwrap();
        }
        if layout > 0 {
            let power = if layout == 3 {
                ORIGIN.above(2)
            } else if layout == 2 {
                ORIGIN.relative(
                    if direction.axis() == ferraloom_util::Axis::Y {
                        Direction::East
                    } else {
                        direction.clockwise()
                    },
                    1,
                )
            } else if block == Block::REPEATER || block == Block::COMPARATOR {
                ORIGIN.relative(direction, 1)
            } else {
                ORIGIN.below(1)
            };
            level
                .set_block_state(power, Block::REDSTONE_BLOCK.default_state())
                .unwrap();
        }
        level.set_block_state(ORIGIN, state).unwrap();
        level.take_effects();
        level
            .set_placed_by(
                state,
                ORIGIN,
                None,
                crate::block::StackView::Borrowed(&Default::default()),
            )
            .unwrap();
        let mut targets = vec![ORIGIN, ORIGIN.above(1)];
        targets.extend((1..=4).map(|i| ORIGIN.relative(direction, i)));
        let mut parts = expected.split('|');
        assert_eq!(
            snapshot(&level, &targets),
            parts.next().unwrap(),
            "{name} state"
        );
        assert_eq!(
            state_mutations(&mut level),
            native_mutations(parts.next().unwrap()),
            "{name} mutation order"
        );
        assert_eq!(
            level.has_scheduled_block_tick(ORIGIN, block).to_string(),
            parts.next().unwrap(),
            "{name} initial tick"
        );
    }
}
#[test]
fn tripwire_shears_disarm_and_base_destroy_event_order_match_vanilla() {
    use crate::entity::{
        EntityData, EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    };
    use ferraloom_data::Item;
    use ferraloom_protocol::{component::ItemStack, play::clientbound::Packet};
    use ferraloom_util::{Uuid, Vec3};
    for (name, expected) in cases("tripwire-shears:") {
        let mut args = name.split(':').skip(1);
        let state = BlockState::from_id(args.next().unwrap().parse().unwrap()).unwrap();
        let tool = args.next().unwrap().parse::<u8>().unwrap();
        let mut level = level();
        game_events::observe(&mut level);
        level.set_block_state(ORIGIN, state).unwrap();
        let actor = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        let mut player = ServerPlayer::new(
            Uuid::new(1, 2),
            "redstone".into(),
            1,
            Vec3::ZERO,
            GameType::Survival,
        );
        if tool > 0 {
            player.inventory.set_selected_item(ItemStack::new(
                if tool == 2 { Item::STICK } else { Item::SHEARS },
                if tool == 3 { 0 } else { 1 },
                Default::default(),
            ));
        }
        level
            .add_entity(actor, EntityState::Player(Box::new(player)))
            .unwrap();
        game_events::take(&mut level);
        let returned = level.player_will_destroy(state, ORIGIN, actor).unwrap();
        let effects = game_events::take(&mut level)
            .into_iter()
            .filter_map(|effect| match effect {
                Observation::Effect(LevelEffect::PlayerPacket { packet, recipients }) => {
                    if let Packet::LevelEvent(event) = *packet {
                        assert_eq!(event.pos, ORIGIN);
                        assert!(recipients.is_empty());
                        Some(format!("L:{}:{}:1", event.event_type, event.data))
                    } else {
                        None
                    }
                }
                Observation::GameEvent {
                    event,
                    source,
                    affected_state,
                    ..
                } => Some(format!(
                    "G:{}:{}:{}",
                    event.protocol_id(),
                    source.map_or(0, |id| level.entity::<EntityData>(id).unwrap().network_id),
                    affected_state.map_or(-1, |state| i32::from(state.id()))
                )),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(
            format!(
                "{}|{}|{effects}",
                returned.id(),
                level.get_block_state(ORIGIN).unwrap().id()
            ),
            expected,
            "{name}"
        );
    }
}
#[test]
fn tripwire_hooks_attachment_removal_and_length_limits_match_vanilla() {
    use ferraloom_data::properties as p;
    use ferraloom_util::random::RandomSource;
    for (name, expected) in cases("tripwire-calc:") {
        let mut args = name.split(':').skip(1);
        let direction = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let length = args.next().unwrap().parse::<i32>().unwrap();
        let mode = args.next().unwrap().parse::<u8>().unwrap();
        let mut level = level_in_radius(3);
        let middle = (length / 2).max(1);
        let attached = mode == 4 || mode == 5;
        let state = state::with_facing(Block::TRIPWIRE_HOOK.default_state(), direction)
            .with(p::ATTACHED, attached)
            .unwrap()
            .with(p::POWERED, mode == 5)
            .unwrap();
        level
            .set_block_state(
                ORIGIN.relative(direction.opposite(), 1),
                Block::STONE.default_state(),
            )
            .unwrap();
        level
            .set_block_state(
                ORIGIN.relative(direction, length + 1),
                Block::STONE.default_state(),
            )
            .unwrap();
        let targets = (0..=length)
            .map(|i| ORIGIN.relative(direction, i))
            .collect::<Vec<_>>();
        for (i, &pos) in targets.iter().enumerate() {
            let placed = if i == 0 {
                state
            } else if i == length as usize {
                state::with_facing(
                    state,
                    if mode == 8 {
                        direction
                    } else {
                        direction.opposite()
                    },
                )
            } else {
                Block::TRIPWIRE
                    .default_state()
                    .with(p::ATTACHED, attached)
                    .unwrap()
                    .with(p::POWERED, (mode == 1 || mode == 2) && i == middle as usize)
                    .unwrap()
                    .with(p::DISARMED, mode == 2 && i == middle as usize)
                    .unwrap()
            };
            level.set_block_state(pos, placed).unwrap();
        }
        if matches!(mode, 3 | 7 | 10) {
            level
                .set_block_state(
                    ORIGIN.relative(direction, middle),
                    Block::AIR.default_state(),
                )
                .unwrap();
        }
        if mode == 6 {
            level
                .set_block_state(ORIGIN, Block::AIR.default_state())
                .unwrap();
        }
        if mode == 9 {
            level
                .set_block_state(
                    ORIGIN.relative(direction, length),
                    Block::AIR.default_state(),
                )
                .unwrap();
        }
        let replacement = Block::TRIPWIRE
            .default_state()
            .with(p::POWERED, true)
            .unwrap()
            .with(p::DISARMED, mode == 10)
            .unwrap();
        level.take_effects();
        tripwire::calculate_state(
            &mut level,
            ORIGIN,
            state,
            mode == 6,
            true,
            matches!(mode, 7 | 10).then_some((middle as usize, Some(replacement))),
        )
        .unwrap();
        let mut parts = expected.split('|');
        assert_eq!(
            snapshot(&level, &targets),
            parts.next().unwrap(),
            "{name} states"
        );
        assert_eq!(
            state_mutations(&mut level),
            native_mutations(parts.next().unwrap()),
            "{name} mutation order"
        );
        assert_eq!(
            format!(
                "{}:{}",
                level.has_scheduled_block_tick(ORIGIN, Block::TRIPWIRE_HOOK),
                level.has_scheduled_block_tick(
                    ORIGIN.relative(direction, length),
                    Block::TRIPWIRE_HOOK
                )
            ),
            parts.next().unwrap(),
            "{name} hook checks"
        );
        assert_eq!(
            level.random().next_long().to_string(),
            parts.next().unwrap(),
            "{name} detach pitch RNG"
        );
    }
}
#[test]
fn tripwire_entity_press_and_release_scheduled_timelines_match_vanilla() {
    use crate::entity::{EntityRef, EntityState, item::PrimedTnt};
    use ferraloom_util::Vec3;
    for (name, expected) in cases("tripwire-timeline:") {
        let direction =
            Direction::VALUES[name.split(':').nth(1).unwrap().parse::<usize>().unwrap()];
        let mut level = level_in_radius(3);
        let hook = state::with_facing(Block::TRIPWIRE_HOOK.default_state(), direction);
        level
            .set_block_state(
                ORIGIN.relative(direction.opposite(), 1),
                Block::STONE.default_state(),
            )
            .unwrap();
        level
            .set_block_state(ORIGIN.relative(direction, 5), Block::STONE.default_state())
            .unwrap();
        let targets = (0..=4)
            .map(|i| ORIGIN.relative(direction, i))
            .collect::<Vec<_>>();
        for (i, &pos) in targets.iter().enumerate() {
            level
                .set_block_state(
                    pos,
                    if i == 0 {
                        hook
                    } else if i == 4 {
                        state::with_facing(hook, direction.opposite())
                    } else {
                        Block::TRIPWIRE.default_state()
                    },
                )
                .unwrap();
        }
        tripwire::calculate_state(&mut level, ORIGIN, hook, false, true, None).unwrap();
        let pos = ORIGIN.relative(direction, 2);
        let actor = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        let mut tnt = PrimedTnt::new(1, 123);
        tnt.parent.set_pos(Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.1,
            f64::from(pos.z) + 0.5,
        ));
        level
            .add_entity(actor, EntityState::PrimedTnt(Box::new(tnt)))
            .unwrap();
        let mut samples = Vec::new();
        for tick in 0..=22 {
            level.set_game_time(tick);
            level.take_effects();
            if tick == 0 {
                tripwire::WIRE
                    .entity_inside(level.get_block_state(pos).unwrap(), pos, actor, &mut level)
                    .unwrap();
            }
            if tick == 7 {
                level.remove_entity(actor).unwrap();
            }
            level.tick_scheduled_blocks(tick, 65536).unwrap();
            samples.push(format!(
                "{}/{}/{}/{}",
                snapshot(&level, &targets),
                level.has_scheduled_block_tick(pos, Block::TRIPWIRE),
                level.has_scheduled_block_tick(ORIGIN, Block::TRIPWIRE_HOOK),
                state_mutations(&mut level)
            ));
        }
        let expected = expected
            .split('|')
            .map(|sample| {
                let mut parts = sample.splitn(4, '/');
                format!(
                    "{}/{}/{}/{}",
                    parts.next().unwrap(),
                    parts.next().unwrap(),
                    parts.next().unwrap(),
                    native_mutations(parts.next().unwrap())
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(samples.join("|"), expected, "{name}");
    }
}
#[test]
fn note_instruments_pitch_and_seeded_sound_events_match_vanilla() {
    use ferraloom_data::properties as p;
    use ferraloom_util::random::RandomSource;
    for (name, expected) in cases("note-instrument:") {
        let mut args = name.split(':').skip(1);
        let below = Block::from_protocol_id(args.next().unwrap().parse().unwrap()).unwrap();
        let above = Block::from_protocol_id(args.next().unwrap().parse().unwrap()).unwrap();
        let mut level = level();
        level
            .set_block_state(ORIGIN.below(1), below.default_state())
            .unwrap();
        level
            .set_block_state(ORIGIN.above(1), above.default_state())
            .unwrap();
        let state = note::set_instrument(
            &LevelReaderRef::Server(&level),
            ORIGIN,
            Block::NOTE_BLOCK.default_state(),
        )
        .unwrap();
        assert_eq!(state.id().to_string(), expected, "{name}");
    }
    for (name, expected) in cases("note-event:") {
        let mut args = name.split(':').skip(1);
        let instrument = note_instrument(args.next().unwrap());
        let value = args.next().unwrap().parse::<u8>().unwrap();
        let mut level = level();
        let state = Block::NOTE_BLOCK
            .default_state()
            .with(p::INSTRUMENT, instrument)
            .unwrap()
            .with(p::NOTE, value)
            .unwrap();
        level.set_block_state(ORIGIN, state).unwrap();
        level.take_effects();
        let played = note::NOTE
            .trigger_event(state, ORIGIN, 0, 0, &mut level)
            .unwrap();
        let actual = format!(
            "{played}|{}|{}",
            note_sounds(&mut level),
            level.random().next_long()
        );
        assert_eq!(actual, expected, "{name}");
    }
}
#[test]
fn note_redstone_edges_and_obstruction_match_vanilla() {
    use ferraloom_data::properties as p;
    for instrument in
        (0..=u8::MAX).filter_map(<p::Instrument as ferraloom_data::PropertyValue>::from_index)
    {
        for blocked in [false, true] {
            let mut level = level();
            let state = Block::NOTE_BLOCK
                .default_state()
                .with(p::INSTRUMENT, instrument)
                .unwrap();
            level.set_block_state(ORIGIN, state).unwrap();
            if blocked {
                level
                    .set_block_state(ORIGIN.above(1), Block::STONE.default_state())
                    .unwrap();
            }
            for change in 0..3 {
                level.take_effects();
                level
                    .set_block_and_update(
                        ORIGIN.west(1),
                        if change == 1 {
                            Block::AIR
                        } else {
                            Block::REDSTONE_BLOCK
                        }
                        .default_state(),
                    )
                    .unwrap();
                level.run_block_events().unwrap();
                let name = format!("note-power:{}:{blocked}:{change}", instrument.as_str());
                let expected = cases("note-power:")
                    .find(|(key, _)| *key == name)
                    .unwrap()
                    .1;
                let actual = format!(
                    "{}|{}",
                    level.get_block_state(ORIGIN).unwrap().id(),
                    note_sounds(&mut level)
                );
                assert_eq!(actual, expected, "{name}");
            }
        }
    }
}
#[test]
fn rail_junctions_slopes_and_connection_update_order_match_vanilla() {
    use ferraloom_data::properties as p;
    let directions = [
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ];
    for (name, expected) in cases("rail-place:") {
        let mut args = name.split(':').skip(1);
        let block = Block::from_name(&format!("minecraft:{}", args.next().unwrap())).unwrap();
        let mut pattern = args.next().unwrap().parse::<u16>().unwrap();
        let powered = args.next().unwrap().parse::<bool>().unwrap();
        let initial = if args.next().unwrap().parse::<bool>().unwrap() {
            p::Shape::EastWest
        } else {
            p::Shape::NorthSouth
        };
        let mut level = level();
        let mut targets = vec![ORIGIN];
        level
            .set_block_state(ORIGIN.below(1), Block::STONE.default_state())
            .unwrap();
        for direction in directions {
            let value = pattern & 3;
            pattern >>= 2;
            let pos = ORIGIN.relative(direction, 1).above(match value {
                1 => -1,
                3 => 1,
                _ => 0,
            });
            targets.push(pos);
            if value != 0 {
                level
                    .set_block_state(pos.below(1), Block::STONE.default_state())
                    .unwrap();
                level.set_block_state(pos, block.default_state()).unwrap();
            }
        }
        let state = block.default_state().with(p::SHAPE, initial).unwrap();
        level.set_block_state(ORIGIN, state).unwrap();
        level.take_effects();
        let mut rail = rail::RailState::new(ORIGIN, state);
        rail.place(&mut level, powered, true, initial).unwrap();
        let mut parts = expected.split('|');
        assert_eq!(
            rail.state().id().to_string(),
            parts.next().unwrap(),
            "{name} returned state"
        );
        assert_eq!(
            positions(rail.connections()),
            parts.next().unwrap(),
            "{name} connections"
        );
        assert_eq!(
            snapshot(&level, &targets),
            parts.next().unwrap(),
            "{name} canonical states"
        );
        assert_eq!(
            state_mutations(&mut level),
            native_mutations(parts.next().unwrap()),
            "{name} mutation order"
        );
    }
}
#[test]
fn powered_and_activator_rails_preserve_search_limits_and_power_update_order() {
    use ferraloom_data::properties as p;
    for (name, expected) in cases("rail-find:") {
        let mut args = name.split(':').skip(1);
        let block = Block::from_name(&format!("minecraft:{}", args.next().unwrap())).unwrap();
        let direction = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let length = args.next().unwrap().parse::<i32>().unwrap();
        let obstruction = args.next().unwrap().parse::<u8>().unwrap();
        let mut level = level();
        let shape = if direction.axis() == ferraloom_util::Axis::X {
            p::Shape::EastWest
        } else {
            p::Shape::NorthSouth
        };
        let state = block
            .default_state()
            .with(p::SHAPE, shape)
            .unwrap()
            .with(p::POWERED, true)
            .unwrap();
        for i in 0..=length {
            level
                .set_block_state(ORIGIN.relative(direction, i), state)
                .unwrap();
        }
        if obstruction > 0 {
            let changed = match obstruction {
                1 => state.with(p::POWERED, false).unwrap(),
                2 => state
                    .with(
                        p::SHAPE,
                        if shape == p::Shape::EastWest {
                            p::Shape::NorthSouth
                        } else {
                            p::Shape::EastWest
                        },
                    )
                    .unwrap(),
                3 => (if block == Block::ACTIVATOR_RAIL {
                    Block::POWERED_RAIL
                } else {
                    Block::ACTIVATOR_RAIL
                })
                .default_state()
                .with(p::SHAPE, shape)
                .unwrap()
                .with(p::POWERED, true)
                .unwrap(),
                _ => unreachable!(),
            };
            level
                .set_block_state(ORIGIN.relative(direction, (length + 1) / 2), changed)
                .unwrap();
        }
        level
            .set_block_state(
                ORIGIN.relative(direction, length).above(1),
                Block::REDSTONE_BLOCK.default_state(),
            )
            .unwrap();
        let forward = direction == Direction::South || direction == Direction::West;
        let values = (0..=8)
            .map(|depth| {
                rail::find_powered_rail_signal(&level, ORIGIN, state, forward, depth)
                    .unwrap()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(values, expected, "{name}");
    }
    for block in [Block::POWERED_RAIL, Block::ACTIVATOR_RAIL] {
        for direction in state::HORIZONTAL {
            let mut level = level();
            let shape = if direction.axis() == ferraloom_util::Axis::X {
                p::Shape::EastWest
            } else {
                p::Shape::NorthSouth
            };
            let targets = (0..13)
                .map(|i| ORIGIN.relative(direction, i))
                .collect::<Vec<_>>();
            for &pos in &targets {
                level
                    .set_block_state(pos.below(1), Block::STONE.default_state())
                    .unwrap();
                level
                    .set_block_state(pos, block.default_state().with(p::SHAPE, shape).unwrap())
                    .unwrap();
            }
            for change in 0..3 {
                level.take_effects();
                level
                    .set_block_and_update(
                        ORIGIN.relative(direction.opposite(), 1),
                        if change == 1 {
                            Block::AIR
                        } else {
                            Block::REDSTONE_BLOCK
                        }
                        .default_state(),
                    )
                    .unwrap();
                let name = format!(
                    "rail-power:{}:{}:{change}",
                    block.name().strip_prefix("minecraft:").unwrap(),
                    direction as u8
                );
                let expected = cases("rail-power:")
                    .find(|(key, _)| *key == name)
                    .unwrap()
                    .1;
                let (states, changes) = expected.split_once('|').unwrap();
                assert_eq!(snapshot(&level, &targets), states, "{name} final state");
                assert_eq!(
                    state_mutations(&mut level),
                    native_mutations(changes),
                    "{name} mutation order"
                );
            }
        }
    }
}
fn query_signals(level: &Level, state: BlockState) -> String {
    let reader = LevelReaderRef::Server(level);
    let mut values = vec![
        reader.is_signal_source(state).unwrap().to_string(),
        reader.own_signal(state, ORIGIN).unwrap().to_string(),
    ];
    for direction in Direction::VALUES {
        values.push(format!(
            "{}:{}:{}:{}",
            reader.get_signal(ORIGIN, direction).unwrap(),
            reader.get_direct_signal(ORIGIN, direction).unwrap(),
            reader
                .get_control_input_signal(ORIGIN, direction, false)
                .unwrap(),
            reader
                .get_control_input_signal(ORIGIN, direction, true)
                .unwrap()
        ));
    }
    values.join(";")
}
#[test]
fn signal_sources_directions_and_conductor_relays_match_vanilla() {
    for (name, expected) in cases("signal:") {
        let state =
            BlockState::from_id(name.strip_prefix("signal:").unwrap().parse().unwrap()).unwrap();
        let mut level = level();
        level.set_block_state(ORIGIN, state).unwrap();
        assert_eq!(query_signals(&level, state), expected, "{name}");
    }
    for (name, expected) in cases("stored-signal:") {
        use crate::block::entity::{BlockEntityData, BlockEntityState, BlockEntityType};
        let mut args = name.split(':').skip(1);
        let state = BlockState::from_id(args.next().unwrap().parse().unwrap()).unwrap();
        let output = args.next().unwrap().parse::<i32>().unwrap();
        let mut level = level();
        level.set_block_state(ORIGIN, state).unwrap();
        let mut entity = comparator::ComparatorBlockEntity::new(
            BlockEntityData::new(BlockEntityType::COMPARATOR, ORIGIN, state).unwrap(),
        );
        entity.output = output;
        level
            .set_block_entity(BlockEntityState::Comparator(Box::new(entity)))
            .unwrap();
        let receiver = ORIGIN.relative(state::facing(state).opposite(), 1);
        level
            .set_block_state(receiver, Block::STONE.default_state())
            .unwrap();
        let actual = format!(
            "{}|{}:{}:{}:{}:{}",
            query_signals(&level, state),
            level.get_signal(receiver, Direction::Down).unwrap(),
            level.get_direct_signal_to(receiver).unwrap(),
            level.get_best_neighbor_signal(receiver).unwrap(),
            level.has_neighbor_signal(receiver).unwrap(),
            level.get_best_own_or_neighbor_signal(ORIGIN).unwrap()
        );
        assert_eq!(actual, expected, "{name}");
    }
    for (name, expected) in cases("relay:") {
        let mut args = name.split(':').skip(1);
        let source = BlockState::from_id(args.next().unwrap().parse().unwrap()).unwrap();
        let d = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let mut level = level();
        level
            .set_block_state(ORIGIN, Block::STONE.default_state())
            .unwrap();
        level
            .set_block_state(ORIGIN.relative(d, 1), source)
            .unwrap();
        assert_eq!(
            format!(
                "{}:{}:{}:{}",
                level.get_signal(ORIGIN, Direction::Down).unwrap(),
                level.get_direct_signal_to(ORIGIN).unwrap(),
                level.get_best_neighbor_signal(ORIGIN).unwrap(),
                level.has_neighbor_signal(ORIGIN).unwrap()
            ),
            expected,
            "{name}"
        );
    }
    let mut level = level();
    let support = ORIGIN.below(1);
    let wire = support.west(1);
    let input = support.west(2);
    for pos in [support, wire.below(1), input.below(1)] {
        level
            .set_block_state(pos, Block::STONE.default_state())
            .unwrap();
    }
    level
        .set_block_and_update(wire, Block::REDSTONE_WIRE.default_state())
        .unwrap();
    level
        .set_block_and_update(ORIGIN, Block::REDSTONE_TORCH.default_state())
        .unwrap();
    for (name, expected) in cases("torch-input:") {
        let powered = name.split(':').nth(1).unwrap().parse::<bool>().unwrap();
        level
            .set_block_and_update(
                input,
                if powered {
                    Block::REDSTONE_BLOCK
                } else {
                    Block::AIR
                }
                .default_state(),
            )
            .unwrap();
        assert_eq!(
            format!(
                "{}|{}|{}",
                level.has_signal(support, Direction::Down).unwrap(),
                level.get_block_state(wire).unwrap().id(),
                level.has_scheduled_block_tick(ORIGIN, Block::REDSTONE_TORCH),
            ),
            expected,
            "{name} strong input"
        );
    }
}
#[test]
fn piston_limits_sticky_branches_and_collision_order_match_vanilla() {
    for (name, expected) in cases("piston-line:") {
        let mut args = name.split(':').skip(1);
        let d = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let extending = args.next().unwrap().parse().unwrap();
        let length = args.next().unwrap().parse::<i32>().unwrap();
        let mut level = level();
        for i in 0..length {
            level
                .set_block_state(
                    ORIGIN.relative(d, i + if extending { 1 } else { 2 }),
                    Block::STONE.default_state(),
                )
                .unwrap();
        }
        let mut resolver = piston::PistonStructureResolver::new(ORIGIN, d, extending);
        let ok = resolver.resolve(&level).unwrap();
        assert_eq!(
            format!(
                "{ok}|{}|{}",
                positions(resolver.to_push()),
                positions(resolver.to_destroy())
            ),
            expected,
            "{name}"
        );
    }
    for (name, expected) in cases("piston-branch:") {
        let mut args = name.split(':').skip(1);
        let sticky = if args.next().unwrap() == "slime" {
            Block::SLIME_BLOCK
        } else {
            Block::HONEY_BLOCK
        };
        let kind = args.next().unwrap().parse::<i32>().unwrap();
        let mut level = level();
        let s = ORIGIN.east(1);
        level.set_block_state(s, sticky.default_state()).unwrap();
        let above = match kind {
            1 => Block::OBSIDIAN,
            2 => Block::REDSTONE_WIRE,
            3 => {
                if sticky == Block::SLIME_BLOCK {
                    Block::HONEY_BLOCK
                } else {
                    Block::SLIME_BLOCK
                }
            }
            _ => sticky,
        };
        level
            .set_block_state(s.above(1), above.default_state())
            .unwrap();
        for p in [s.below(1), s.north(1), s.south(1)] {
            level
                .set_block_state(p, Block::STONE.default_state())
                .unwrap();
        }
        if kind == 4 {
            for p in [s.east(1), s.above(1).east(1), s.above(1).east(2)] {
                level.set_block_state(p, sticky.default_state()).unwrap();
            }
        }
        if kind == 5 {
            for i in 0..12 {
                level
                    .set_block_state(s.south(i + 1), Block::STONE.default_state())
                    .unwrap();
            }
        }
        let mut resolver = piston::PistonStructureResolver::new(ORIGIN, Direction::East, true);
        let ok = resolver.resolve(&level).unwrap();
        assert_eq!(
            format!(
                "{ok}|{}|{}",
                positions(resolver.to_push()),
                positions(resolver.to_destroy())
            ),
            expected,
            "{name}"
        );
    }
}
#[test]
fn default_and_experimental_wire_networks_match_each_vanilla_state_mutation() {
    for experimental in [false, true] {
        for shape in 0..3 {
            let mut level = level();
            level.rules_mut().redstone_experiments = experimental;
            let mut wires: Vec<_> = (0..7).map(|x| ORIGIN.east(x)).collect();
            if shape > 0 {
                for z in 1..=3 {
                    wires.push(ORIGIN.east(2).south(z));
                    wires.push(ORIGIN.east(4).north(z));
                }
            }
            if shape == 2 {
                wires.push(ORIGIN.east(3).above(1));
                level
                    .set_block_state(ORIGIN.east(3), Block::STONE.default_state())
                    .unwrap();
                wires.retain(|p| *p != ORIGIN.east(3));
            }
            for pos in &wires {
                level
                    .set_block_state(pos.below(1), Block::STONE.default_state())
                    .unwrap();
                level
                    .set_block_state(*pos, Block::REDSTONE_WIRE.default_state())
                    .unwrap();
            }
            for pos in &wires {
                let state = level.get_block_state(*pos).unwrap();
                let updated = level.update_from_neighbor_shapes(state, *pos).unwrap();
                level.set_block_state(*pos, updated).unwrap();
            }
            for change in 0..3 {
                level.take_effects();
                let block = if change == 1 {
                    Block::AIR
                } else {
                    Block::REDSTONE_BLOCK
                };
                level
                    .set_block(
                        ORIGIN.west(1),
                        block.default_state(),
                        UpdateFlags::ALL,
                        UPDATE_LIMIT,
                    )
                    .unwrap();
                let name = format!("wire:{experimental}:{shape}:{change}");
                let expected = cases("wire:").find(|(key, _)| *key == name).unwrap().1;
                let mut parts = expected.split('|');
                assert_eq!(
                    snapshot(&level, &wires),
                    parts.next().unwrap(),
                    "{name} final state"
                );
                let changes = level
                    .take_effects()
                    .into_iter()
                    .filter_map(|e| match e {
                        LevelEffect::BlockUpdated { pos, new, .. } => {
                            Some(format!("{}, {}, {}={}", pos.x, pos.y, pos.z, new.id()))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(";");
                let expected_changes = parts
                    .next()
                    .unwrap()
                    .split(';')
                    .map(|s| s.split_once('/').unwrap().0)
                    .collect::<Vec<_>>()
                    .join(";");
                assert_eq!(changes, expected_changes, "{name} exact mutation order");
            }
        }
    }
}

#[test]
fn moving_piston_shapes_and_progress_match_vanilla_including_noclip_and_retraction() {
    use crate::block::entity::{BlockEntityData, BlockEntityState, BlockEntityType};
    for (name, expected) in cases("moving:") {
        let mut args = name.split(':').skip(1);
        let moved = BlockState::from_id(args.next().unwrap().parse().unwrap()).unwrap();
        let direction = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let extending = args.next().unwrap().parse().unwrap();
        let source = args.next().unwrap().parse().unwrap();
        let progress = args.next().unwrap().parse::<f32>().unwrap();
        let suppress = args.next().unwrap().parse::<bool>().unwrap();
        let mut level = level();
        let state = Block::MOVING_PISTON.default_state();
        level.set_block_state(ORIGIN, state).unwrap();
        let data = BlockEntityData::new(BlockEntityType::PISTON, ORIGIN, state).unwrap();
        let mut entity = moving_piston::PistonMovingBlockEntity::moving(
            data, moved, direction, extending, source,
        );
        entity.progress = progress;
        entity.progress_o = progress - 0.25;
        let samples = [-0.25, 0.0, 0.5, 1.0, 2.0]
            .map(|p| format!("{:x}", entity.get_progress(p).to_bits()))
            .join(",");
        level.region_mut().redstone.piston_noclip = suppress.then(|| entity.movement_direction());
        level
            .set_block_entity(BlockEntityState::MovingPiston(Box::new(entity)))
            .unwrap();
        let handle = level.block_entity_ref(ORIGIN).unwrap().unwrap();
        let boxes = moving_piston::collision_shape(&level, handle)
            .unwrap()
            .to_aabbs()
            .into_iter()
            .map(|b| {
                format!(
                    "{:x},{:x},{:x},{:x},{:x},{:x}",
                    b.min_x.to_bits(),
                    b.min_y.to_bits(),
                    b.min_z.to_bits(),
                    b.max_x.to_bits(),
                    b.max_y.to_bits(),
                    b.max_z.to_bits()
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(format!("{boxes}|{samples}"), expected, "{name}");
    }
}

#[test]
fn piston_entity_push_slime_velocity_and_honey_drag_match_real_vanilla_moves() {
    use crate::{
        block::entity::{
            BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState, BlockEntityType,
        },
        entity::{EntityData, EntityRef, EntityState, item::PrimedTnt},
    };
    use ferraloom_util::Vec3;
    for (name, expected) in cases("push:") {
        let mut args = name.split(':').skip(1);
        let block = Block::from_protocol_id(args.next().unwrap().parse().unwrap()).unwrap();
        let stuck = args.next().unwrap().parse::<bool>().unwrap();
        let mut level = level();
        let fluid_values = include_str!("../../../tests/java/level/fluid-tags.tsv")
            .lines()
            .map(|line| {
                let (name, fluids) = line.split_once('\t').unwrap();
                (
                    name.strip_prefix("fluid-tag:").unwrap().parse().unwrap(),
                    fluids
                        .split(',')
                        .filter(|f| !f.is_empty())
                        .map(|name| ferraloom_data::registries::fluid::Id::from_name(name).unwrap())
                        .collect(),
                )
            })
            .collect();
        level.set_fluid_tags(Arc::new(
            ferraloom_data::datapack::TagSet::from_tags(
                ferraloom_data::datapack::Tags {
                    values: fluid_values,
                    diagnostics: Vec::new(),
                },
                ferraloom_data::registries::fluid::ALL.len(),
            )
            .unwrap(),
        ));
        let state = state::with_facing(Block::MOVING_PISTON.default_state(), Direction::East);
        level.set_block_state(ORIGIN, state).unwrap();
        let data = BlockEntityData::new(BlockEntityType::PISTON, ORIGIN, state).unwrap();
        let moving = moving_piston::PistonMovingBlockEntity::moving(
            data,
            block.default_state(),
            Direction::East,
            true,
            false,
        );
        level
            .set_block_entity(BlockEntityState::MovingPiston(Box::new(moving)))
            .unwrap();
        let handle: BlockEntityRef = level.block_entity_ref(ORIGIN).unwrap().unwrap();
        let actor = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        let mut tnt = PrimedTnt::new(1, 123);
        tnt.parent.set_pos(Vec3::new(
            f64::from(ORIGIN.x) + if stuck { -0.4 } else { 0.1 },
            f64::from(ORIGIN.y) + if stuck { 0.9375 } else { 0.1 },
            f64::from(ORIGIN.z) + 0.5,
        ));
        tnt.parent.on_ground = stuck;
        level
            .add_entity(actor, EntityState::PrimedTnt(Box::new(tnt)))
            .unwrap();
        let mut samples = Vec::new();
        for tick in 1..=3 {
            level.set_game_time(tick);
            moving_piston::MOVING_PISTON_ENTITY
                .tick(handle, &mut level)
                .unwrap();
            let entity = level.entity::<EntityData>(actor).unwrap();
            let pos = entity.position;
            let velocity = entity.velocity;
            samples.push(format!(
                "{:x},{:x},{:x}/{:x},{:x},{:x}/{}",
                pos.x.to_bits(),
                pos.y.to_bits(),
                pos.z.to_bits(),
                velocity.x.to_bits(),
                velocity.y.to_bits(),
                velocity.z.to_bits(),
                level.get_block_state(ORIGIN).unwrap().id()
            ));
        }
        assert_eq!(samples.join(";"), expected, "{name}");
    }
}

#[test]
fn comparator_container_reads_and_occlusion_match_real_vanilla_inventories() {
    use crate::block::{containers::storage::BaseContainerBlockEntity, entity};
    use ferraloom_data::{Item, properties as p};
    use ferraloom_protocol::component::ItemStack;
    for (name, expected) in cases("container-comparator:") {
        let mut args = name.split(':').skip(1);
        let block = Block::from_protocol_id(args.next().unwrap().parse().unwrap()).unwrap();
        let pattern = args.next().unwrap().parse::<usize>().unwrap();
        let gap = args.next().unwrap().parse::<i32>().unwrap();
        let blocked = args.next().unwrap().parse::<bool>().unwrap();
        let mut level = level();
        let direction = Direction::North;
        let target = ORIGIN.relative(direction, 1 + gap);
        let target_state = block.default_state();
        level.set_block_state(target, target_state).unwrap();
        level
            .set_block_entity(entity::create(target, target_state).unwrap())
            .unwrap();
        let handle = level.block_entity_ref(target).unwrap().unwrap();
        for (slot, stack) in level
            .block_entity_mut::<BaseContainerBlockEntity>(handle)
            .unwrap()
            .items
            .iter_mut()
            .enumerate()
        {
            let item = match pattern {
                1..=3 if slot == 0 => Some((Item::STONE, [0, 1, 32, 64][pattern])),
                4 => Some((Item::STONE, 64)),
                5 if slot % 2 == 0 => Some((Item::ENDER_PEARL, 16)),
                5 => Some((Item::WATER_BUCKET, 1)),
                6 if slot == 0 => Some((Item::WATER_BUCKET, 1)),
                _ => None,
            };
            if let Some((item, count)) = item {
                *stack = ItemStack::new(item, count, Default::default());
            }
        }
        if gap == 1 {
            level
                .set_block_state(ORIGIN.relative(direction, 1), Block::STONE.default_state())
                .unwrap();
        }
        if blocked {
            level
                .set_block_state(target.above(1), Block::STONE.default_state())
                .unwrap();
        }
        level
            .set_block_state(
                ORIGIN.relative(direction.clockwise(), 1),
                Block::REDSTONE_WIRE
                    .default_state()
                    .with(p::POWER, 7)
                    .unwrap(),
            )
            .unwrap();
        let mut outputs = Vec::new();
        for mode in [p::Mode::Compare, p::Mode::Subtract] {
            let state = state::with_facing(Block::COMPARATOR.default_state(), direction)
                .with(p::MODE, mode)
                .unwrap();
            level.set_block_state(ORIGIN, state).unwrap();
            outputs.push(format!(
                "{}:{}",
                comparator::calculate_output(&mut level, ORIGIN, state).unwrap(),
                comparator::should_turn_on(&mut level, ORIGIN, state).unwrap()
            ));
        }
        assert_eq!(outputs.join("|"), expected, "{name}");
    }
}

#[test]
fn comparator_compare_and_subtract_match_vanilla_for_all_inputs() {
    use ferraloom_data::properties as p;
    for (name, expected) in cases("comparator:") {
        let mut args = name.split(':').skip(1);
        let state = BlockState::from_id(args.next().unwrap().parse().unwrap()).unwrap();
        let main = args.next().unwrap().parse::<u8>().unwrap();
        let side = args.next().unwrap().parse::<u8>().unwrap();
        let mut level = level();
        let direction = state::facing(state);
        level.set_block_state(ORIGIN, state).unwrap();
        level
            .set_block_state(
                ORIGIN.relative(direction, 1),
                Block::REDSTONE_WIRE
                    .default_state()
                    .with(p::POWER, main)
                    .unwrap(),
            )
            .unwrap();
        level
            .set_block_state(
                ORIGIN.relative(direction.clockwise(), 1),
                Block::REDSTONE_WIRE
                    .default_state()
                    .with(p::POWER, side)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            format!(
                "{}:{}",
                comparator::calculate_output(&mut level, ORIGIN, state).unwrap(),
                comparator::should_turn_on(&mut level, ORIGIN, state).unwrap()
            ),
            expected,
            "{name}"
        );
    }
}
#[test]
fn redstone_placements_match_real_vanilla_contexts() {
    use crate::block::{DirectionalPlaceContext, LevelAccessRef};
    use ferraloom_protocol::component::ItemStack;
    for (name, expected) in cases("placement:") {
        let mut args = name.split(':').skip(1);
        let block = Block::from_protocol_id(args.next().unwrap().parse().unwrap()).unwrap();
        let direction = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let face = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let layout = args.next().unwrap().parse::<u8>().unwrap();
        let mut level = level();
        level
            .set_block_state(ORIGIN.below(1), Block::STONE.default_state())
            .unwrap();
        if layout != 0 {
            level
                .set_block_state(
                    ORIGIN.relative(direction, 1),
                    if layout == 1 {
                        Block::STONE
                    } else {
                        Block::REDSTONE_BLOCK
                    }
                    .default_state(),
                )
                .unwrap();
        }
        let item = ItemStack::default();
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            ORIGIN,
            direction,
            &item,
            face,
        )
        .unwrap();
        let placed = LevelAccessRef::Server(&mut level)
            .get_state_for_placement(block, &context)
            .unwrap();
        assert_eq!(
            placed.map_or(-1, |s| i32::from(s.id())).to_string(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn hopper_chains_cooldowns_locks_and_each_transfer_match_vanilla() {
    use crate::{
        block::{
            containers::storage::BaseContainerBlockEntity,
            entity::{self, BlockEntityState},
        },
        inventory::Container,
    };
    use ferraloom_data::{Item, properties as p};
    use ferraloom_protocol::component::ItemStack;
    for (name, expected) in cases("hopper:") {
        let mut args = name.split(':').skip(1);
        let reverse = args.next().unwrap().parse::<bool>().unwrap();
        let locked = args.next().unwrap().parse::<bool>().unwrap();
        let amount = args.next().unwrap().parse::<i32>().unwrap();
        let mut level = level();
        let mut handles = Vec::new();
        for y in 0..3 {
            let pos = ORIGIN.below(y);
            let state = Block::HOPPER.default_state();
            level.set_block_state(pos, state).unwrap();
            level
                .set_block_entity(entity::create(pos, state).unwrap())
                .unwrap();
            handles.push(level.block_entity_ref(pos).unwrap().unwrap());
        }
        let bottom = ORIGIN.below(3);
        let barrel = Block::BARREL.default_state();
        level.set_block_state(bottom, barrel).unwrap();
        level
            .set_block_entity(entity::create(bottom, barrel).unwrap())
            .unwrap();
        let sink = level.block_entity_ref(bottom).unwrap().unwrap();
        level
            .block_entity_mut::<BaseContainerBlockEntity>(handles[0])
            .unwrap()
            .set_item(0, ItemStack::new(Item::STONE, amount, Default::default()));
        let mut steps = Vec::new();
        for tick in 1..=20 {
            level.set_game_time(tick);
            if locked && matches!(tick, 4 | 11) {
                level
                    .set_block(
                        ORIGIN.below(1).relative(Direction::East, 1),
                        if tick == 4 {
                            Block::REDSTONE_BLOCK
                        } else {
                            Block::AIR
                        }
                        .default_state(),
                        UpdateFlags::ALL,
                        UPDATE_LIMIT,
                    )
                    .unwrap();
            }
            for index in 0..3 {
                let i = if reverse { 2 - index } else { index };
                hopper::push_items_tick(&mut level, handles[i]).unwrap();
                let mut values = Vec::new();
                for &handle in &handles {
                    let BlockEntityState::Hopper(h) = level.block_entity_state(handle).unwrap()
                    else {
                        unreachable!()
                    };
                    let items = h
                        .parent
                        .parent
                        .items
                        .iter()
                        .map(|item| crate::entity::player::inventory::count(item).to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    values.push(format!(
                        "{items}/{}/{}",
                        h.cooldown_time,
                        level
                            .get_block_state(handle.pos)
                            .unwrap()
                            .get(p::ENABLED)
                            .unwrap()
                    ));
                }
                values.push(
                    crate::entity::player::inventory::count(
                        &level
                            .block_entity::<BaseContainerBlockEntity>(sink)
                            .unwrap()
                            .get_item(0),
                    )
                    .to_string(),
                );
                steps.push(values.join(";"));
            }
        }
        assert_eq!(steps.join("|"), expected, "{name}");
    }
}

#[test]
fn piston_block_events_short_pulses_and_quasi_timelines_match_vanilla() {
    for (name, expected) in cases("piston-timeline:") {
        let mut args = name.split(':').skip(1);
        let sticky = args.next().unwrap().parse::<bool>().unwrap();
        let quasi = args.next().unwrap().parse::<bool>().unwrap();
        let length = args.next().unwrap().parse::<i32>().unwrap();
        let off = args.next().unwrap().parse::<i64>().unwrap();
        let mut level = level();
        let piston = state::with_facing(
            if sticky {
                Block::STICKY_PISTON
            } else {
                Block::PISTON
            }
            .default_state(),
            Direction::East,
        );
        level.set_block_state(ORIGIN, piston).unwrap();
        for i in 1..=length {
            level
                .set_block_state(
                    ORIGIN.relative(Direction::East, i),
                    Block::STONE.default_state(),
                )
                .unwrap();
        }
        let power = if quasi {
            ORIGIN.above(2)
        } else {
            ORIGIN.below(1)
        };
        level.take_effects();
        level
            .set_block(
                power,
                Block::REDSTONE_BLOCK.default_state(),
                UpdateFlags::ALL,
                UPDATE_LIMIT,
            )
            .unwrap();
        let snapshot = |level: &Level| {
            (0..=14)
                .map(|i| {
                    level
                        .get_block_state(ORIGIN.relative(Direction::East, i))
                        .unwrap()
                        .id()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        let mut steps = Vec::new();
        if quasi {
            steps.push(snapshot(&level));
            level.neighbor_changed(ORIGIN, Block::STONE, None).unwrap();
        }
        level.run_block_events().unwrap();
        steps.push(snapshot(&level));
        if off == 0 {
            level
                .set_block(
                    power,
                    Block::AIR.default_state(),
                    UpdateFlags::ALL,
                    UPDATE_LIMIT,
                )
                .unwrap();
            if quasi {
                level.neighbor_changed(ORIGIN, Block::STONE, None).unwrap();
            }
            level.run_block_events().unwrap();
            steps.push(snapshot(&level));
        }
        for tick in 1..=7 {
            level.set_game_time(tick);
            level.tick_block_entities(true).unwrap();
            steps.push(snapshot(&level));
            if tick == off {
                level
                    .set_block(
                        power,
                        Block::AIR.default_state(),
                        UpdateFlags::ALL,
                        UPDATE_LIMIT,
                    )
                    .unwrap();
                if quasi {
                    level.neighbor_changed(ORIGIN, Block::STONE, None).unwrap();
                }
                level.run_block_events().unwrap();
                steps.push(snapshot(&level));
            }
        }
        let (expected_steps, expected_changes) = expected.split_once('#').unwrap();
        assert_eq!(steps.join("|"), expected_steps, "{name} tick states");
        let changes = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                LevelEffect::BlockUpdated { pos, new, .. } => {
                    Some(format!("{}, {}, {}={}", pos.x, pos.y, pos.z, new.id()))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(";");
        let expected_changes = expected_changes
            .split(';')
            .map(|change| change.split_once('/').unwrap().0)
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(changes, expected_changes, "{name} sub-tick mutation order");
    }
}

#[test]
fn dispenser_item_spawn_and_projectile_parameters_match_vanilla() {
    use crate::entity::{EntityData, player::inventory::count};
    use ferraloom_data::Item;
    use ferraloom_protocol::component::ItemStack;
    use ferraloom_util::{Vec3, random::RandomSource};
    for (name, expected) in cases("dispense-item:") {
        let mut args = name.split(':').skip(1);
        let seed = args.next().unwrap().parse::<i64>().unwrap();
        let direction = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let mut level = level();
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(seed)), 0);
        let step = direction.step();
        let pos = Vec3::new(
            f64::from(ORIGIN.x) + 0.5 + 0.7 * f64::from(step.x),
            f64::from(ORIGIN.y) + 0.5 + 0.7 * f64::from(step.y),
            f64::from(ORIGIN.z) + 0.5 + 0.7 * f64::from(step.z),
        );
        let id = dispenser::spawn_item(
            &mut level,
            ItemStack::new(Item::STONE, 1, Default::default()),
            6,
            direction,
            pos,
        )
        .unwrap();
        let item = level.entity::<EntityData>(id).unwrap();
        let pos = item.position;
        let v = item.velocity;
        let actual = format!(
            "{:x},{:x},{:x}/{:x},{:x},{:x}/{}",
            pos.x.to_bits(),
            pos.y.to_bits(),
            pos.z.to_bits(),
            v.x.to_bits(),
            v.y.to_bits(),
            v.z.to_bits(),
            level.random().next_long()
        );
        assert_eq!(actual, expected, "{name}");
        assert_eq!(
            count(
                level
                    .entity::<crate::entity::item::ItemEntity>(id)
                    .unwrap()
                    .item()
            ),
            1
        );
    }
    for (name, expected) in cases("projectile-config:") {
        let mut args = name.split(':').skip(1);
        let item = Item::from_protocol_id(args.next().unwrap().parse().unwrap()).unwrap();
        let direction = Direction::VALUES[args.next().unwrap().parse::<usize>().unwrap()];
        let config = item.dispenser_properties().projectile.unwrap();
        let pos = dispenser::projectile_position(ORIGIN, direction, config.position);
        assert_eq!(
            format!(
                "{:x},{:x},{:x}/{:x},{:x}/{}",
                pos.x.to_bits(),
                pos.y.to_bits(),
                pos.z.to_bits(),
                config.power.to_bits(),
                config.uncertainty.to_bits(),
                config
                    .override_dispense_event
                    .map_or("none".into(), |v| v.to_string())
            ),
            expected,
            "{name}"
        );
    }
}

#[test]
fn dispenser_dropper_quasi_triggers_and_four_tick_dedup_match_vanilla() {
    use ferraloom_data::properties as p;
    for (name, expected) in cases("dispenser-trigger:") {
        let mut args = name.split(':').skip(1);
        let block = Block::from_protocol_id(args.next().unwrap().parse().unwrap()).unwrap();
        let layout = args.next().unwrap().parse::<u8>().unwrap();
        let mut level = level();
        level
            .set_block_state(ORIGIN, block.default_state())
            .unwrap();
        let power = match layout {
            0 => ORIGIN.below(1),
            1 => ORIGIN.above(1),
            2 => ORIGIN.above(2),
            _ => ORIGIN.relative(Direction::East, 1),
        };
        let mut samples = Vec::new();
        for tick in 0..=7 {
            level.set_game_time(tick);
            if matches!(tick, 0 | 1 | 2 | 6) {
                level
                    .set_block(
                        power,
                        if tick == 0 || tick == 2 {
                            Block::REDSTONE_BLOCK
                        } else {
                            Block::AIR
                        }
                        .default_state(),
                        UpdateFlags::ALL,
                        UPDATE_LIMIT,
                    )
                    .unwrap();
            }
            let before = format!(
                "{}/{}",
                level
                    .get_block_state(ORIGIN)
                    .unwrap()
                    .get(p::TRIGGERED)
                    .unwrap(),
                level.has_scheduled_block_tick(ORIGIN, block)
            );
            level.neighbor_changed(ORIGIN, Block::STONE, None).unwrap();
            let poked = format!(
                "{}/{}",
                level
                    .get_block_state(ORIGIN)
                    .unwrap()
                    .get(p::TRIGGERED)
                    .unwrap(),
                level.has_scheduled_block_tick(ORIGIN, block)
            );
            level.tick_scheduled_blocks(tick, 65536).unwrap();
            samples.push(format!(
                "{before};{poked};{}",
                level.has_scheduled_block_tick(ORIGIN, block)
            ));
        }
        assert_eq!(samples.join("|"), expected, "{name}");
    }
}
