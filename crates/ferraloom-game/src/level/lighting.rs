//! Concrete live light-engine queries and shared dimension context.
use super::{Level, LevelError};
use crate::block::LevelReaderRef;
use ferraloom_light::LightLayer;
use ferraloom_util::BlockPos;

impl Level {
    pub fn set_has_skylight(&mut self, has_skylight: bool) {
        self.region_mut().has_skylight = Some(has_skylight);
    }
    /// Port of DimensionType#hasSkyLight, supplied with the dimension context.
    pub fn has_skylight(&self) -> Result<bool, LevelError> {
        self.region()
            .has_skylight
            .ok_or(LevelError::UnsupportedLightQuery)
    }
    /// Port of LevelLightEngine#getRawBrightness. An absent sky engine bypasses
    /// subtraction entirely; present engines use Java's wrapping integer math.
    pub fn get_raw_brightness(&self, pos: BlockPos, sky_darken: i32) -> Result<i32, LevelError> {
        let sky = if self.has_skylight()? {
            i32::from(self.brightness(LightLayer::Sky, pos)?).wrapping_sub(sky_darken)
        } else {
            0
        };
        Ok(i32::from(self.brightness(LightLayer::Block, pos)?).max(sky))
    }
    pub fn max_local_raw_brightness(&self, pos: BlockPos) -> Result<i32, LevelError> {
        self.max_local_raw_brightness_with_sky(pos, self.sky_darken()?)
    }
    /// Port of LevelReader#getMaxLocalRawBrightness(BlockPos, int).
    pub fn max_local_raw_brightness_with_sky(
        &self,
        pos: BlockPos,
        sky_darken: i32,
    ) -> Result<i32, LevelError> {
        LevelReaderRef::Server(self).max_local_raw_brightness_with_sky(pos, sky_darken)
    }
    pub fn get_effective_sky_brightness(&self, pos: BlockPos) -> Result<i32, LevelError> {
        LevelReaderRef::Server(self).get_effective_sky_brightness(pos)
    }
}
impl LevelReaderRef<'_, '_> {
    pub fn has_skylight(&self) -> Result<bool, LevelError> {
        match self {
            Self::Server(level) => level.has_skylight(),
            Self::PathNavigation(_) => Err(LevelError::UnsupportedPathNavigationQuery(
                super::path_navigation_region::PathNavigationQuery::Light,
            )),
            Self::Worldgen(region) => region
                .has_skylight()
                .map_err(|_| LevelError::UnsupportedWorldgenEnvironment),
        }
    }
    pub fn max_local_raw_brightness(&self, pos: BlockPos) -> Result<i32, LevelError> {
        self.max_local_raw_brightness_with_sky(pos, self.sky_darken()?)
    }
    pub fn max_local_raw_brightness_with_sky(
        &self,
        pos: BlockPos,
        sky_darken: i32,
    ) -> Result<i32, LevelError> {
        if pos.x < -30_000_000 || pos.z < -30_000_000 || pos.x >= 30_000_000 || pos.z >= 30_000_000
        {
            return Ok(15);
        }
        self.get_raw_brightness(pos, sky_darken)
    }
    /// Effective sky brightness subtracts even for an absent engine, unlike the
    /// raw-light query. The shared skyDarken remains Level-owned.
    pub fn get_effective_sky_brightness(&self, pos: BlockPos) -> Result<i32, LevelError> {
        let sky = if self.has_skylight()? {
            match self {
                Self::Server(level) => i32::from(level.brightness(LightLayer::Sky, pos)?),
                Self::PathNavigation(_) => {
                    return Err(LevelError::UnsupportedPathNavigationQuery(
                        super::path_navigation_region::PathNavigationQuery::Light,
                    ));
                }
                Self::Worldgen(region) => i32::from(
                    region
                        .brightness(
                            ferraloom_worldgen::region::LightLayer::Sky,
                            [pos.x, pos.y, pos.z],
                        )
                        .map_err(|_| LevelError::UnsupportedWorldgenEnvironment)?,
                ),
            }
        } else {
            0
        };
        Ok(sky.wrapping_sub(self.sky_darken()?).max(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        level::{Region, chunk::LevelChunk, weather::ChunkLightSnapshot},
    };
    use ferraloom_util::ChunkPos;
    use std::{collections::BTreeMap, sync::Arc};
    static EMPTY: Behaviors = Behaviors::EMPTY;

    #[test]
    fn raw_brightness_matches_native_absent_sky_and_integer_overflow() {
        let mut level = Level::new(
            Region::new(&EMPTY),
            crate::level::LevelContext::fixture(0, 63),
        );
        assert_eq!(level.has_skylight(), Err(LevelError::UnsupportedLightQuery));
        let pos = BlockPos::new(8, 8, 8);
        let chunk = ChunkPos::containing(pos);
        level
            .add_chunk(LevelChunk::new(
                chunk,
                0..1,
                ferraloom_chunk::LevelChunkSection::new(
                    ferraloom_chunk::Strategy::block_states(),
                    ferraloom_chunk::Strategy::biomes(Arc::new(
                        ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                    )),
                    ferraloom_chunk::BiomeId(0),
                ),
            ))
            .unwrap();
        let mut count = 0;
        for line in include_str!("../../tests/java/level/raw-brightness.tsv").lines() {
            let fields: Vec<_> = line.split('\t').collect();
            let has_sky: bool = fields[0].parse().unwrap();
            let sky: u8 = fields[1].parse().unwrap();
            let block: u8 = fields[2].parse().unwrap();
            let darken: i32 = fields[3].parse().unwrap();
            let expected: i32 = fields[4].parse().unwrap();
            level.set_has_skylight(has_sky);
            level.set_light_snapshot(
                chunk,
                ChunkLightSnapshot {
                    sky: BTreeMap::from([(0, ferraloom_light::DataLayer::new(sky))]),
                    block: BTreeMap::from([(0, ferraloom_light::DataLayer::new(block))]),
                },
            );
            assert_eq!(
                level.get_raw_brightness(pos, darken),
                Ok(expected),
                "{line}"
            );
            assert_eq!(
                LevelReaderRef::Server(&level).get_raw_brightness(pos, darken),
                Ok(expected),
                "{line}"
            );
            count += 1;
        }
        assert_eq!(count, 22);
    }
}
