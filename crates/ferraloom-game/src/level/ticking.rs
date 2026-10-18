//! ServerLevel's scheduled and random tick primitives. The phase driver supplies
//! dimension-wide budgets and chunk order, then calls block events separately.
use super::{Level, LevelError};
use crate::{
    block::{BlockMethod, fluid::FlowingFluid},
    ticks::{ScheduledTick, TickPriority},
};
use ferraloom_data::{Block, registries::fluid::Id as Fluid};
use ferraloom_util::{BlockPos, ChunkPos, random::Random};
impl Level {
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
            region.block_ticking_chunks.contains(&p)
        });
        let result = self.run_collected_blocks();
        self.region_mut().block_ticks.cleanup_after_tick();
        result
    }
    pub fn tick_fluid_phase(&mut self, game_time: i64, budget: usize) -> Result<(), LevelError> {
        let region = self.region_mut();
        region.game_time = game_time;
        region.fluid_ticks.collect(game_time, budget, |p| {
            region.block_ticking_chunks.contains(&p)
        });
        let result = self.run_collected_fluids();
        self.region_mut().fluid_ticks.cleanup_after_tick();
        result
    }
    fn run_collected_fluids(&mut self) -> Result<(), LevelError> {
        while let Some(tick) = self.region_mut().fluid_ticks.poll_this_tick() {
            let state = self.get_block_state(tick.pos)?;
            let fluid = state.data().fluid;
            if fluid.fluid_type == tick.block {
                if let Some(flow) = FlowingFluid::from_id(tick.block) {
                    self.optional_call(state.block(), BlockMethod::FluidTick, |level| {
                        flow.tick(level, tick.pos, state, fluid)
                    })?;
                }
            }
        }
        Ok(())
    }
    pub fn tick_random_chunk(&mut self, chunk_pos: ChunkPos) -> Result<(), LevelError> {
        let speed = self.rules().random_tick_speed;
        if speed == 0 {
            return Ok(());
        }
        let chunk = self
            .chunk(chunk_pos)
            .ok_or(LevelError::MissingChunk(chunk_pos))?;
        let min = chunk.min_section();
        let count = chunk.sections().count();
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
                let state = self.get_block_state(pos)?;
                if state.data().random_ticking {
                    self.optional_block_call(
                        state.block(),
                        BlockMethod::RandomTick,
                        |behavior, level| behavior.random_tick(state, pos, level),
                    )?;
                }
                // The fluid is captured from the original state, even if the
                // block callback changed it (LiquidBlock invokes lava twice).
                if let Some(flow) = FlowingFluid::from_id(state.data().fluid.fluid_type) {
                    self.optional_call(state.block(), BlockMethod::FluidRandomTick, |level| {
                        flow.random_tick(level, pos, state.data().fluid)
                    })?;
                }
            }
        }
        Ok(())
    }
}
