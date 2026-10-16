//! Player experience arithmetic from the official 26.3 Player/ServerPlayer bytecode.
//!
//! Keep every float operation in order: deriving progress from integer totals loses
//! the rounding accumulated at level boundaries. XpTotal is an independent counter.
//! Primary minecraft.jar SHA-256:
//! a362163eec5d1612d520772bc16e5b39c09e3b234fdc045f56bf544284ee8ae6.

use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::java::client::play::CSetExperience;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Experience {
    pub level: i32,
    pub progress: f32,
    pub total: i32,
}

impl Experience {
    pub fn read_nbt(nbt: &NbtCompound) -> Self {
        Self {
            level: nbt.get_int("XpLevel").unwrap_or(0),
            progress: nbt.get_float("XpP").unwrap_or(0.0),
            total: nbt.get_int("XpTotal").unwrap_or(0),
        }
    }

    pub fn write_nbt(self, nbt: &mut NbtCompound) {
        nbt.put_float("XpP", self.progress);
        nbt.put_int("XpLevel", self.level);
        nbt.put_int("XpTotal", self.total);
    }

    pub const fn packet(self) -> CSetExperience {
        CSetExperience::new(
            self.progress,
            pumpkin_protocol::codec::var_int::VarInt(self.level),
            pumpkin_protocol::codec::var_int::VarInt(self.total),
        )
    }

    pub const fn points_needed(self) -> i32 {
        // Java int arithmetic wraps, including for levels set near i32::MAX.
        if self.level >= 30 {
            112i32.wrapping_add(self.level.wrapping_sub(30).wrapping_mul(9))
        } else if self.level >= 15 {
            37i32.wrapping_add(self.level.wrapping_sub(15).wrapping_mul(5))
        } else {
            7i32.wrapping_add(self.level.wrapping_mul(2))
        }
    }

    pub fn points(self) -> i32 {
        // ExperienceCommand uses Mth.floor after an fmul, not round or XpTotal.
        (self.progress * self.points_needed() as f32).floor() as i32
    }

    pub const fn set_level(&mut self, level: i32) {
        self.level = level;
    }

    pub fn add_levels(&mut self, levels: i32) {
        if levels == 0 {
            return;
        }
        self.level = self.level.saturating_add(levels);
        if self.level < 0 {
            *self = Self::default();
        }
    }

    pub fn set_points(&mut self, points: i32) {
        let needed = self.points_needed() as f32;
        let maximum = (needed - 1.0) / needed;
        let progress = points as f32 / needed;
        // Mth.clamp checks the lower bound, then Math.min with the upper bound.
        // f32::clamp would panic for a negative/NaN bound; f32::min ignores NaN.
        let progress = if progress < 0.0 {
            0.0
        } else if progress.is_nan() || maximum.is_nan() {
            f32::NAN
        } else {
            progress.min(maximum)
        };
        // ServerPlayer's equality check also preserves a saved negative zero.
        if progress != self.progress {
            self.progress = progress;
        }
    }

