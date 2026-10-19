use super::*;
use crate::{
    block::{UnportedBehavior, UpdateFlags},
    entity::{EntityData, EntityState, player::GameType},
    level::{
        effects::LevelEffect,
        game_event::test_support::{self as game_events, Observation},
        interaction::SpawnProtectionSnapshot,
    },
};
use ferraloom_data::{Block, Item};
use ferraloom_protocol::{
    chat::{Argument, Contents},
    component::{DataComponent, DataComponentPatch, ItemStack, Persistent},
    play::clientbound as cb,
};
use ferraloom_util::{Uuid, Vec3};

const POS: BlockPos = BlockPos::new(8, 64, 8);
const VECTORS: &str = include_str!("../../../../tests/java/level/block-actions.tsv");
fn pos(value: &str) -> BlockPos {
    let values: Vec<i32> = value
        .split(',')
        .map(|value| value.parse().unwrap())
        .collect();
    BlockPos::new(values[0], values[1], values[2])
}
fn vector(value: &str) -> Vec3 {
    let values: Vec<f64> = value
        .split(',')
        .map(|value| value.parse().unwrap())
        .collect();
    Vec3::new(values[0], values[1], values[2])
}
fn position(pos: BlockPos) -> String {
    format!("{},{},{}", pos.x, pos.y, pos.z)
}
fn direction(value: &str) -> Direction {
    match value.to_ascii_lowercase().as_str() {
        "down" => Direction::Down,
        "up" => Direction::Up,
        "north" => Direction::North,
        "south" => Direction::South,
        "west" => Direction::West,
        "east" => Direction::East,
        _ => panic!("direction: {value}"),
    }
}
fn state(value: &BlockBreakingState) -> String {
    let direction = format!("{:?}", value.destroy_direction).to_ascii_lowercase();
    format!(
        "{}/{}/{}/{direction}/{}/{}/{}/{}/{}",
        value.is_destroying_block,
        value.destroy_progress_start,
        position(value.destroy_pos),
        value.game_ticks,
        value.has_delayed_destroy,
        position(value.delayed_destroy_pos),
        value.delayed_tick_start,
        value.last_sent_state
    )
}
fn replace(level: &mut Level, pos: BlockPos, state: ferraloom_data::BlockState) {
    level
        .set_block(pos, state, UpdateFlags::KNOWN_SHAPE, 512)
        .unwrap();
    game_events::take(level);
}
fn setup(fields: &[&str]) -> (Level, EntityRef, EntityRef) {
    let (mut level, player) = super::tests::fixture(false, true);
    assert_eq!(level.min_build_height(), fields[7].parse::<i32>().unwrap());
    assert_eq!(level.max_build_height(), fields[8].parse::<i32>().unwrap());
    let mode = match fields[2] {
        "survival" => GameType::Survival,
        "creative" => GameType::Creative,
        "adventure" => GameType::Adventure,
        "spectator" => GameType::Spectator,
        _ => panic!("mode: {}", fields[2]),
    };
    let miner = level.entity_mut::<ServerPlayer>(player).unwrap();
    miner.game_mode = mode;
    miner.update_game_mode_abilities();
    miner.inventory.set_selected_item(if fields[3] == "-" {
        ItemStack::default()
    } else {
        ItemStack::from_nbt(&ferraloom_nbt::Tag::Compound(
            ferraloom_nbt::parse_compound(fields[3]).unwrap(),
        ))
        .unwrap()
    });
    level.set_entity_pos(player, vector(fields[6])).unwrap();
    let mut border = crate::level::border::WorldBorder::default();
    border.set_size(fields[5].parse().unwrap());
    level.set_entity_world_border(border);
    level.set_spawn_protection(SpawnProtectionSnapshot::new(
        "minecraft:overworld".parse().unwrap(),
        "minecraft:overworld".parse().unwrap(),
        POS,
        16,
        if fields[4] == "true" {
            vec![Uuid::new(100, 100)]
        } else {
            Vec::new()
        },
    ));
    let observer = level.next_entity_ref();
    level
        .add_entity(
            observer,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::offline_player("observer"),
                "observer".into(),
                level.next_entity_id(),
                Vec3::new(8.0, 64.0, 8.0),
                GameType::Spectator,
            ))),
        )
        .unwrap();
    for pos in [POS, POS.relative(Direction::East, 1)] {
        replace(&mut level, pos, Block::STONE.default_state());
    }
    game_events::take(&mut level);
    (level, player, observer)
}
fn trace(level: &mut Level, player: EntityRef, observer: EntityRef) -> String {
    let mut result = Vec::new();
    for effect in game_events::take(level) {
        match effect {
            Observation::Effect(LevelEffect::PlayerPacket { recipients, packet }) => {
                match *packet {
                    cb::Packet::BlockDestruction(packet) => {
                        if recipients.is_empty() {
                            continue;
                        }
                        assert_eq!(recipients, [observer]);
                        // Native keeps int progress until its byte encoder. Sequence
                        // stages here are [-1,10]; independent audience rows cover wrap.
                        result.push(format!(
                            "C:1:{}:{}",
                            position(packet.pos),
                            i32::from(packet.progress as i8)
                        ));
                    }
                    cb::Packet::BlockUpdate(packet) => {
                        assert_eq!(recipients, [player]);
                        result.push(format!(
                            "U:0:{}:{}",
                            position(packet.pos),
                            packet.block_state.0
                        ));
                    }
                    cb::Packet::LevelEvent(packet) => {
                        result.push(format!(
                            "L:{}:{}:{}",
                            packet.event_type,
                            position(packet.pos),
                            packet.data
                        ));
                    }
                    cb::Packet::SystemChat(packet) => {
                        assert_eq!(recipients, [player]);
                        let Contents::Translatable { key, args, .. } = &packet.content.contents
                        else {
                            panic!("overlay contents")
                        };
                        let Argument::String(argument) = &args[0] else {
                            panic!("overlay argument")
                        };
                        assert_eq!(
                            packet.content.style.color.as_ref().unwrap().serialize(),
                            "red"
                        );
                        result.push(format!("S:0:{}:{key}:{argument}:red", packet.overlay));
                    }
                    _ => {}
                }
            }
            Observation::GameEvent {
                event,
                source,
                affected_state,
                ..
            } => result.push(format!(
                "G:{}:{}:{}",
                event.name(),
                affected_state.unwrap().id(),
                if source == Some(player) {
                    "miner"
                } else {
                    "none"
                }
            )),
            _ => {}
        }
    }
    if result.is_empty() {
        "-".into()
    } else {
        result.join(";")
    }
}

