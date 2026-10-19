//! Shared LevelReader and LevelAccessor operations needed by vegetation.
use crate::{
    block::{BlockMethod, LevelAccessRef, LevelReaderRef},
    level::LevelError,
};
use ferraloom_data::{FluidState, tags::BlockTags};
use ferraloom_util::{
    BlockPos, Direction,
    random::Random,
    shapes::{self, BooleanOp},
};

impl LevelReaderRef<'_, '_> {
    /// Port of BlockAndLightGetter#getRawBrightness and LevelLightEngine#getRawBrightness.
    /// The caller supplies sky darkening; this overload never substitutes the clock.
    pub fn get_raw_brightness(&self, pos: BlockPos, sky_darken: i32) -> Result<i32, LevelError> {
        match self {
            Self::Server(level) => level.get_raw_brightness(pos, sky_darken),
            Self::PathNavigation(_) => Err(LevelError::UnsupportedPathNavigationQuery(
                crate::level::path_navigation_region::PathNavigationQuery::Light,
            )),
            Self::Worldgen(region) => {
                let sky = if self.has_skylight()? {
                    i32::from(
                        region
                            .brightness(
                                ferraloom_worldgen::region::LightLayer::Sky,
                                [pos.x, pos.y, pos.z],
                            )
                            .map_err(|_| LevelError::UnsupportedWorldgenEnvironment)?,
                    )
                    .wrapping_sub(sky_darken)
                } else {
                    0
                };
                let block = region
                    .brightness(
                        ferraloom_worldgen::region::LightLayer::Block,
                        [pos.x, pos.y, pos.z],
                    )
                    .map_err(|_| LevelError::UnsupportedWorldgenEnvironment)?;
                Ok(i32::from(block).max(sky))
            }
        }
    }
    /// Port of BlockAndLightGetter#canSeeSky, independent of day time.
    pub fn can_see_sky(&self, pos: BlockPos) -> Result<bool, LevelError> {
        match self {
            Self::Server(level) => {
                Ok(level.brightness(ferraloom_light::LightLayer::Sky, pos)? >= 15)
            }
            Self::PathNavigation(_) => Err(LevelError::UnsupportedPathNavigationQuery(
                crate::level::path_navigation_region::PathNavigationQuery::Light,
            )),
            Self::Worldgen(region) => region
                .brightness(
                    ferraloom_worldgen::region::LightLayer::Sky,
                    [pos.x, pos.y, pos.z],
                )
                .map(|light| light >= 15)
                .map_err(|_| LevelError::UnsupportedWorldgenEnvironment),
        }
    }
    /// Port of BlockGetter#getFluidState, using the canonical block's fluid state.
    pub fn get_fluid_state(&self, pos: BlockPos) -> Result<FluidState, LevelError> {
        Ok(self.get_block_state(pos)?.data().fluid)
    }
    /// Port of Block#canSupportCenter including the unstable bottom-center tag.
    pub fn can_support_center(&self, pos: BlockPos, face: Direction) -> Result<bool, LevelError> {
        let state = self.get_block_state(pos)?;
        if face == Direction::Down
            && self
                .block_tags()?
                .contains_key(BlockTags::UNSTABLE_BOTTOM_CENTER, state.block())
        {
            return Ok(false);
        }
        if let Some(sturdy) = state.cached_center_support(face) {
            return Ok(sturdy);
        }
        let support = self
            .required_behavior(state.block(), BlockMethod::IsFaceSturdy)?
            .support_shape(state, pos, self)?;
        let center = shapes::create(
            7.0 / 16.0,
            0.0,
            7.0 / 16.0,
            9.0 / 16.0,
            10.0 / 16.0,
            9.0 / 16.0,
        );
        Ok(!shapes::join_is_not_empty(
            &support.get_face_shape(face),
            &center,
            BooleanOp::OnlySecond,
        ))
    }
}
impl LevelAccessRef<'_, '_> {
    /// Port of LevelAccessor#getRandom. Region and feature streams stay separate.
    pub fn random(&mut self) -> Result<&mut Random, LevelError> {
        match self {
            Self::Server(level) => Ok(level.random()),
            Self::Worldgen(region) => region
                .random_mut()
                .map_err(|_| LevelError::UnsupportedWorldgenEnvironment),
        }
    }
}