    pub fn add_points(&mut self, points: i32) {
        // ServerPlayer skips zero awards before Player normalizes the bar.
        if points == 0 {
            return;
        }
        self.progress += points as f32 / self.points_needed() as f32;
        // Vanilla uses iadd, then clamps; overflow is not a saturating add.
        self.total = self.total.wrapping_add(points).max(0);

        while self.progress < 0.0 {
            let remainder = self.progress * self.points_needed() as f32;
            if self.level > 0 {
                self.add_levels(-1);
                self.progress = 1.0 + remainder / self.points_needed() as f32;
            } else {
                self.add_levels(-1);
                self.progress = 0.0;
            }
        }
        while self.progress >= 1.0 {
            self.progress = (self.progress - 1.0) * self.points_needed() as f32;
            self.add_levels(1);
            self.progress /= self.points_needed() as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Experience;
    use pumpkin_nbt::compound::NbtCompound;

    // Retained vanilla/source player save from player-data-bnvpygj5 after +223.
    // The query and +4 continuation expectations are bytecode-derived, not paired.
    const VANILLA_223_BITS: u32 = 0x3e67_39c4;
    const CONTINUATION_227_BITS: u32 = 0x3eb5_ad66;

    #[test]
    fn point_award_matches_retained_java_progress_bits() {
        let mut experience = Experience::default();
        experience.add_points(223);
        assert_eq!(experience.level, 12);
        assert_eq!(experience.total, 223);
        assert_eq!(experience.progress.to_bits(), VANILLA_223_BITS);
        assert_eq!(experience.points(), 6);
    }

    #[test]
    fn player_nbt_continuation_and_wire_keep_total_and_progress_independent() {
        let mut nbt = NbtCompound::new();
        nbt.put_int("XpLevel", 12);
        nbt.put_int("XpTotal", 223);
        nbt.put_float("XpP", f32::from_bits(VANILLA_223_BITS));
        nbt.put_int("XpSeed", -123_456_789);

        let mut experience = Experience::read_nbt(&nbt);
        experience.write_nbt(&mut nbt);
        assert_eq!(
            nbt.get_float("XpP").map(f32::to_bits),
            Some(VANILLA_223_BITS)
        );
        assert_eq!(nbt.get_int("XpTotal"), Some(223));

        experience.add_points(4);
        experience.write_nbt(&mut nbt);
        assert_eq!(nbt.get_int("XpLevel"), Some(12));
        assert_eq!(nbt.get_int("XpTotal"), Some(227));
        assert_eq!(
            nbt.get_float("XpP").map(f32::to_bits),
            Some(CONTINUATION_227_BITS)
        );
        assert_eq!(nbt.get_int("XpSeed"), Some(-123_456_789));
        assert_eq!(experience.points(), 10);
        let packet = experience.packet();
        assert_eq!(packet.level.0, 12);
        assert_eq!(packet.total_experience.0, 227);
        assert_eq!(packet.progress.to_bits(), CONTINUATION_227_BITS);

        experience.set_level(3);
        experience.set_points(2);
        experience.write_nbt(&mut nbt);
        let loaded = Experience::read_nbt(&nbt);
        assert_eq!(loaded.level, 3);
        assert_eq!(loaded.progress.to_bits(), (2.0f32 / 13.0).to_bits());
        assert_eq!(loaded.total, 227);
    }

    #[test]
    fn nbt_fields_default_independently_without_reconstructing_totals() {
        let mut nbt = NbtCompound::new();
        nbt.put_int("XpTotal", 223);
        let loaded = Experience::read_nbt(&nbt);
        assert_eq!(loaded.level, 0);
        assert_eq!(loaded.progress.to_bits(), 0.0f32.to_bits());
        assert_eq!(loaded.total, 223);
        nbt.put_int("XpLevel", 12);
        nbt.put_float("XpP", -0.0);
        let loaded = Experience::read_nbt(&nbt);
        assert_eq!(loaded.level, 12);
        assert_eq!(loaded.progress.to_bits(), (-0.0f32).to_bits());
        assert_eq!(loaded.total, 223);
    }

    #[test]
    fn setters_preserve_total_and_levels_preserve_progress() {
        let mut experience = Experience {
            level: 12,
            progress: f32::from_bits(VANILLA_223_BITS),
            total: 223,
        };
        experience.set_level(30);
        assert_eq!(experience.progress.to_bits(), VANILLA_223_BITS);
        assert_eq!(experience.total, 223);
        experience.add_levels(-30);
        assert_eq!(experience.level, 0);
        assert_eq!(experience.progress.to_bits(), VANILLA_223_BITS);
        assert_eq!(experience.total, 223);
        experience.set_points(i32::MAX);
        assert_eq!(experience.progress.to_bits(), (6.0f32 / 7.0).to_bits());
        experience.set_points(-1);
        assert_eq!(experience.progress.to_bits(), 0.0f32.to_bits());
        assert_eq!(experience.total, 223);
        experience.add_levels(-1);
        assert_eq!(experience.level, 0);
        assert_eq!(experience.total, 0);
    }

    #[test]
    fn level_boundaries_and_negative_remainders_use_stepwise_floats() {
        for (level, needed) in [(0, 7), (14, 35), (15, 37), (29, 107), (30, 112)] {
            let mut experience = Experience {
                level,
                progress: 0.0,
                total: 2000,
            };
            assert_eq!(experience.points_needed(), needed);
            experience.add_points(needed);
            assert_eq!(experience.level, level + 1);
            assert_eq!(experience.progress.to_bits(), 0.0f32.to_bits());
            assert_eq!(experience.total, 2000 + needed);
            experience.add_points(-1);
            assert_eq!(experience.level, level);
            assert_eq!(
                experience.progress.to_bits(),
                (1.0f32 - 1.0 / needed as f32).to_bits()
            );
            assert_eq!(experience.total, 1999 + needed);
        }
        let mut experience = Experience::default();
        experience.add_points(-1);
        assert_eq!(experience.level, 0);
        assert_eq!(experience.total, 0);
        assert_eq!(experience.progress.to_bits(), 0.0f32.to_bits());
    }

    #[test]
    fn level_saturation_and_total_wrapping_follow_java_int_operations() {
        let mut experience = Experience {
            level: i32::MAX - 1,
            progress: 0.25,
            total: i32::MAX,
        };
        experience.add_levels(2);
        assert_eq!(experience.level, i32::MAX);
        assert_eq!(experience.progress.to_bits(), 0.25f32.to_bits());
        assert_eq!(experience.points_needed(), 2_147_483_481);
        experience.add_points(1);
        assert_eq!(experience.total, 0);
        assert_eq!(experience.level, i32::MAX);
        experience.add_levels(i32::MIN);
        assert_eq!(experience.level, 0);
        assert_eq!(experience.progress.to_bits(), 0.0f32.to_bits());
        assert_eq!(experience.total, 0);
    }

    #[test]
    fn maximum_point_award_and_minimum_removal_have_bounded_controls() {
        // Bytecode-derived control: 21,863 level crossings, no huge level scan.
        let mut experience = Experience::default();
        experience.add_points(i32::MAX);
        assert_eq!(experience.level, 21_863);
        assert_eq!(experience.total, i32::MAX);
        assert_eq!(experience.progress.to_bits(), 0x3ebc_bc42);
        assert_eq!(experience.points(), 72_474);

        let mut empty = Experience::default();
        empty.add_points(i32::MIN);
        assert_eq!(empty.level, 0);
        assert_eq!(empty.progress.to_bits(), 0.0f32.to_bits());
        assert_eq!(empty.total, 0);
    }

    #[test]
    fn zero_awards_do_not_normalize_saved_progress() {
        let mut experience = Experience {
            level: 12,
            progress: 1.0,
            total: -5,
        };
        experience.add_points(0);
        experience.add_levels(0);
        assert_eq!(experience.level, 12);
        assert_eq!(experience.progress.to_bits(), 1.0f32.to_bits());
        assert_eq!(experience.total, -5);
    }
}
