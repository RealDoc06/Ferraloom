//! Joint world/RNG access keeps canonical Level.random borrows short.
use super::{FeatureRandom, FeatureRandomSource, LiveWorldGenLevel};
use crate::Error;
use ferraloom_util::random::WorldgenRandom;

/// The caller can supply an independent stream, or use Level#getRandom itself.
/// Canonical is resolved for each draw, so world callbacks can consume that same
/// stream between feature draws without aliasing mutable references.
pub enum FeatureRandomSelector<'a> {
    External(FeatureRandom<'a>),
    Canonical,
}
impl FeatureRandomSelector<'_> {
    pub fn reborrow(&mut self) -> FeatureRandomSelector<'_> {
        match self {
            Self::External(random) => FeatureRandomSelector::External(random.feature_random()),
            Self::Canonical => FeatureRandomSelector::Canonical,
        }
    }
    pub(crate) fn resolve<'a, L: LiveWorldGenLevel>(
        &'a mut self,
        level: &'a mut L,
    ) -> Result<FeatureRandom<'a>, Error> {
        match self {
            Self::External(random) => Ok(random.feature_random()),
            Self::Canonical => Ok(FeatureRandom::Live(level.random_mut()?)),
        }
    }
    pub(crate) fn region_mut(&mut self) -> Result<&mut WorldgenRandom, Error> {
        match self {
            Self::External(random) => random.region_mut(),
            Self::Canonical => Err(Error(
                "canonical live random used with a generation region".into(),
            )),
        }
    }
}

enum LevelAccess<'a, L> {
    Read(&'a L),
    Write(&'a mut L),
}
/// A helper may interleave world operations and draws, but never retain a
/// canonical RNG borrow across a world callback. Read-only processors retain
/// their original region access and independent RNG through `read`.
pub struct FeaturePlacementAccess<'a, L> {
    level: LevelAccess<'a, L>,
    random: FeatureRandomSelector<'a>,
}
impl<'a, L: LiveWorldGenLevel> FeaturePlacementAccess<'a, L> {
    pub fn new(level: &'a mut L, random: FeatureRandomSelector<'a>) -> Self {
        Self {
            level: LevelAccess::Write(level),
            random,
        }
    }
    pub fn read(level: &'a L, random: FeatureRandom<'a>) -> Self {
        Self {
            level: LevelAccess::Read(level),
            random: FeatureRandomSelector::External(random),
        }
    }
    pub fn level(&self) -> &L {
        match &self.level {
            LevelAccess::Read(level) => level,
            LevelAccess::Write(level) => level,
        }
    }
    pub fn level_mut(&mut self) -> Result<&mut L, Error> {
        match &mut self.level {
            LevelAccess::Write(level) => Ok(level),
            LevelAccess::Read(_) => Err(Error(
                "mutable feature access through a read-only processor".into(),
            )),
        }
    }
    pub fn random(&mut self) -> Result<FeatureRandom<'_>, Error> {
        match &mut self.level {
            LevelAccess::Write(level) => self.random.resolve(&mut **level),
            LevelAccess::Read(_) => match &mut self.random {
                FeatureRandomSelector::External(random) => Ok(random.feature_random()),
                FeatureRandomSelector::Canonical => Err(Error(
                    "canonical random requires mutable live access".into(),
                )),
            },
        }
    }
    /// Random-aware world operations receive the selector, allowing a live
    /// implementation to draw its own canonical stream inside that operation.
    pub fn parts_mut(&mut self) -> Result<(&mut L, &mut FeatureRandomSelector<'a>), Error> {
        match &mut self.level {
            LevelAccess::Write(level) => Ok((level, &mut self.random)),
            LevelAccess::Read(_) => Err(Error(
                "mutable feature access through a read-only processor".into(),
            )),
        }
    }
}
