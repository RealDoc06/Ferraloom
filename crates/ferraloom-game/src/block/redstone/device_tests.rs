use super::*;
use crate::{
    block::{BlockBehavior, InteractionResult, UseContext},
    entity::{
        EntityData, EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    },
    level::{Level, effects::LevelEffect},
};
use ferraloom_data::{BlockState, Item, properties as p};
use ferraloom_protocol::{
    component::ItemStack,
    play::{BlockHitResult, InteractionHand, clientbound::Packet},
};
use ferraloom_util::{
    BlockPos, Direction, Uuid, Vec3,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::{io::Read, sync::LazyLock};
const POS: BlockPos = BlockPos::new(8, 64, 8);
const PLAYER: EntityRef = EntityRef(1);
fn cases(prefix: &str) -> impl Iterator<Item = (&'static str, &'static str)> + use<'_> {
    static DATA: LazyLock<String> = LazyLock::new(|| {
        let mut output = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../../../tests/java/blocks/redstone/devices.tsv.gz")[..],
        )
        .read_to_string(&mut output)
        .unwrap();
        output
    });
    DATA.lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(move |(name, _)| name.starts_with(prefix))
}
fn level(state: BlockState) -> Level {
    static EMPTY: crate::Behaviors = crate::Behaviors::EMPTY;
    let mut level = tests::level_with_behaviors(1, &EMPTY);
    level
        .set_block_state(POS.below(1), Block::STONE.default_state())
        .unwrap();
    level.set_block_state(POS, state).unwrap();
    level.set_random_streams(Random::Legacy(LegacyRandomSource::new(42)), 0);
    level.take_effects();
    level
}
fn player(level: &mut Level) {
    level.install_shared(Box::new(crate::ServerShared::default()));
    let actor = ServerPlayer::new(
        Uuid::new(1, 2),
        "redstone".into(),
        1,
        Vec3::new(8.5, 64.5, 8.5),
        GameType::Survival,
    );
    level
        .add_entity(PLAYER, EntityState::Player(Box::new(actor)))
        .unwrap();
    level.take_effects();
}
fn context(face: Direction) -> UseContext {
    UseContext {
        player: PLAYER,
        hand: InteractionHand::MainHand,
        hit: BlockHitResult {
            block_pos: POS,
            direction: face,
            offset_x: 0.5,
            offset_y: 0.5,
            offset_z: 0.5,
            inside: false,
            world_border_hit: false,
        },
    }
}
fn state(value: &str) -> BlockState {
    BlockState::from_id(value.parse().unwrap()).unwrap()
}
fn events(level: &Level, effects: &[LevelEffect]) -> String {
    effects
        .iter()
        .filter_map(|effect| match effect {
            LevelEffect::PlayerPacket { packet, recipients } => match packet.as_ref() {
                Packet::LevelEvent(event) => Some(format!(
                    "L:{}:{}:{}",
                    event.event_type,
                    event.data,
                    i32::from(event.event_type == 2001 && recipients.is_empty())
                )),
                _ => None,
            },
            LevelEffect::GameEvent {
                event,
                source,
                affected_state,
                ..
            } => Some(format!(
                "{}:{}:{}",
                event.protocol_id(),
                source.map_or(0, |id| level.entity::<EntityData>(id).unwrap().network_id),
                affected_state.map_or(-1, |s| i32::from(s.id()))
            )),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(";")
}
#[test]
fn daylight_signal_math_matches_native_angles_brightness_and_inversion() {
    let mut count = 0;
    for (name, expected) in cases("daylight:") {
        let args: Vec<_> = name.split(':').skip(1).collect();
        assert_eq!(
            device::daylight_power(
                args[0].parse().unwrap(),
                args[1].parse().unwrap(),
                args[2].parse().unwrap()
            )
            .to_string(),
            expected,
            "{name}"
        );
        count += 1;
    }
    assert_eq!(count, 11552);
}
#[test]
fn note_tuning_and_top_instrument_use_match_native() {
    use crate::stats::{Stat, custom};
    for (name, expected) in cases("note:") {
        let args: Vec<_> = name.split(':').skip(1).collect();
        let original = state(args[0]);
        let mut level = level(original);
        if args[1] == "true" {
            level
                .set_block_state(POS.above(1), Block::STONE.default_state())
                .unwrap();
        }
        player(&mut level);
        let result = note::NOTE
            .use_without_item(original, POS, &context(Direction::Up), &mut level)
            .unwrap();
        let effects = level.take_effects();
        let expected: Vec<_> = expected.split('|').collect();
        assert_eq!(result, InteractionResult::SUCCESS, "{name}");
        assert_eq!(
            level.get_block_state(POS).unwrap().id().to_string(),
            expected[1],
            "{name}"
        );
        let gameplay = expected[2]
            .split(';')
            .filter(|event| !event.starts_with("B:"))
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(events(&level, &effects), gameplay, "{name}");
        assert_eq!(
            level
                .entity::<ServerPlayer>(PLAYER)
                .unwrap()
                .stats
                .get_value(Stat::Custom(custom::TUNE_NOTEBLOCK)),
            1,
            "{name}"
        );
    }
    let mut level = level(Block::NOTE_BLOCK.default_state());
    player(&mut level);
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
    let resources = ferraloom_data::datapack::ResourceManager::new(vec![
        ferraloom_data::datapack::Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap(),
    ]);
    level.set_item_tags(std::sync::Arc::new(
        ferraloom_data::datapack::TagSet::load(&resources).unwrap(),
    ));
    for (name, expected) in cases("note-item:") {
        let args: Vec<_> = name.split(':').skip(1).collect();
        let item = Item::from_protocol_id(args[0].parse().unwrap()).unwrap();
        level
            .entity_mut::<ServerPlayer>(PLAYER)
            .unwrap()
            .set_item_in_hand(
                InteractionHand::MainHand,
                ItemStack::new(item, args[1].parse().unwrap(), Default::default()),
            );
        let result = note::NOTE
            .use_item_on(
                Block::NOTE_BLOCK.default_state(),
                POS,
                &context(Direction::VALUES[args[2].parse::<usize>().unwrap()]),
                &mut level,
            )
            .unwrap();
        assert_eq!(
            result,
            if expected == "pass" {
                InteractionResult::Pass
            } else {
                InteractionResult::TryWithEmptyHand
            },
            "{name}"
        );
    }
}
#[test]
fn lightning_rod_activation_eight_tick_release_and_removal_match_native() {
    for (name, expected) in cases("rod:") {
        let args: Vec<_> = name.split(':').skip(1).collect();
        let original = state(args[0]);
        let mut level = level(original);
        match args[1] {
            "0" => device::DEVICES
                .on_place(original, POS, Block::AIR.default_state(), false, &mut level)
                .unwrap(),
            "1" => device::on_lightning_strike(original, POS, &mut level).unwrap(),
            _ => device::DEVICES
                .affect_neighbors_after_removal(original, POS, false, &mut level)
                .unwrap(),
        }
        let (timeline, random) = expected.rsplit_once('|').unwrap();
        for (tick, sample) in timeline.split('#').enumerate() {
            level.tick_block_phase(tick as i64, 65536).unwrap();
            let parts: Vec<_> = sample.split('|').collect();
            assert_eq!(
                format!(
                    "{}:{}",
                    level.get_block_state(POS).unwrap().id(),
                    level.has_scheduled_block_tick(POS, original.block())
                ),
                parts[0],
                "{name} tick{tick}"
            );
            let effects = level.take_effects();
            let mutations = effects
                .iter()
                .filter_map(|effect| match effect {
                    LevelEffect::BlockUpdated { pos, new, .. } => {
                        Some(format!("{}, {}, {}={}", pos.x, pos.y, pos.z, new.id()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(";");
            let expected_mutations = parts[1]
                .split(';')
                .filter(|part| !part.is_empty())
                .map(|part| part.split_once('/').unwrap().0)
                .collect::<Vec<_>>()
                .join(";");
            assert_eq!(mutations, expected_mutations, "{name} tick{tick}");
            assert_eq!(events(&level, &effects), parts[3], "{name} tick{tick}");
        }
        assert_eq!(level.random().next_long().to_string(), random, "{name}");
    }
}
#[test]
fn door_and_head_destroy_follow_native_player_abilities_and_tool_order() {
    for (name, expected) in cases("door:") {
        let args: Vec<_> = name.split(':').skip(1).collect();
        let original = state(args[0]);
        let mut level = level(original);
        player(&mut level);
        let actor = level.entity_mut::<ServerPlayer>(PLAYER).unwrap();
        actor.instabuild = args[1] == "1";
        if args[1] == "2" {
            actor.inventory.set_selected_item(ItemStack::new(
                Item::IRON_PICKAXE,
                1,
                Default::default(),
            ));
        }
        if args[2] != "0" {
            level
                .set_block_state(
                    POS.below(1),
                    if args[2] == "1" {
                        original.with(p::HALF, p::Half::Lower).unwrap()
                    } else {
                        Block::STONE.default_state()
                    },
                )
                .unwrap();
        }
        level.take_effects();
        let returned = level.player_will_destroy(original, POS, PLAYER).unwrap();
        let effects = level.take_effects();
        assert_eq!(
            format!(
                "{}|{}|{}",
                returned.id(),
                level.get_block_state(POS.below(1)).unwrap().id(),
                events(&level, &effects)
            ),
            expected,
            "{name}"
        );
    }
    for (name, expected) in cases("head:") {
        let args: Vec<_> = name.split(':').skip(1).collect();
        let original = state(args[0]);
        let mut level = level(original);
        player(&mut level);
        level.entity_mut::<ServerPlayer>(PLAYER).unwrap().instabuild = args[2] == "true";
        let kind = args[1].parse::<i32>().unwrap();
        let pos = POS.relative(state::facing(original).opposite(), 1);
        let base = if kind == 0 {
            Block::AIR.default_state()
        } else {
            state::with_facing(
                if kind < 3 {
                    Block::PISTON
                } else {
                    Block::STICKY_PISTON
                }
                .default_state(),
                state::facing(original),
            )
            .with(p::EXTENDED, kind % 2 == 0)
            .unwrap()
        };
        level.set_block_state(pos, base).unwrap();
        level.take_effects();
        let returned = level.player_will_destroy(original, POS, PLAYER).unwrap();
        let effects = level.take_effects();
        let event_string = events(&level, &effects);
        assert_eq!(
            format!(
                "{}|{}|{}",
                returned.id(),
                level.get_block_state(pos).unwrap().id(),
                event_string
            ),
            expected,
            "{name}"
        );
    }
}
#[test]
fn redstone_ore_careful_steps_use_results_decay_and_rng_match_native() {
    for (name, expected) in cases("ore-step:") {
        let args: Vec<_> = name.split(':').skip(1).collect();
        let original = state(args[0]);
        let mut level = level(original);
        let mask = args[1].parse::<usize>().unwrap();
        for (index, direction) in Direction::VALUES.into_iter().enumerate() {
            if mask & (1 << index) != 0 {
                level
                    .set_block_state(POS.relative(direction, 1), Block::STONE.default_state())
                    .unwrap();
            }
        }
        player(&mut level);
        level
            .entity_mut::<EntityData>(PLAYER)
            .unwrap()
            .set_shared_flag(1, args[2] == "true");
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(42)), 0);
        ore::ORE.step_on(original, POS, PLAYER, &mut level).unwrap();
        let parts: Vec<_> = expected.split('|').collect();
        assert_eq!(
            level.get_block_state(POS).unwrap().id().to_string(),
            parts[0],
            "{name}"
        );
        // ServerLevel inherits Level#addParticle's no-op. The native oracle also
        // records its call coordinates, while gameplay observes these RNG draws.
        assert_eq!(level.random().next_long().to_string(), parts[2], "{name}");
    }
    for (name, expected) in cases("ore-tick:") {
        let original = state(name.split_once(':').unwrap().1);
        let mut level = level(original);
        ore::ORE.random_tick(original, POS, &mut level).unwrap();
        assert_eq!(
            level.get_block_state(POS).unwrap().id().to_string(),
            expected,
            "{name}"
        );
    }
    for (name, expected) in cases("ore-use:") {
        let args: Vec<_> = name.split(':').skip(1).collect();
        let original = state(args[0]);
        let mut level = level(original);
        player(&mut level);
        let direction = Direction::VALUES[args[2].parse::<usize>().unwrap()];
        if args[3] == "true" {
            level
                .set_block_state(POS.relative(direction, 1), Block::STONE.default_state())
                .unwrap();
        }
        let item = Item::from_protocol_id(args[1].parse().unwrap()).unwrap();
        level
            .entity_mut::<ServerPlayer>(PLAYER)
            .unwrap()
            .inventory
            .set_selected_item(ItemStack::new(item, 1, Default::default()));
        level.set_random_streams(Random::Legacy(LegacyRandomSource::new(42)), 0);
        let result = ore::ORE
            .use_item_on(original, POS, &context(direction), &mut level)
            .unwrap();
        assert_eq!(
            format!(
                "{}|{}|{}",
                if result == InteractionResult::Pass {
                    "pass"
                } else {
                    "success"
                },
                level.get_block_state(POS).unwrap().id(),
                level.random().next_long()
            ),
            expected,
            "{name}"
        );
    }
}
