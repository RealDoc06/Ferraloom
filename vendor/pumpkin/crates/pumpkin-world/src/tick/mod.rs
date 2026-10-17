use std::str::FromStr;

use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::{
    math::position::BlockPos,
    resource_location::{FromResourceLocation, ResourceLocation, ToResourceLocation},
};

pub mod scheduler;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
#[repr(i32)]
pub enum TickPriority {
    ExtremelyHigh = -3,
    VeryHigh = -2,
    High = -1,
    Normal = 0,
    Low = 1,
    VeryLow = 2,
    ExtremelyLow = 3,
}

impl TickPriority {
    #[must_use]
    pub const fn values() -> [Self; 7] {
        [
            Self::ExtremelyHigh,
            Self::VeryHigh,
            Self::High,
            Self::Normal,
            Self::Low,
            Self::VeryLow,
            Self::ExtremelyLow,
        ]
    }
}

#[derive(Debug)]
pub struct TickPriorityNotFound;

impl TryFrom<i32> for TickPriority {
    type Error = TickPriorityNotFound;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            -3 => Ok(Self::ExtremelyHigh),
            -2 => Ok(Self::VeryHigh),
            -1 => Ok(Self::High),
            0 => Ok(Self::Normal),
            1 => Ok(Self::Low),
            2 => Ok(Self::VeryLow),
            3 => Ok(Self::ExtremelyLow),
            _ => Err(TickPriorityNotFound),
        }
    }
}

/// Runtime tick. Identity is (position, type), independently of either ordering.
#[derive(Clone, Copy)]
pub struct ScheduledTick<T> {
    pub position: BlockPos,
    pub value: T,
    pub trigger_tick: i64,
    pub priority: TickPriority,
    pub sub_tick_order: i64,
}

impl<T> ScheduledTick<T> {
    pub(crate) fn drain_key(&self) -> (i64, TickPriority, i64) {
        (self.trigger_tick, self.priority, self.sub_tick_order)
    }

    pub(crate) fn intra_tick_key(&self) -> (TickPriority, i64) {
        (self.priority, self.sub_tick_order)
    }
}

impl<T: Copy> ScheduledTick<T> {
    pub fn to_saved_tick(&self, now: i64) -> SavedTick<T> {
        SavedTick {
            position: self.position,
            value: self.value,
            delay: self.trigger_tick.wrapping_sub(now) as i32,
            priority: self.priority,
        }
    }
}

/// Disk/proto record. It stays relative until the chunk starts block ticking.
#[derive(Clone, Copy)]
pub struct SavedTick<T> {
    pub position: BlockPos,
    pub value: T,
    pub delay: i32,
    pub priority: TickPriority,
}

impl<T> SavedTick<T> {
    pub fn unpack(self, now: i64, sub_tick_order: i64) -> ScheduledTick<T> {
        ScheduledTick {
            position: self.position,
            value: self.value,
            trigger_tick: now.wrapping_add(i64::from(self.delay)),
            priority: self.priority,
            sub_tick_order,
        }
    }
}

impl<T> SavedTick<T>
where
    T: ToResourceLocation,
{
    #[must_use]
    pub fn to_nbt_compound(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_int("x", self.position.0.x);
        nbt.put_int("y", self.position.0.y);
        nbt.put_int("z", self.position.0.z);
        nbt.put_int("t", self.delay);
        nbt.put_int("p", self.priority as i32);
        nbt.put_string("i", self.value.to_resource_location());
        nbt
    }
}

impl<T> SavedTick<T>
where
    T: FromResourceLocation,
{
    #[must_use]
    pub fn from_nbt_compound(nbt: &NbtCompound) -> Option<Self> {
        let x = nbt.get_int("x")?;
        let y = nbt.get_int("y")?;
        let z = nbt.get_int("z")?;
        let delay = nbt.get_int("t")?;
        let priority = TickPriority::try_from(nbt.get_int("p")?).ok()?;
        let res_loc_str = nbt.get_string("i")?;
        let res_loc = ResourceLocation::from_str(res_loc_str).ok()?;
        let value = T::from_resource_location(&res_loc)?;

        Some(Self {
            delay,
            priority,
            position: BlockPos::new(x, y, z),
            value,
        })
    }
}