#[test]
fn break_actions_match_real_native_sequences_and_packet_order() {
    let mut current = None;
    let mut mining = BlockBreakingState::default();
    let mut count = 0;
    for line in VECTORS.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        match fields[0] {
            "setup" => {
                current = Some(setup(&fields));
                mining = BlockBreakingState::default();
            }
            "step" => {
                let (level, player, observer) = current.as_mut().unwrap();
                let input: Vec<_> = fields[2].split(':').collect();
                match input[0] {
                    "tick" => mining.tick(*player, level).unwrap(),
                    "field" => {
                        assert_eq!(input[1], "gameTicks");
                        mining.game_ticks = input[2].parse().unwrap();
                    }
                    "move" => {
                        level.set_entity_pos(*player, vector(input[1])).unwrap();
                        game_events::take(level);
                    }
                    "replace" => replace(
                        level,
                        pos(input[1]),
                        Block::from_name(&format!("{}:{}", input[2], input[3]))
                            .unwrap()
                            .default_state(),
                    ),
                    action => {
                        let action = match action {
                            "START_DESTROY_BLOCK" => DestroyAction::Start,
                            "ABORT_DESTROY_BLOCK" => DestroyAction::Abort,
                            "STOP_DESTROY_BLOCK" => DestroyAction::Stop,
                            "CHANGE_DESTROY_DIRECTION" => DestroyAction::ChangeDirection,
                            _ => panic!("action: {line}"),
                        };
                        let max_y = level.max_build_height();
                        mining
                            .handle_block_break_action(
                                *player,
                                BlockBreakAction {
                                    action,
                                    pos: pos(input[1]),
                                    direction: direction(input[2]),
                                    max_y,
                                },
                                level,
                            )
                            .unwrap();
                    }
                }
                assert_eq!(state(&mining), fields[3], "{line}");
                assert_eq!(
                    format!(
                        "{},{}",
                        level.get_block_state(POS).unwrap().id(),
                        level
                            .get_block_state(POS.relative(Direction::East, 1))
                            .unwrap()
                            .id()
                    ),
                    fields[4],
                    "{line}"
                );
                assert_eq!(trace(level, *player, *observer), fields[5], "{line}");
                count += 1;
            }
            _ => {}
        }
    }
    assert_eq!(count, 51);
}

#[test]
fn crack_audience_is_captured_includes_spectators_and_encodes_native_byte() {
    let (mut level, player) = super::tests::fixture(false, true);
    level
        .set_entity_pos(player, Vec3::new(8.0, 64.0, 8.0))
        .unwrap();
    let mut others = Vec::new();
    for (index, offset) in [31.999, 32.0, -32.0].into_iter().enumerate() {
        let entity = level.next_entity_ref();
        let name = format!("audience{index}");
        level
            .add_entity(
                entity,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::offline_player(&name),
                    name,
                    level.next_entity_id(),
                    Vec3::new(8.0 + offset, 64.0, 8.0),
                    if index == 0 {
                        GameType::Spectator
                    } else {
                        GameType::Survival
                    },
                ))),
            )
            .unwrap();
        others.push(entity);
    }
    level.take_effects();
    let source_id = level.entity::<EntityData>(player).unwrap().network_id;
    let mut count = 0;
    for line in VECTORS
        .lines()
        .filter(|line| line.starts_with("audience\t"))
    {
        let fields: Vec<_> = line.split('\t').collect();
        let progress: i32 = fields[1].parse().unwrap();
        level
            .destroy_block_progress(source_id, POS, progress)
            .unwrap();
        let effects = level.take_effects();
        let [LevelEffect::PlayerPacket { recipients, packet }] = effects.as_slice() else {
            panic!("{effects:?}")
        };
        assert_eq!(recipients, &[others[0]]);
        let cb::Packet::BlockDestruction(packet) = packet.as_ref() else {
            panic!("{packet:?}")
        };
        assert_eq!(packet.id.0, source_id);
        assert_eq!(packet.progress, progress as u8);
        assert_eq!(fields[2], format!("C:1:8,64,8:{progress}"));
        count += 1;
    }
    assert_eq!(count, 6);
    level.destroy_block_progress(source_id, POS, 0).unwrap();
    level
        .set_entity_pos(others[0], Vec3::new(100.0, 64.0, 8.0))
        .unwrap();
    assert!(
        matches!(&level.take_effects()[0],LevelEffect::PlayerPacket {recipients,..} if recipients==&[others[0]])
    );
}

