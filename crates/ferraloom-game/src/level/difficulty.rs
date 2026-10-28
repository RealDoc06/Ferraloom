//! Regional difficulty uses world-clock age, chunk inhabited time and moon brightness.
use ferraloom_protocol::play::Difficulty;

/// Port of net.minecraft.world.DifficultyInstance. Preserve native float order:
/// world age and inhabited time are converted to float before normalization.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DifficultyInstance {
    base: Difficulty,
    effective_difficulty: f32,
}
impl DifficultyInstance {
    pub fn new(
        base: Difficulty,
        total_game_time: i64,
        local_game_time: i64,
        moon_brightness: f32,
    ) -> Self {
        let effective_difficulty = if base == Difficulty::Peaceful {
            0.0
        } else {
            let global_scale =
                ((total_game_time as f32 - 72000.0) / 1440000.0).clamp(0.0, 1.0) * 0.25;
            let mut scale = 0.75 + global_scale;
            let mut local_scale = 0.0;
            local_scale += (local_game_time as f32 / 3600000.0).clamp(0.0, 1.0)
                * if base == Difficulty::Hard { 1.0 } else { 0.75 };
            local_scale += (moon_brightness * 0.25).clamp(0.0, global_scale);
            if base == Difficulty::Easy {
                local_scale *= 0.5;
            }
            scale += local_scale;
            let id = match base {
                Difficulty::Peaceful => 0,
                Difficulty::Easy => 1,
                Difficulty::Normal => 2,
                Difficulty::Hard => 3,
            };
            id as f32 * scale
        };
        Self {
            base,
            effective_difficulty,
        }
    }
    pub fn get_difficulty(self) -> Difficulty {
        self.base
    }
    pub fn get_effective_difficulty(self) -> f32 {
        self.effective_difficulty
    }
    pub fn is_hard(self) -> bool {
        self.effective_difficulty >= 3.0
    }
    pub fn is_harder_than(self, required_difficulty: f32) -> bool {
        self.effective_difficulty > required_difficulty
    }
    pub fn get_special_multiplier(self) -> f32 {
        if self.effective_difficulty < 2.0 {
            0.0
        } else if self.effective_difficulty > 4.0 {
            1.0
        } else {
            (self.effective_difficulty - 2.0) / 2.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_difficulty_float_order_and_thresholds() {
        let mut count = 0;
        for row in include_str!("../../tests/java/level/difficulty.tsv").lines() {
            let fields: Vec<_> = row.split('\t').collect();
            let base = match fields[0] {
                "0" => Difficulty::Peaceful,
                "1" => Difficulty::Easy,
                "2" => Difficulty::Normal,
                "3" => Difficulty::Hard,
                _ => panic!("{row}"),
            };
            let value = DifficultyInstance::new(
                base,
                fields[1].parse().unwrap(),
                fields[2].parse().unwrap(),
                f32::from_bits(fields[3].parse().unwrap()),
            );
            assert_eq!(value.get_difficulty(), base);
            assert_eq!(
                value.get_effective_difficulty().to_bits(),
                fields[4].parse::<u32>().unwrap(),
                "{row}"
            );
            assert_eq!(
                value.get_special_multiplier().to_bits(),
                fields[5].parse::<u32>().unwrap(),
                "{row}"
            );
            assert_eq!(value.is_hard(), fields[6] == "true", "{row}");
            assert_eq!(value.is_harder_than(3.0), fields[7] == "true", "{row}");
            count += 1;
        }
        assert_eq!(count, 600);
    }
}
