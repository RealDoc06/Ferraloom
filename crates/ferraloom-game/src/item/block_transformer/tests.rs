use super::*;
use crate::level::game_event::test_support::{self as game_events, Observation};
use crate::{item::stack::tests::level, level::effects::LevelEffect};
use ferraloom_data::{Block, BlockState};
use ferraloom_protocol::{component::ItemStack, play::clientbound as cb};
use ferraloom_util::{
    BlockPos,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::io::Read;

#[test]
fn native_sound_holders_keep_direct_ranges_and_missing_references_required() {
    let (mut level, _) = level();
    let mut cases = 0;
    for line in include_str!("../../../tests/java/items/sound-holders.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let value: Value = serde_json::from_str(fields[1]).unwrap();
        if fields[0] == "missing" {
            assert!(decode_sound(Some(&value)).is_err());
            let sound = SoundEventHolder(Holder::Reference("fixture:missing".parse().unwrap()));
            assert!(matches!(
                level.play_seeded_sound_holder(
                    None,
                    Vec3::new(8.5, 5.0, 8.5),
                    sound,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                    99
                ),
                Err(LevelError::MissingSoundEvent(_))
            ));
            continue;
        }
        let sound = decode_sound(Some(&value)).unwrap();
        let volume = f32::from_bits(fields[2].parse::<i32>().unwrap() as u32);
        let range = f32::from_bits(fields[3].parse::<i32>().unwrap() as u32);
        assert_eq!(
            matches!(sound.0, Holder::Reference(_)),
            fields[4] == "REFERENCE",
            "{line}"
        );
        level.take_effects();
        level
            .play_seeded_sound_holder(
                None,
                Vec3::new(8.5, 5.0, 8.5),
                sound.clone(),
                SoundSource::Blocks,
                volume,
                1.0,
                99,
            )
            .unwrap();
        let packets: Vec<_> = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                LevelEffect::PlayerPacket { recipients, packet } => Some((recipients, packet)),
                _ => None,
            })
            .collect();
        assert_eq!(packets.len(), 1);
        // The fixture player is exactly four blocks from the emitter. Native
        // audience checks square the radius, retaining negative fixed ranges.
        assert_eq!(
            !packets[0].0.is_empty(),
            16.0 < f64::from(range) * f64::from(range),
            "{line}"
        );
        let cb::Packet::Sound(packet) = packets[0].1.as_ref() else {
            panic!("missing sound");
        };
        assert_eq!(packet.sound, sound, "{line}");
        assert_eq!(packet.volume.to_bits(), volume.to_bits());
        assert_eq!(packet.seed, 99);
        cases += 1;
    }
    assert_eq!(cases, 30);
}

#[test]
fn native_transformer_providers_preserve_properties_effect_order_and_rng() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/items/transformers.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let (mut level, player) = level();
    game_events::observe(&mut level);
    let pos = BlockPos::new(8, 5, 8);
    let mut cases = 0;
    for line in vectors.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        let item = Item::from_protocol_id(fields[1].parse().unwrap()).unwrap();
        let state = BlockState::from_id(fields[2].parse().unwrap()).unwrap();
        for direction in Direction::VALUES {
            level
                .set_block_state(pos.relative(direction, 1), Block::AIR.default_state())
                .unwrap();
        }
        level
            .set_block_state(pos.below(1), Block::STONE.default_state())
            .unwrap();
        level.set_block_state(pos, state).unwrap();
        if fields[4] == "true" {
            level
                .set_block_state(pos.above(1), Block::STONE.default_state())
                .unwrap();
        }
        let mut operation = StackOperation::new(&mut level);
        let receiver = operation
            .own(player, ItemStack::new(item, 3, Default::default()))
            .unwrap();
        let context = UseOnContext::new_retained(
            None,
            InteractionHand::OffHand,
            receiver,
            crate::block::PlacementHit {
                pos,
                face: Direction::VALUES[fields[3].parse::<usize>().unwrap()],
                location: Vec3::new(8.5, 5.5, 8.5),
                inside: false,
            },
        );
        game_events::take(operation.level);
        *operation.level.random() = Random::Legacy(LegacyRandomSource::new(1234));
        let result = crate::item::stack::use_on(receiver, &context, &mut operation)
            .unwrap()
            .unwrap();
        assert_eq!(
            if result.consumes_action() {
                "success"
            } else {
                "pass"
            },
            fields[5],
            "{line}"
        );
        assert_eq!(
            operation.level.get_block_state(pos).unwrap().id(),
            fields[6].parse::<u16>().unwrap(),
            "{line}"
        );
        assert_eq!(
            inventory::count(operation.level.stack(receiver).unwrap()),
            fields[7].parse::<i32>().unwrap(),
            "{line}"
        );
        let actions: Vec<_> = game_events::take(operation.level)
            .into_iter()
            .filter_map(|effect| match effect {
                Observation::Effect(LevelEffect::PlayerPacket { packet, .. }) => {
                    match packet.as_ref() {
                        cb::Packet::Sound(sound) => {
                            let Holder::Reference(name) = &sound.sound.0 else {
                                panic!("builtin sound");
                            };
                            Some(format!(
                                "sound:{name}:{}:{}",
                                sound.volume.to_bits() as i32,
                                sound.pitch.to_bits() as i32
                            ))
                        }
                        cb::Packet::LevelEvent(event) => {
                            Some(format!("level-event:{}:{}", event.event_type, event.data))
                        }
                        _ => None,
                    }
                }
                Observation::GameEvent {
                    event,
                    affected_state,
                    ..
                } => Some(format!(
                    "event:{}:{}",
                    event.name(),
                    affected_state.unwrap().id()
                )),
                _ => None,
            })
            .collect();
        assert_eq!(actions.join(";"), fields[8], "{line}");
        assert_eq!(
            operation.level.random().next_long(),
            fields[9].parse::<i64>().unwrap(),
            "{line}"
        );
        assert_eq!(operation.level.unported_behavior_stats().skipped_calls, 0);
        cases += 1;
    }
    assert_eq!(cases, 324);
}