#[test]
fn pending_item_and_missing_stat_context_preserve_source_side_effects() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let (mut level, player) = super::tests::fixture(false, true);
        level.set_unported_behavior(policy);
        level.set_spawn_protection(SpawnProtectionSnapshot::new(
            "minecraft:overworld".parse().unwrap(),
            "minecraft:overworld".parse().unwrap(),
            POS,
            0,
            [],
        ));
        replace(&mut level, POS, Block::DIRT.default_state());
        let error = destroy_block(player, POS, &mut level).unwrap_err();
        assert!(
            matches!(error,BlockBreakError::Level(ref error) if matches!(error.as_ref(),LevelError::MissingServerShared))
        );
        assert_eq!(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .stats
                .get_value(crate::stats::Stat::Mined(Block::DIRT)),
            1
        );
        assert_eq!(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .food_data
                .exhaustion_level()
                .to_bits(),
            0,
        );
        assert!(level.get_block_state(POS).unwrap().is_air());
        assert!(
            game_events::take(&mut level)
                .iter()
                .any(|event| matches!(event, Observation::GameEvent { .. }))
        );
        replace(&mut level, POS, Block::STONE.default_state());
        level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .inventory
            .set_selected_item(ItemStack::new(
                Item::STICK,
                1,
                DataComponentPatch {
                    added: vec![DataComponent::Enchantments(
                        ferraloom_protocol::component::ItemEnchantments(vec![(
                            ferraloom_protocol::component::Enchantment(
                                "minecraft:efficiency".parse().unwrap(),
                            ),
                            ferraloom_protocol::codec::VarInt(1),
                        )]),
                    )],
                    removed: Vec::new(),
                },
            ));
        let mut mining = BlockBreakingState::default();
        mining.game_ticks = 17;
        let max_y = level.max_build_height();
        assert!(matches!(
            mining.handle_block_break_action(
                player,
                BlockBreakAction {
                    action: DestroyAction::Start,
                    pos: POS,
                    direction: Direction::Up,
                    max_y,
                },
                &mut level
            ),
            Err(BlockBreakError::ItemOnHitBlock(Item::STICK))
        ));
        assert_eq!(mining.destroy_progress_start, 17);
        assert!(!mining.is_destroying_block);
        assert!(level.take_effects().is_empty());
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
}

#[test]
fn adventure_non_loading_state_read_matches_native_absence_without_a_fallback() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        for line in VECTORS
            .lines()
            .filter(|line| line.starts_with("adventure-unloaded\t"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let mut level = Level::new(
                crate::level::Region::new(&crate::Behaviors::EMPTY),
                crate::level::LevelContext::fixture(0, 63),
            );
            level.set_unported_behavior(policy);
            let player = level.next_entity_ref();
            let mut state = ServerPlayer::new(
                Uuid::offline_player("missing"),
                "missing".into(),
                level.next_entity_id(),
                Vec3::new(8.5, 64.0, 8.5),
                GameType::Adventure,
            );
            let added = if fields[1] == "-" {
                Vec::new()
            } else {
                vec![DataComponent::CanBreak(
                    ferraloom_protocol::component::AdventureModePredicate::from_json(
                        &serde_json::from_str(fields[1]).unwrap(),
                    )
                    .unwrap(),
                )]
            };
            state.inventory.set_selected_item(ItemStack::new(
                Item::STICK,
                1,
                DataComponentPatch {
                    added,
                    removed: Vec::new(),
                },
            ));
            level
                .add_entity(player, EntityState::Player(Box::new(state)))
                .unwrap();
            let result = super::block_actions::block_action_restricted(player, POS, &mut level);
            match fields[2] {
                "true" => assert!(result.unwrap(), "{line}"),
                "false" => assert!(!result.unwrap(), "{line}"),
                "NullPointerException" => {
                    assert!(matches!(result,Err(BlockBreakError::Level(error))
                    if matches!(error.as_ref(),LevelError::MissingChunk(chunk) if *chunk == ferraloom_util::ChunkPos::ZERO)))
                }
                _ => panic!("{line}"),
            }
            assert_eq!(fields[3], "0");
            assert!(!level.has_chunk_at(POS));
            assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        }
    }
}
