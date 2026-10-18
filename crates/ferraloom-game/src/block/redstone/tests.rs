use super::*;
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
    random::{LegacyRandomSource, Random},
};
use std::sync::Arc;
const ORIGIN: BlockPos = BlockPos::new(8, 0, 8);
static EMPTY: Behaviors = Behaviors::EMPTY;
fn level() -> Level {
    let mut level = Level::new(Region::new(&EMPTY));
    for x in -1..=1 {
        for z in -1..=1 {
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
    let values = include_str!("../../../tests/java/level/tags.tsv")
        .lines()
        .map(|line| {
            let (name, blocks) = line.split_once('\t').unwrap();
            (
                name.strip_prefix("tag:").unwrap().parse().unwrap(),
                blocks
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|n| Block::from_name(n).unwrap())
                    .collect(),
            )
        })
        .collect();
    level.set_block_tags(Arc::new(
        ferraloom_data::datapack::TagSet::from_tags(
            ferraloom_data::datapack::Tags {
                values,
                diagnostics: Vec::new(),
            },
            ferraloom_data::BLOCKS.len(),
        )
        .unwrap(),
    ));
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
#[test]
fn signal_sources_directions_and_conductor_relays_match_vanilla() {
    for (name, expected) in cases("signal:") {
        let state =
            BlockState::from_id(name.strip_prefix("signal:").unwrap().parse().unwrap()).unwrap();
        let mut level = level();
        level.set_block_state(ORIGIN, state).unwrap();
        let reader = LevelReaderRef::Server(&level);
        let mut values = vec![
            reader.is_signal_source(state).unwrap().to_string(),
            reader.own_signal(state, ORIGIN).unwrap().to_string(),
        ];
        for d in Direction::VALUES {
            values.push(format!(
                "{}:{}:{}:{}",
                reader.get_signal(ORIGIN, d).unwrap(),
                reader.get_direct_signal(ORIGIN, d).unwrap(),
                reader.get_control_input_signal(ORIGIN, d, false).unwrap(),
                reader.get_control_input_signal(ORIGIN, d, true).unwrap()
            ));
        }
        assert_eq!(values.join(";"), expected, "{name}");
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
        let actor = EntityRef(1);
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
                LevelEffect::BlockUpdated { pos, state, .. } => {
                    Some(format!("{}, {}, {}={}", pos.x, pos.y, pos.z, state.id()))
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
