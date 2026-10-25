use super::ServerPlayer;
use ferraloom_nbt::{CompoundTag, Tag};

/// Port of ServerPlayer.SavedPosition.MAP_CODEC. Partial field values are
/// retained independently, exactly as TagValueInput#read consumes them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedPosition {
    pub dimension: Option<ferraloom_protocol::Identifier>,
    pub position: Option<ferraloom_util::Vec3>,
    pub rotation: Option<(f32, f32)>,
}
impl SavedPosition {
    /// Port of PrepareSpawnTask#start's nullable dimension selection. The
    /// validated server always owns an overworld; lookup/acquire errors after
    /// this native nullable choice are not swallowed by another fallback.
    pub fn resolve_dimension(
        &self,
        respawn: &ferraloom_protocol::Identifier,
        contains: impl Fn(&ferraloom_protocol::Identifier) -> bool,
    ) -> ferraloom_protocol::Identifier {
        self.dimension
            .as_ref()
            .filter(|key| contains(key))
            .or_else(|| contains(respawn).then_some(respawn))
            .cloned()
            .unwrap_or_else(|| "minecraft:overworld".parse().expect("native dimension key"))
    }
}
impl ServerPlayer {
    /// Port of ServerPlayer.SavedPosition.MAP_CODEC as read by PrepareSpawnTask.
    pub fn saved_position(tag: Option<&CompoundTag>) -> SavedPosition {
        let Some(tag) = tag else {
            return SavedPosition::default();
        };
        let dimension = tag
            .get_string("Dimension")
            .and_then(|name| name.to_string_checked().ok())
            .and_then(|name| name.parse().ok());
        let position = tag.get("Pos").and_then(|values| {
            let [x, y, z] = match values {
                Tag::List(values) => prefix(values.iter().filter_map(Tag::as_double)),
                Tag::ByteArray(values) => prefix(values.iter().map(|&value| f64::from(value))),
                Tag::IntArray(values) => prefix(values.iter().map(|&value| f64::from(value))),
                Tag::LongArray(values) => prefix(values.iter().map(|&value| value as f64)),
                _ => None,
            }?;
            Some(ferraloom_util::Vec3::new(x, y, z))
        });
        let rotation = tag.get("Rotation").and_then(|values| {
            let [yaw, pitch] = match values {
                Tag::List(values) => prefix(values.iter().filter_map(Tag::as_float)),
                Tag::ByteArray(values) => prefix(values.iter().map(|&value| f32::from(value))),
                Tag::IntArray(values) => prefix(values.iter().map(|&value| value as f32)),
                Tag::LongArray(values) => prefix(values.iter().map(|&value| value as f32)),
                _ => None,
            }?;
            Some((yaw, pitch))
        });
        SavedPosition {
            dimension,
            position,
            rotation,
        }
    }
    /// PrepareSpawnTask captures raw angles before Entity#load wraps them.
    pub fn saved_spawn_rotation(tag: Option<&CompoundTag>) -> Option<(f32, f32)> {
        Self::saved_position(tag).rotation
    }
}

