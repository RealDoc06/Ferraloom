//! Scoped tick-state access for live level adapters and serial compatibility.
use super::{LevelAccess, Ticks};
use horizon_core::position::{BlockPos, ChunkPos};
use std::ops::{Deref, DerefMut};

/// Resolve the necessary ownership before borrowing tick data. A chunk scope
/// must reach the loaded owner; a miss never means an empty schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickScope {
    Current,
    Chunk(ChunkPos),
    Dimension,
}

impl TickScope {
    pub fn at(pos: BlockPos) -> Self {
        Self::Chunk(ChunkPos::new(pos.x >> 4, pos.z >> 4))
    }
}

/// A data operation ends before the caller invokes world code or parks.
/// The result cannot borrow from the temporary `Ticks` reference. Runtime
/// implementations keep their owner and borrow helpers private; operations
/// passed here only read or mutate tick data, never call back into the world.
pub trait TickAccess {
    fn read_ticks<R>(&self, scope: TickScope, read: impl FnOnce(&Ticks) -> R) -> R;
    fn write_ticks<R>(&mut self, scope: TickScope, write: impl FnOnce(&mut Ticks) -> R) -> R;
}
impl TickAccess for Ticks {
    fn read_ticks<R>(&self, _scope: TickScope, read: impl FnOnce(&Ticks) -> R) -> R {
        read(self)
    }
    fn write_ticks<R>(&mut self, _scope: TickScope, write: impl FnOnce(&mut Ticks) -> R) -> R {
        write(self)
    }
}
impl<T: TickAccess + ?Sized> TickAccess for &mut T {
    fn read_ticks<R>(&self, scope: TickScope, read: impl FnOnce(&Ticks) -> R) -> R {
        (**self).read_ticks(scope, read)
    }
    fn write_ticks<R>(&mut self, scope: TickScope, write: impl FnOnce(&mut Ticks) -> R) -> R {
        (**self).write_ticks(scope, write)
    }
}

/// Keep world calls and tick operations behind one context borrow. In
/// particular, a borrow returned by `LevelAccess::random` cannot coexist with
/// another mutable operation on that same context.
/// Reborrowing retains the context's type shape across nested behaviour calls.
pub trait TickContext: TickAccess + DerefMut<Target = Self::Level> {
    type Level: LevelAccess + ?Sized;
    type Borrowed<'a>: TickContext<Level = Self::Level>
    where
        Self: 'a;

    fn reborrow(&mut self) -> Self::Borrowed<'_>;
}
impl<L: LevelAccess + TickAccess + ?Sized> TickContext for &mut L {
    type Level = L;
    type Borrowed<'a>
        = &'a mut L
    where
        Self: 'a;

    fn reborrow(&mut self) -> Self::Borrowed<'_> {
        &mut **self
    }
}

/// Compatibility for callers that still own the level and `Ticks` separately.
/// Live adapters implement `TickAccess` on their own owner instead; this pair
/// does not provide an ownership-handoff mechanism.
pub struct BorrowedLevelTicks<'a, L: LevelAccess + ?Sized> {
    level: &'a mut L,
    ticks: &'a mut Ticks,
}
impl<'a, L: LevelAccess + ?Sized> BorrowedLevelTicks<'a, L> {
    pub fn new(level: &'a mut L, ticks: &'a mut Ticks) -> Self {
        Self { level, ticks }
    }
}
impl<L: LevelAccess + ?Sized> Deref for BorrowedLevelTicks<'_, L> {
    type Target = L;

    fn deref(&self) -> &L {
        self.level
    }
}
impl<L: LevelAccess + ?Sized> DerefMut for BorrowedLevelTicks<'_, L> {
    fn deref_mut(&mut self) -> &mut L {
        self.level
    }
}
impl<L: LevelAccess + ?Sized> TickAccess for BorrowedLevelTicks<'_, L> {
    fn read_ticks<R>(&self, scope: TickScope, read: impl FnOnce(&Ticks) -> R) -> R {
        self.ticks.read_ticks(scope, read)
    }
    fn write_ticks<R>(&mut self, scope: TickScope, write: impl FnOnce(&mut Ticks) -> R) -> R {
        self.ticks.write_ticks(scope, write)
    }
}
impl<L: LevelAccess + ?Sized> TickContext for BorrowedLevelTicks<'_, L> {
    type Level = L;
    type Borrowed<'a>
        = BorrowedLevelTicks<'a, L>
    where
        Self: 'a;

    fn reborrow(&mut self) -> Self::Borrowed<'_> {
        BorrowedLevelTicks::new(self.level, self.ticks)
    }
}
