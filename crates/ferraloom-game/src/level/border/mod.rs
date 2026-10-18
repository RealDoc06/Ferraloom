//! Port of 26.3 WorldBorder. Lerp time is game ticks, including saved-data time.
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    codec::{VarInt, VarLong},
    play::clientbound as packet,
};
use ferraloom_storage::{Error, Result, SavedDataCodec};
use ferraloom_util::{
    Aabb, BlockPos, ChunkPos, Vec3, mth,
    shapes::{self, BooleanOp, VoxelShape},
};

pub const MAX_SIZE: f64 = 59_999_968.0;
pub const MAX_CENTER_COORDINATE: f64 = 29_999_984.0;
pub const SAVED_DATA_ID: &str = "minecraft:world_border";

/// Port of WorldBorder.Settings. Defaults retain vanilla's 300-tick warning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub center_x: f64,
    pub center_z: f64,
    pub damage_per_block: f64,
    pub safe_zone: f64,
    pub warning_blocks: i32,
    pub warning_time: i32,
    pub size: f64,
    pub lerp_time: i64,
    pub lerp_target: f64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            center_x: 0.0,
            center_z: 0.0,
            damage_per_block: 0.2,
            safe_zone: 5.0,
            warning_blocks: 5,
            warning_time: 300,
            size: MAX_SIZE,
            lerp_time: 0,
            lerp_target: 0.0,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BorderStatus {
    Stationary,
    Growing,
    Shrinking,
}
#[derive(Clone, Debug)]
enum Extent {
    Static {
        size: f64,
    },
    Moving {
        from: f64,
        to: f64,
        begin: i64,
        end: i64,
        duration: f64,
        progress: i64,
        size: f64,
        previous_size: f64,
    },
}
/// Listener events are returned in call order. The owning level sends packets
/// and invokes dimension listeners at the originating call site.
#[derive(Clone, Debug, PartialEq)]
pub enum BorderChange {
    Center {
        x: f64,
        z: f64,
    },
    Size(f64),
    Lerp {
        from: f64,
        to: f64,
        ticks: i64,
        game_time: i64,
    },
    SafeZone(f64),
    DamagePerBlock(f64),
    WarningTime(i32),
    WarningBlocks(i32),
}
impl BorderChange {
    pub fn packet(&self) -> Option<packet::Packet> {
        Some(match *self {
            Self::Center { x, z } => packet::Packet::SetBorderCenter(packet::SetBorderCenter {
                new_center_x: x,
                new_center_z: z,
            }),
            Self::Size(size) => packet::Packet::SetBorderSize(packet::SetBorderSize { size }),
            Self::Lerp {
                from, to, ticks, ..
            } => packet::Packet::SetBorderLerpSize(packet::SetBorderLerpSize {
                old_size: from,
                new_size: to,
                lerp_time: VarLong(ticks),
            }),
            Self::WarningTime(time) => {
                packet::Packet::SetBorderWarningDelay(packet::SetBorderWarningDelay {
                    warning_delay: VarInt(time),
                })
            }
            Self::WarningBlocks(blocks) => {
                packet::Packet::SetBorderWarningDistance(packet::SetBorderWarningDistance {
                    warning_blocks: VarInt(blocks),
                })
            }
            Self::SafeZone(_) | Self::DamagePerBlock(_) => return None,
        })
    }
}
#[derive(Clone, Debug)]
pub struct WorldBorder {
    initial: Settings,
    initialized: bool,
    center_x: f64,
    center_z: f64,
    damage_per_block: f64,
    safe_zone: f64,
    warning_time: i32,
    warning_blocks: i32,
    absolute_max_size: i32,
    extent: Extent,
    dirty: bool,
    extra: CompoundTag,
}
impl Default for WorldBorder {
    fn default() -> Self {
        Self::new(Settings::default())
    }
}
impl WorldBorder {
    pub fn new(settings: Settings) -> Self {
        Self {
            initial: settings,
            initialized: false,
            center_x: 0.0,
            center_z: 0.0,
            damage_per_block: 0.2,
            safe_zone: 5.0,
            warning_time: 15,
            warning_blocks: 5,
            absolute_max_size: 29_999_984,
            extent: Extent::Static { size: MAX_SIZE },
            dirty: false,
            extra: CompoundTag::new(),
        }
    }
    pub fn center_x(&self) -> f64 {
        self.center_x
    }
    pub fn center_z(&self) -> f64 {
        self.center_z
    }
    pub fn size(&self) -> f64 {
        match self.extent {
            Extent::Static { size } | Extent::Moving { size, .. } => size,
        }
    }
    pub fn lerp_time(&self) -> i64 {
        match self.extent {
            Extent::Static { .. } => 0,
            Extent::Moving { progress, .. } => progress,
        }
    }
    pub fn lerp_target(&self) -> f64 {
        match self.extent {
            Extent::Static { size } => size,
            Extent::Moving { to, .. } => to,
        }
    }
    pub fn lerp_speed(&self) -> f64 {
        match self.extent {
            Extent::Static { .. } => 0.0,
            Extent::Moving {
                from,
                to,
                begin,
                end,
                ..
            } => (from - to).abs() / end.wrapping_sub(begin) as f64,
        }
    }
    pub fn status(&self) -> BorderStatus {
        match self.extent {
            Extent::Static { .. } => BorderStatus::Stationary,
            Extent::Moving { from, to, .. } => {
                if to < from {
                    BorderStatus::Shrinking
                } else {
                    BorderStatus::Growing
                }
            }
        }
    }
    pub fn absolute_max_size(&self) -> i32 {
        self.absolute_max_size
    }
    pub fn safe_zone(&self) -> f64 {
        self.safe_zone
    }
    pub fn damage_per_block(&self) -> f64 {
        self.damage_per_block
    }
    pub fn warning_time(&self) -> i32 {
        self.warning_time
    }
    pub fn warning_blocks(&self) -> i32 {
        self.warning_blocks
    }
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }
    fn bounds_size(&self, partial: f32) -> f64 {
        match self.extent {
            Extent::Static { size } => size,
            Extent::Moving {
                size,
                previous_size,
                ..
            } => mth::lerp(f64::from(partial), previous_size, size),
        }
    }
    pub fn min_x(&self) -> f64 {
        self.min_x_at(0.0)
    }
    pub fn max_x(&self) -> f64 {
        self.max_x_at(0.0)
    }
    pub fn min_z(&self) -> f64 {
        self.min_z_at(0.0)
    }
    pub fn max_z(&self) -> f64 {
        self.max_z_at(0.0)
    }
    pub fn min_x_at(&self, partial: f32) -> f64 {
        (self.center_x - self.bounds_size(partial) / 2.0).clamp(
            -f64::from(self.absolute_max_size),
            f64::from(self.absolute_max_size),
        )
    }
    pub fn max_x_at(&self, partial: f32) -> f64 {
        (self.center_x + self.bounds_size(partial) / 2.0).clamp(
            -f64::from(self.absolute_max_size),
            f64::from(self.absolute_max_size),
        )
    }
    pub fn min_z_at(&self, partial: f32) -> f64 {
        (self.center_z - self.bounds_size(partial) / 2.0).clamp(
            -f64::from(self.absolute_max_size),
            f64::from(self.absolute_max_size),
        )
    }
    pub fn max_z_at(&self, partial: f32) -> f64 {
        (self.center_z + self.bounds_size(partial) / 2.0).clamp(
            -f64::from(self.absolute_max_size),
            f64::from(self.absolute_max_size),
        )
    }
    pub fn is_within_bounds(&self, x: f64, z: f64) -> bool {
        self.is_within_bounds_with_margin(x, z, 0.0)
    }
    pub fn is_within_bounds_with_margin(&self, x: f64, z: f64, margin: f64) -> bool {
        x >= self.min_x() - margin
            && x < self.max_x() + margin
            && z >= self.min_z() - margin
            && z < self.max_z() + margin
    }
    pub fn is_within_block(&self, pos: BlockPos) -> bool {
        self.is_within_bounds(f64::from(pos.x), f64::from(pos.z))
    }
    pub fn is_within_chunk(&self, pos: ChunkPos) -> bool {
        self.is_within_bounds(
            f64::from(pos.x.wrapping_mul(16)),
            f64::from(pos.z.wrapping_mul(16)),
        ) && self.is_within_bounds(
            f64::from(pos.x.wrapping_mul(16).wrapping_add(15)),
            f64::from(pos.z.wrapping_mul(16).wrapping_add(15)),
        )
    }
    pub fn is_within_aabb(&self, bb: Aabb) -> bool {
        self.is_within_bounds(bb.min_x, bb.min_z)
            && self.is_within_bounds(
                bb.max_x - f64::from(1e-5_f32),
                bb.max_z - f64::from(1e-5_f32),
            )
    }
    pub fn clamp_vec3(&self, pos: Vec3) -> Vec3 {
        Vec3::new(
            pos.x
                .clamp(self.min_x(), self.max_x() - f64::from(1e-5_f32)),
            pos.y,
            pos.z
                .clamp(self.min_z(), self.max_z() - f64::from(1e-5_f32)),
        )
    }
    pub fn clamp_to_bounds(&self, pos: Vec3) -> BlockPos {
        let pos = self.clamp_vec3(pos);
        BlockPos::containing(pos.x, pos.y, pos.z)
    }
    pub fn distance_to_border(&self, x: f64, z: f64) -> f64 {
        (x - self.min_x())
            .min(self.max_x() - x)
            .min(z - self.min_z())
            .min(self.max_z() - z)
    }
    pub fn is_inside_close_to_border(&self, pos: Vec3, bb: Aabb) -> bool {
        let size = bb.x_size().abs().max(bb.z_size().abs()).max(1.0);
        self.distance_to_border(pos.x, pos.z) < size * 2.0
            && self.is_within_bounds_with_margin(pos.x, pos.z, size)
    }
    pub fn collision_shape(&self) -> VoxelShape {
        shapes::join(
            &shapes::infinity(),
            &shapes::box_shape(
                self.min_x().floor(),
                f64::NEG_INFINITY,
                self.min_z().floor(),
                self.max_x().ceil(),
                f64::INFINITY,
                self.max_z().ceil(),
            ),
            BooleanOp::OnlyFirst,
        )
    }
    pub fn set_center(&mut self, x: f64, z: f64) -> BorderChange {
        self.center_x = x;
        self.center_z = z;
        self.dirty = true;
        BorderChange::Center { x, z }
    }
    pub fn set_size(&mut self, size: f64) -> BorderChange {
        self.extent = Extent::Static { size };
        self.dirty = true;
        BorderChange::Size(size)
    }
    pub fn lerp_size_between(
        &mut self,
        from: f64,
        to: f64,
        ticks: i64,
        game_time: i64,
    ) -> BorderChange {
        self.extent = if from == to {
            Extent::Static { size: to }
        } else {
            let duration = ticks as f64;
            let remaining = ticks;
            let progress = (duration - remaining as f64) / duration;
            let size = if progress < 1.0 {
                mth::lerp(progress, from, to)
            } else {
                to
            };
            Extent::Moving {
                from,
                to,
                begin: game_time,
                end: game_time.wrapping_add(ticks),
                duration: ticks as f64,
                progress: ticks,
                size,
                previous_size: size,
            }
        };
        self.dirty = true;
        BorderChange::Lerp {
            from,
            to,
            ticks,
            game_time,
        }
    }
    pub fn set_absolute_max_size(&mut self, size: i32) {
        self.absolute_max_size = size;
    }
    pub fn set_safe_zone(&mut self, value: f64) -> BorderChange {
        self.safe_zone = value;
        self.dirty = true;
        BorderChange::SafeZone(value)
    }
    pub fn set_damage_per_block(&mut self, value: f64) -> BorderChange {
        self.damage_per_block = value;
        self.dirty = true;
        BorderChange::DamagePerBlock(value)
    }
    pub fn set_warning_time(&mut self, value: i32) -> BorderChange {
        self.warning_time = value;
        self.dirty = true;
        BorderChange::WarningTime(value)
    }
    pub fn set_warning_blocks(&mut self, value: i32) -> BorderChange {
        self.warning_blocks = value;
        self.dirty = true;
        BorderChange::WarningBlocks(value)
    }
    pub fn tick(&mut self) {
        if let Extent::Moving {
            from,
            to,
            duration,
            progress,
            size,
            previous_size,
            ..
        } = &mut self.extent
        {
            *progress = progress.wrapping_sub(1);
            *previous_size = *size;
            let fraction = (*duration - *progress as f64) / *duration;
            *size = if fraction < 1.0 {
                mth::lerp(fraction, *from, *to)
            } else {
                *to
            };
            self.dirty = true;
            if *progress <= 0 {
                self.extent = Extent::Static { size: *to };
            }
        }
    }
    pub fn apply_initial_settings(&mut self, game_time: i64) -> Vec<BorderChange> {
        if self.initialized {
            return Vec::new();
        }
        let s = self.initial;
        let changes = vec![
            self.set_center(s.center_x, s.center_z),
            self.set_damage_per_block(s.damage_per_block),
            self.set_safe_zone(s.safe_zone),
            self.set_warning_blocks(s.warning_blocks),
            self.set_warning_time(s.warning_time),
            if s.lerp_time > 0 {
                self.lerp_size_between(s.size, s.lerp_target, s.lerp_time, game_time)
            } else {
                self.set_size(s.size)
            },
        ];
        self.initialized = true;
        changes
    }
    pub fn settings(&self) -> Settings {
        Settings {
            center_x: self.center_x,
            center_z: self.center_z,
            damage_per_block: self.damage_per_block,
            safe_zone: self.safe_zone,
            warning_blocks: self.warning_blocks,
            warning_time: self.warning_time,
            size: self.size(),
            lerp_time: self.lerp_time(),
            lerp_target: self.lerp_target(),
        }
    }
    pub fn initialization_packet(&self) -> packet::InitializeBorder {
        packet::InitializeBorder {
            new_center_x: self.center_x,
            new_center_z: self.center_z,
            old_size: self.size(),
            new_size: self.lerp_target(),
            lerp_time: VarLong(self.lerp_time()),
            new_absolute_max_size: VarInt(self.absolute_max_size),
            warning_blocks: VarInt(self.warning_blocks),
            warning_time: VarInt(self.warning_time),
        }
    }
}
impl SavedDataCodec for WorldBorder {
    fn decode(tag: &Tag) -> Result<Self> {
        let tag = tag
            .as_compound()
            .ok_or_else(|| Error::Invalid("world_border must be a compound".into()))?;
        let double = |key| {
            tag.get_double(key)
                .ok_or_else(|| Error::Invalid(format!("missing world_border {key}")))
        };
        let int = |key| {
            tag.get_int(key)
                .ok_or_else(|| Error::Invalid(format!("missing world_border {key}")))
        };
        let center_x = double("center_x")?;
        let center_z = double("center_z")?;
        if !(-MAX_CENTER_COORDINATE..=MAX_CENTER_COORDINATE).contains(&center_x)
            || !(-MAX_CENTER_COORDINATE..=MAX_CENTER_COORDINATE).contains(&center_z)
        {
            return Err(Error::Invalid("world border center out of range".into()));
        }
        let mut border = Self::new(Settings {
            center_x,
            center_z,
            damage_per_block: double("damage_per_block")?,
            safe_zone: double("safe_zone")?,
            warning_blocks: int("warning_blocks")?,
            warning_time: int("warning_time")?,
            size: double("size")?,
            lerp_time: tag
                .get_long("lerp_time")
                .ok_or_else(|| Error::Invalid("missing world_border lerp_time".into()))?,
            lerp_target: double("lerp_target")?,
        });
        border.extra = tag.clone();
        Ok(border)
    }
    fn encode(&self) -> Result<Tag> {
        let s = self.settings();
        let mut tag = self.extra.clone();
        for (key, value) in [
            ("center_x", s.center_x),
            ("center_z", s.center_z),
            ("damage_per_block", s.damage_per_block),
            ("safe_zone", s.safe_zone),
            ("size", s.size),
            ("lerp_target", s.lerp_target),
        ] {
            tag.insert(key, Tag::Double(value));
        }
        tag.insert("warning_blocks", Tag::Int(s.warning_blocks));
        tag.insert("warning_time", Tag::Int(s.warning_time));
        tag.insert("lerp_time", Tag::Long(s.lerp_time));
        Ok(Tag::Compound(tag))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_tick_lerp_preserves_previous_tick_bounds_and_save_resume() {
        let mut border = WorldBorder::default();
        border.apply_initial_settings(100);
        assert_eq!(border.warning_time(), 300);
        border.lerp_size_between(100.0, 200.0, 4, 100);
        border.tick();
        assert_eq!(border.size().to_bits(), 125.0_f64.to_bits());
        assert_eq!(border.max_x().to_bits(), 50.0_f64.to_bits());
        assert_eq!(border.max_x_at(1.0).to_bits(), 62.5_f64.to_bits());
        let mut resumed = WorldBorder::decode(&border.encode().unwrap()).unwrap();
        resumed.apply_initial_settings(101);
        for _ in 0..3 {
            border.tick();
            resumed.tick();
            assert_eq!(border.size().to_bits(), resumed.size().to_bits());
        }
        assert_eq!(border.status(), BorderStatus::Stationary);
        assert_eq!(border.max_x(), 100.0);
    }
    #[test]
    fn border_bounds_are_half_open_and_collision_uses_integer_rounding() {
        let mut border = WorldBorder::default();
        border.set_center(0.25, -0.25);
        border.set_size(10.0);
        assert!(border.is_within_bounds(-4.75, -5.25));
        assert!(!border.is_within_bounds(5.25, 0.0));
        let shape = border.collision_shape();
        let bb = Aabb::new(4.0, 0.0, 0.0, 4.5, 1.0, 0.5);
        assert_eq!(shape.collide(ferraloom_util::Axis::X, bb, 4.0), 1.5);
        assert!(border.is_inside_close_to_border(Vec3::new(4.5, 0.0, 0.0), bb));
        assert!(!border.is_inside_close_to_border(Vec3::new(7.0, 0.0, 0.0), bb));
    }
}
