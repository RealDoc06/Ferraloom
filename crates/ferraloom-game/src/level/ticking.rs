//! ServerLevel's scheduled and random tick primitives. The phase driver supplies
//! dimension-wide budgets and chunk order, then calls block events separately.
use super::{Level, LevelError};
use crate::{
    block::{BlockMethod, fluid::FlowingFluid},
    ticks::{ScheduledTick, TickPriority},
};
use ferraloom_data::{Block, registries::fluid::Id as Fluid};
use ferraloom_util::{
    BlockPos, ChunkPos,
    random::{Random, RandomSource},
};
impl Level {
    pub fn tick_world_border(&mut self) {
        self.region_mut().entities.border.tick();
    }
    /// Port of ServerLevel#tick's inactivity gate, after block events. Active
    /// tickets reset it even during freeze; only running simulation advances it.
    /// The returned flag gates dragon/entity/block-entity phases, never entity
    /// storage management.
    pub fn update_empty_time(&mut self, active_tickets: bool, running: bool) -> bool {
        let region = self.region_mut();
        if active_tickets {
            region.empty_time = 0;
        }
        if running {
            region.empty_time = region.empty_time.wrapping_add(1);
        }
        region.empty_time < 300
    }
    pub fn set_game_time(&mut self, game_time: i64) {
        self.region_mut().game_time = game_time;
    }
    pub fn game_time(&self) -> i64 {
        self.region().game_time
    }
    pub fn random(&mut self) -> &mut Random {
        &mut self.region_mut().random
    }
    /// Single-region mode installs the original streams without forking.
    pub fn set_random_streams(&mut self, random: Random, rand_value: i32) {
        let region = self.region_mut();
        region.random = random;
        region.rand_value = rand_value;
    }
    pub fn get_block_random_pos(&mut self, origin: BlockPos, y_mask: i32) -> BlockPos {
        let region = self.region_mut();
        region.rand_value = region
            .rand_value
            .wrapping_mul(3)
            .wrapping_add(1_013_904_223);
        let value = region.rand_value >> 2;
        origin.offset(value & 15, (value >> 16) & y_mask, (value >> 8) & 15)
    }
    /// Port of Util.shuffle at ServerChunkCache's spawning-chunk slot. This
    /// consumes Level#random even when SPAWN_MOBS disables spawn attempts.
    pub fn shuffle_spawning_chunks(&mut self, chunks: &mut [ChunkPos]) {
        for size in (2..=chunks.len()).rev() {
            let other = self.random().next_int_bound(size as i32) as usize;
            chunks.swap(size - 1, other);
        }
    }
    pub fn increment_chunk_inhabited_time(&mut self, pos: ChunkPos) -> Result<(), LevelError> {
        self.region_mut()
            .chunks
            .get_mut(&pos)
            .ok_or(LevelError::MissingChunk(pos))?
            .increment_inhabited_time();
        Ok(())
    }
    pub fn schedule_block_tick_after(
        &mut self,
        pos: BlockPos,
        block: Block,
        delay: i32,
    ) -> Result<(), LevelError> {
        self.schedule_block_tick(
            pos,
            block,
            self.game_time().wrapping_add(i64::from(delay)),
            TickPriority::Normal,
        )
    }
    pub fn schedule_fluid_tick_after(
        &mut self,
        pos: BlockPos,
        fluid: Fluid,
        delay: i32,
    ) -> Result<(), LevelError> {
        let chunk = ChunkPos::containing(pos);
        if self.chunk(chunk).is_none() {
            return Err(LevelError::MissingChunk(chunk));
        }
        let region = self.region_mut();
        let order = region.next_sub_tick_order;
        region.next_sub_tick_order = order.wrapping_add(1);
        region.fluid_ticks.schedule_tick(ScheduledTick {
            pos,
            block: fluid,
            trigger_tick: region.game_time.wrapping_add(i64::from(delay)),
            priority: TickPriority::Normal,
            sub_tick_order: order,
        });
        Ok(())
    }
    pub fn will_tick_block_this_tick(&mut self, pos: BlockPos, block: Block) -> bool {
        self.region_mut()
            .block_ticks
            .will_tick_this_tick(pos, block)
    }
    pub fn has_scheduled_block_tick(&self, pos: BlockPos, block: Block) -> bool {
        self.region().block_ticks.has_scheduled_tick(pos, block)
    }
    pub fn will_tick_fluid_this_tick(&mut self, pos: BlockPos, fluid: Fluid) -> bool {
        self.region_mut()
            .fluid_ticks
            .will_tick_this_tick(pos, fluid)
    }
    pub fn has_scheduled_fluid_tick(&self, pos: BlockPos, fluid: Fluid) -> bool {
        self.region().fluid_ticks.has_scheduled_tick(pos, fluid)
    }
    pub fn tick_block_phase(&mut self, game_time: i64, budget: usize) -> Result<(), LevelError> {
        let region = self.region_mut();
        region.game_time = game_time;
        region.block_ticks.collect(game_time, budget, |p| {
            region.entities.are_entities_loaded(p)
                && region.simulation.borrow().level(p) <= 32
                && region
                    .chunks
                    .get(&p)
                    .is_some_and(|chunk| chunk.ticking_ready())
        });
        let result = self.run_collected_blocks();
        self.region_mut().block_ticks.cleanup_after_tick();
        result
    }
    pub fn tick_fluid_phase(&mut self, game_time: i64, budget: usize) -> Result<(), LevelError> {
        let region = self.region_mut();
        region.game_time = game_time;
        region.fluid_ticks.collect(game_time, budget, |p| {
            region.entities.are_entities_loaded(p)
                && region.simulation.borrow().level(p) <= 32
                && region
                    .chunks
                    .get(&p)
                    .is_some_and(|chunk| chunk.ticking_ready())
        });
        let result = self.run_collected_fluids();
        self.region_mut().fluid_ticks.cleanup_after_tick();
        result
    }
    fn run_collected_fluids(&mut self) -> Result<(), LevelError> {
        while let Some(tick) = self.region_mut().fluid_ticks.poll_this_tick() {
            let state = self
                .get_block_state(tick.pos)
                .map_err(|error| error.at(tick.pos, "scheduled fluid tick"))?;
            let fluid = state.data().fluid;
            if fluid.fluid_type == tick.block
                && let Some(flow) = FlowingFluid::from_id(tick.block)
            {
                self.optional_call(state.block(), BlockMethod::FluidTick, |level| {
                    flow.tick(level, tick.pos, state, fluid)
                })
                .map_err(|error| error.at(tick.pos, "scheduled fluid tick"))?;
            }
        }
        Ok(())
    }
    /// Port of ServerLevel#tickThunder, called in the shuffled spawning pass.
    /// Target/rod, biome rain, difficulty and lightning/trap effects are pending.
    pub fn tick_thunder(&mut self, chunk_pos: ChunkPos) -> Result<(), LevelError> {
        self.chunk(chunk_pos)
            .ok_or(LevelError::MissingChunk(chunk_pos))?;
        if self.is_raining() && self.is_thundering() && self.random().next_int_bound(100_000) == 0 {
            self.get_block_random_pos(BlockPos::new(chunk_pos.x * 16, 0, chunk_pos.z * 16), 15);
            self.unported_operation(crate::block::UnportedOperation::WeatherStrike)?;
        }
        Ok(())
    }
    /// Port of ServerLevel#tickChunk: precipitation slots, then section samples.
    pub fn tick_random_chunk(&mut self, chunk_pos: ChunkPos) -> Result<(), LevelError> {
        let speed = self.rules().random_tick_speed;
        let chunk = self
            .chunk(chunk_pos)
            .ok_or(LevelError::MissingChunk(chunk_pos))?;
        let min = chunk.min_section();
        let count = chunk.sections().count();
        // ServerLevel#tickChunk always consumes the precipitation selection
        // slots before visiting any randomly ticking section, even when dry.
        for _ in 0..speed {
            if self.random().next_int_bound(48) == 0 {
                let pos = self
                    .get_block_random_pos(BlockPos::new(chunk_pos.x * 16, 0, chunk_pos.z * 16), 15);
                self.tick_precipitation(pos)
                    .map_err(|error| error.at(pos, "precipitation tick"))?;
            }
        }
        if speed == 0 {
            return Ok(());
        }
        for index in 0..count {
            // Vanilla tests section eligibility once, before sampling it. A
            // callback may change the section without cancelling later samples.
            if !self
                .chunk(chunk_pos)
                .unwrap()
                .section(min + index as i32)
                .unwrap()
                .is_randomly_ticking()
            {
                continue;
            }
            for _ in 0..speed {
                let pos = self.get_block_random_pos(
                    BlockPos::new(
                        chunk_pos.x * 16,
                        (min + index as i32) * 16,
                        chunk_pos.z * 16,
                    ),
                    15,
                );
                let state = self
                    .get_block_state(pos)
                    .map_err(|error| error.at(pos, "random block tick"))?;
                if state.data().random_ticking {
                    self.optional_block_call(
                        state.block(),
                        BlockMethod::RandomTick,
                        |behavior, level| behavior.random_tick(state, pos, level),
                    )
                    .map_err(|error| error.at(pos, "random block tick"))?;
                }
                // The fluid is captured from the original state, even if the
                // block callback changed it (LiquidBlock invokes lava twice).
                let fluid = state.data().fluid;
                if fluid.is_randomly_ticking() {
                    self.optional_call(state.block(), BlockMethod::FluidRandomTick, |level| {
                        FlowingFluid::from_id(fluid.fluid_type)
                            .ok_or(LevelError::UnsupportedFluidRandomTick)?
                            .random_tick(level, pos, fluid)
                    })
                    .map_err(|error| error.at(pos, "random fluid tick"))?;
                }
            }
        }
        Ok(())
    }
}
