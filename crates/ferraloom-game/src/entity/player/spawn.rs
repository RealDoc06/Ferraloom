use super::ServerPlayer;
use ferraloom_nbt::{CompoundTag, Tag};

impl ServerPlayer {
    /// Port of PrepareSpawnTask's SavedPosition.MAP_CODEC rotation selection.
    /// Capture the raw saved angles before Entity#load wraps them; absent or
    /// undecodable rotation uses the shared respawn data at the caller.
    pub fn saved_spawn_rotation(tag: Option<&CompoundTag>) -> Option<(f32, f32)> {
        let values = tag?.get("Rotation")?;
        let [yaw, pitch] = match values {
            Tag::List(values) => prefix(values.iter().filter_map(Tag::as_float)),
            Tag::ByteArray(values) => prefix(values.iter().map(|value| f32::from(*value))),
            Tag::IntArray(values) => prefix(values.iter().map(|value| *value as f32)),
            Tag::LongArray(values) => prefix(values.iter().map(|value| *value as f32)),
            _ => None,
        }?;
        Some((yaw, pitch))
    }
}

// Codec.FLOAT.listOf retains successfully decoded entries, and Util.fixedSize
// preserves an oversized list's first two values as its partial result.
fn prefix(mut values: impl Iterator<Item = f32>) -> Option<[f32; 2]> {
    Some([values.next()?, values.next()?])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::player::GameType;
    use ferraloom_util::{Uuid, Vec3, random::RandomSource};

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
