use super::*;
use crate::{
    Behaviors,
    entity::{EntityState, player::GameType},
    level::{LevelContext, Region},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::codec::Holder;
use ferraloom_util::{Uuid, Vec3};

const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
fn level() -> (Level, EntityRef) {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = {
        let mut fixture_level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
        fixture_level.install_shared(Box::default());
        fixture_level
    };
    level.set_sound_seed(0);
    let player = level.next_entity_ref();
    let data = ServerPlayer::new(
        Uuid::new(0, 1),
        "xp".into(),
        level.next_entity_id().unwrap(),
        Vec3::ZERO,
        GameType::Survival,
    );
    level
        .add_entity(player, EntityState::Player(Box::new(data)))
        .unwrap();
    level.take_effects();
    (level, player)
}
fn progress(value: &str) -> f32 {
    if value == "NaN" {
        f32::NAN
    } else {
        value.parse().unwrap()
    }
}

#[test]
fn experience_curve_awards_cache_invalidation_and_level_up_sounds_match_native_bits() {
    let mut counts = (0, 0);
    for row in VECTORS.lines() {
        let fields: Vec<_> = row.split('\t').collect();
        match fields[0] {
            "xp-curve" => {
                let (mut level, player) = level();
                let data = level.entity_mut::<ServerPlayer>(player).unwrap();
                data.experience_level = fields[1].parse().unwrap();
                assert_eq!(
                    data.xp_needed_for_next_level().to_string(),
                    fields[2],
                    "{row}"
                );
                counts.0 += 1;
            }
            "xp-award" => {
                let (mut level, player) = level();
                let data = level.entity_mut::<ServerPlayer>(player).unwrap();
                data.experience_level = fields[2].parse().unwrap();
                data.experience_progress = progress(fields[3]);
                data.total_experience = fields[4].parse().unwrap();
                data.set_score(fields[5].parse().unwrap());
                data.tick_count = fields[7].parse().unwrap();
                data.last_level_up_time = fields[8].parse().unwrap();
                data.last_sent_experience = 23;
                let amount = fields[6].parse().unwrap();
                match fields[1] {
                    "points" => give_experience_points(player, amount, &mut level).unwrap(),
                    "levels" => give_experience_levels(player, amount, &mut level).unwrap(),
                    _ => panic!("unknown native XP operation"),
                }
                let data = level.entity::<ServerPlayer>(player).unwrap();
                assert_eq!(
                    format!(
                        "{}\t{:x}\t{}\t{}\t{}\t{}",
                        data.experience_level,
                        data.experience_progress.to_bits(),
                        data.total_experience,
                        data.score(),
                        data.last_level_up_time,
                        data.last_sent_experience
                    ),
                    fields[9..15].join("\t"),
                    "{row}"
                );
                let sounds: Vec<_> = level
                    .take_effects()
                    .into_iter()
                    .map(|effect| {
                        let LevelEffect::PlayerPacket { recipients, packet } = effect else {
                            panic!("unexpected XP effect")
                        };
                        assert_eq!(recipients, vec![player]);
                        let cb::Packet::Sound(sound) = *packet else {
                            panic!("XP rewards must not send XP before doTick")
                        };
                        let Holder::Reference(id) = sound.sound.0 else {
                            panic!("registered sound")
                        };
                        format!(
                            "{id}:{}:{:x}:{:x}:{:x}",
                            sound.source as i32,
                            sound.volume.to_bits(),
                            sound.pitch.to_bits(),
                            sound.seed as u64
                        )
                    })
                    .collect();
                let sounds = if sounds.is_empty() {
                    "-".to_owned()
                } else {
                    sounds.join(",")
                };
                assert_eq!(sounds, fields[15], "{row}");
                counts.1 += 1;
            }
            _ => (),
        }
    }
    assert_eq!(counts, (15, 25));
}

#[test]
fn player_xp_nbt_preserves_native_unclamped_values_and_metadata_score() {
    let mut count = 0;
    for row in VECTORS.lines().filter(|line| line.starts_with("xp-nbt\t")) {
        let fields: Vec<_> = row.split('\t').collect();
        let mut tag = CompoundTag::new();
        match fields[1] {
            "absent" => (),
            "normal" => {
                tag.insert("XpP", Tag::Float(0.75));
                tag.insert("XpLevel", Tag::Int(17));
                tag.insert("XpTotal", Tag::Int(777));
                tag.insert("Score", Tag::Int(123));
            }
            "negative" => {
                // Vanilla FloatTag.valueOf canonicalizes the fixture's -0F to +0F.
                tag.insert("XpP", Tag::Float(0.0));
                tag.insert("XpLevel", Tag::Int(-5));
                tag.insert("XpTotal", Tag::Int(-9));
                tag.insert("Score", Tag::Int(-20));
            }
            "wrong" => {
                for key in ["XpP", "XpLevel", "XpTotal", "Score"] {
                    tag.insert(key, Tag::String("bad".into()));
                }
            }
            "nan" => {
                tag.insert("XpP", Tag::Float(f32::NAN));
            }
            _ => panic!("unknown native NBT case"),
        }
        tag.insert("future", Tag::Int(42));
        let (mut level, player) = level();
        let data = level.entity_mut::<ServerPlayer>(player).unwrap();
        data.try_load(tag).unwrap();
        let saved = data.save_without_id();
        assert_eq!(
            format!(
                "{}\t{:x}\t{}\t{}",
                saved.get_int("XpLevel").unwrap(),
                saved.get_float("XpP").unwrap().to_bits(),
                saved.get_int("XpTotal").unwrap(),
                saved.get_int("Score").unwrap()
            ),
            fields[2..].join("\t"),
            "{row}"
        );
        assert_eq!(saved.get_int("future"), Some(42));
        assert!(!saved.contains_key("lastSentExp") && !saved.contains_key("lastLevelUpTime"));
        count += 1;
    }
    assert_eq!(count, 5);
}

#[test]
fn do_tick_xp_slot_sends_initial_and_invalidated_values_once_in_order() {
    let (mut level, player) = level();
    send_experience_update(player, &mut level).unwrap();
    send_experience_update(player, &mut level).unwrap();
    give_experience_points(player, 8, &mut level).unwrap();
    send_experience_update(player, &mut level).unwrap();
    give_experience_levels(player, 1, &mut level).unwrap();
    send_experience_update(player, &mut level).unwrap();
    send_experience_update(player, &mut level).unwrap();
    let packets: Vec<_> = level
        .take_effects()
        .into_iter()
        .map(|effect| {
            let LevelEffect::PlayerPacket { recipients, packet } = effect else {
                panic!("unexpected XP effect")
            };
            assert_eq!(recipients, vec![player]);
            let cb::Packet::SetExperience(packet) = *packet else {
                panic!("only XP packets expected before level-up sound threshold")
            };
            (
                packet.experience_progress.to_bits(),
                packet.total_experience.0,
                packet.experience_level.0,
            )
        })
        .collect();
    let after_eight = 0x3de3_8e3e;
    assert_eq!(
        packets,
        [(0, 0, 0), (after_eight, 8, 1), (after_eight, 8, 2)]
    );
}