// Numeric list codecs retain successful elements and fixedSize retains an
// oversized prefix as its partial value. Do not convert longs through double
// when decoding FLOAT: Number#floatValue must retain its direct rounding.
fn prefix<T, const N: usize>(values: impl Iterator<Item = T>) -> Option<[T; N]> {
    values.take(N).collect::<Vec<_>>().try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::player::GameType;
    use ferraloom_util::{Uuid, Vec3, random::RandomSource};

    #[test]
    fn full_saved_position_matches_native_optional_fields_and_partial_record_values() {
        let mut count = 0;
        for row in include_str!("../../../tests/java/entity/saved-position.tsv").lines() {
            let fields: Vec<_> = row.split('\t').collect();
            let tag = (fields[0] != "-").then(|| ferraloom_nbt::parse_compound(fields[0]).unwrap());
            let parsed = ServerPlayer::saved_position(tag.as_ref());
            assert_eq!(
                parsed.dimension.as_ref().map_or("-", |id| id.as_str()),
                fields[1],
                "{row}"
            );
            assert_eq!(
                parsed.position.map_or_else(
                    || "-".into(),
                    |p| format!(
                        "{:x},{:x},{:x}",
                        p.x.to_bits(),
                        p.y.to_bits(),
                        p.z.to_bits()
                    )
                ),
                fields[2],
                "{row}"
            );
            assert_eq!(
                parsed.rotation.map_or_else(
                    || "-".into(),
                    |(a, b)| format!("{:x},{:x}", a.to_bits(), b.to_bits())
                ),
                fields[3],
                "{row}"
            );
            count += 1;
        }
        assert_eq!(count, 393);
    }
    #[test]
    fn spawn_dimension_uses_loaded_then_respawn_then_explicit_overworld() {
        let respawn = "minecraft:the_end".parse().unwrap();
        for (saved, available, expected) in [
            (
                Some("minecraft:the_nether"),
                vec![
                    "minecraft:overworld",
                    "minecraft:the_nether",
                    "minecraft:the_end",
                ],
                "minecraft:the_nether",
            ),
            (
                Some("fixture:absent"),
                vec!["minecraft:overworld", "minecraft:the_end"],
                "minecraft:the_end",
            ),
            (
                None,
                vec!["minecraft:overworld", "minecraft:the_end"],
                "minecraft:the_end",
            ),
            (
                Some("fixture:absent"),
                vec!["minecraft:overworld"],
                "minecraft:overworld",
            ),
        ] {
            let saved = SavedPosition {
                dimension: saved.map(|id| id.parse().unwrap()),
                ..Default::default()
            };
            assert_eq!(
                saved
                    .resolve_dimension(&respawn, |id| available.contains(&id.as_str()))
                    .as_str(),
                expected
            );
        }
    }
    fn angles(values: &[f32]) -> String {
        values
            .iter()
            .map(|value| format!("{:x}", value.to_bits()))
            .collect::<Vec<_>>()
            .join(",")
    }
    fn position(value: Vec3) -> String {
        format!(
            "{:x},{:x},{:x}",
            value.x.to_bits(),
            value.y.to_bits(),
            value.z.to_bits()
        )
    }

    #[test]
    fn native_spawn_rotation_uses_saved_or_shared_respawn_angles_then_snaps() {
        let vectors: Vec<_> = include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("player-spawn-rotation\t"))
            .collect();
        assert!(!vectors.is_empty());
        let mut actual = Vec::new();
        for vector in &vectors {
            let fields: Vec<_> = vector.split('\t').collect();
            let saved =
                (fields[2] != "-").then(|| ferraloom_nbt::parse_compound(fields[2]).unwrap());
            let loaded = ServerPlayer::saved_spawn_rotation(saved.as_ref());
            let (yaw, pitch) = fields[3].split_once(',').unwrap();
            let selected = loaded.unwrap_or((yaw.parse().unwrap(), pitch.parse().unwrap()));
            let mut player = ServerPlayer::new(
                Uuid::new(1, 2),
                "spawn".into(),
                1,
                Vec3::ZERO,
                GameType::Survival,
            );
            player.set_y_rot(4.214188);
            player.set_x_rot(5.0);
            if let Some(saved) = saved {
                player.try_load(saved).unwrap();
            }
            // Isolate the final snap's RNG behavior from other player load phases.
            player.random.set_seed(12345);
            player.last_known_position = Some(Vec3::ZERO);
            let destination = Vec3::new(8.5, 64.25, -12.75);
            player.snap_to(destination, selected.0, selected.1);
            assert!(player.last_known_position.is_none());
            assert_eq!(
                player.bounding_box,
                player.dimensions.make_bounding_box(destination)
            );
            actual.push(format!(
                "player-spawn-rotation\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:x}",
                fields[1],
                fields[2],
                fields[3],
                loaded.is_some(),
                angles(&[selected.0, selected.1]),
                angles(&[player.yaw, player.pitch, player.old_yaw, player.old_pitch]),
                position(player.position),
                position(player.old_position),
                position(player.interpolation_old_position),
                player.random.next_long()
            ));
        }
        assert_eq!(actual, vectors);
    }
}
