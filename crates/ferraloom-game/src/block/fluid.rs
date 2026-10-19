//! Ports of FlowingFluid, WaterFluid, LavaFluid, LiquidBlock and
//! SimpleWaterloggedBlock. Collision queries delegate to the shape port.
use super::{BlockBehavior, LevelAccessRef, UPDATE_LIMIT, UpdateFlags};
use crate::level::{Level, LevelError, effects::LevelEffect};
use ferraloom_data::{
    Block, BlockClass, BlockState, FluidState, Item, properties, registries::fluid::Id,
    tags::FluidTags,
};
use ferraloom_util::{BlockPos, Direction, core::Plane, random::RandomSource};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlowingFluid {
    Water,
    Lava,
}
impl FlowingFluid {
    pub fn from_id(id: Id) -> Option<Self> {
        match id {
            Id::WATER | Id::FLOWING_WATER => Some(Self::Water),
            Id::LAVA | Id::FLOWING_LAVA => Some(Self::Lava),
            _ => None,
        }
    }
    pub fn source_id(self) -> Id {
        match self {
            Self::Water => Id::WATER,
            Self::Lava => Id::LAVA,
        }
    }
    pub fn flowing_id(self) -> Id {
        match self {
            Self::Water => Id::FLOWING_WATER,
            Self::Lava => Id::FLOWING_LAVA,
        }
    }
    pub fn source(self) -> FluidState {
        FluidState {
            fluid_type: self.source_id(),
            amount: 8,
            source: true,
            empty: false,
            falling: false,
        }
    }
    pub fn flowing(self, amount: u8, falling: bool) -> FluidState {
        FluidState {
            fluid_type: self.flowing_id(),
            amount,
            source: false,
            empty: false,
            falling,
        }
    }
    fn same(self, state: FluidState) -> bool {
        Self::from_id(state.fluid_type) == Some(self)
    }
    pub fn tick_delay(self, fast_lava: bool) -> i32 {
        match self {
            Self::Water => 5,
            Self::Lava => {
                if fast_lava {
                    10
                } else {
                    30
                }
            }
        }
    }
    pub fn tick_delay_for(self, level: &Level) -> Result<i32, LevelError> {
        Ok(self.tick_delay(self == Self::Lava && level.fast_lava()?))
    }
    fn drop_off(self, level: &Level) -> Result<u8, LevelError> {
        Ok(if self == Self::Lava && !level.fast_lava()? {
            2
        } else {
            1
        })
    }
    fn slope_find_distance(self, level: &Level) -> Result<i32, LevelError> {
        Ok(if self == Self::Lava && !level.fast_lava()? {
            2
        } else {
            4
        })
    }
    fn can_convert_to_source(self, level: &Level) -> bool {
        match self {
            Self::Water => level.rules().water_source_conversion,
            Self::Lava => level.rules().lava_source_conversion,
        }
    }
    pub fn height(
        self,
        fluid: FluidState,
        level: &Level,
        pos: BlockPos,
    ) -> Result<f32, LevelError> {
        Ok(
            if self.same(level.get_block_state(pos.above(1))?.data().fluid) {
                1.0
            } else {
                f32::from(fluid.amount) / 9.0
            },
        )
    }
    /// Port of FlowingFluid#getFlow. Float height differences are widened only
    /// after multiplying by the horizontal direction, as in vanilla.
    pub fn get_flow(
        self,
        fluid: FluidState,
        level: &Level,
        pos: BlockPos,
    ) -> Result<ferraloom_util::Vec3, LevelError> {
        let mut flow_x = 0.0;
        let mut flow_z = 0.0;
        let own_height = f32::from(fluid.amount) / 9.0;
        for direction in Plane::Horizontal {
            let neighbor = pos.relative(direction, 1);
            let neighbor_state = level.get_block_state(neighbor)?;
            let neighbor_fluid = neighbor_state.data().fluid;
            if !neighbor_fluid.empty && !self.same(neighbor_fluid) {
                continue;
            }
            let mut neighbor_height = f32::from(neighbor_fluid.amount) / 9.0;
            let mut difference = 0.0_f32;
            if neighbor_height == 0.0 {
                if !level.block_tags()?.contains_key(
                    ferraloom_data::tags::BlockTags::BLOCKS_FLUID_FLOW,
                    neighbor_state.block(),
                ) {
                    let below = level.get_block_state(neighbor.below(1))?.data().fluid;
                    if below.empty || self.same(below) {
                        neighbor_height = f32::from(below.amount) / 9.0;
                        if neighbor_height > 0.0 {
                            difference = own_height - (neighbor_height - 0.888_888_9_f32);
                        }
                    }
                }
            } else if neighbor_height > 0.0 {
                difference = own_height - neighbor_height;
            }
            if difference != 0.0 {
                flow_x += f64::from(direction.step().x as f32 * difference);
                flow_z += f64::from(direction.step().z as f32 * difference);
            }
        }
        let mut flow = ferraloom_util::Vec3::new(flow_x, 0.0, flow_z);
        if fluid.falling {
            for direction in Plane::Horizontal {
                let neighbor = pos.relative(direction, 1);
                if self.is_solid_flow_face(level, neighbor, direction)?
                    || self.is_solid_flow_face(level, neighbor.above(1), direction)?
                {
                    flow = flow.normalize().add_xyz(0.0, -6.0, 0.0);
                    break;
                }
            }
        }
        Ok(flow.normalize())
    }
    fn is_solid_flow_face(
        self,
        level: &Level,
        pos: BlockPos,
        direction: Direction,
    ) -> Result<bool, LevelError> {
        let state = level.get_block_state(pos)?;
        if self.same(state.data().fluid) {
            return Ok(false);
        }
        if direction == Direction::Up {
            return Ok(true);
        }
        if state.block().is_instance_of(BlockClass::IceBlock) {
            return Ok(false);
        }
        super::LevelReaderRef::Server(level).is_face_sturdy(state, pos, direction)
    }
    fn is_source(self, state: FluidState) -> bool {
        self.same(state) && state.source
    }
    fn can_replace(
        self,
        current: FluidState,
        new: Id,
        direction: Direction,
        level: &Level,
        pos: BlockPos,
    ) -> Result<bool, LevelError> {
        if current.empty {
            return Ok(true);
        }
        match Self::from_id(current.fluid_type) {
            Some(Self::Water) => Ok(direction == Direction::Down
                && !level.fluid_tags()?.contains_key(FluidTags::WATER, new)),
            Some(Self::Lava) => Ok(Self::Lava.height(current, level, pos)? >= 0.44444445
                && level.fluid_tags()?.contains_key(FluidTags::WATER, new)),
            None => Ok(false),
        }
    }
    fn can_hold_any(state: BlockState, level: &Level) -> Result<bool, LevelError> {
        Ok(state
            .block()
            .is_instance_of(BlockClass::LiquidBlockContainer)
            || level.block_tags()?.contains_key(
                ferraloom_data::tags::BlockTags::WASHED_AWAY_BY_FLUIDS,
                state.block(),
            ))
    }
    fn can_hold_specific(state: BlockState, fluid: Id, level: &Level) -> Result<bool, LevelError> {
        if state
            .block()
            .is_instance_of(BlockClass::LiquidBlockContainer)
        {
            return Ok(level
                .required_block_behavior(state.block(), super::BlockMethod::CanPlaceLiquid)?
                .can_place_liquid(state, fluid));
        }
        Ok(true)
    }
    fn can_maybe_pass(
        self,
        level: &Level,
        pos: BlockPos,
        state: BlockState,
        direction: Direction,
        target: BlockPos,
        target_state: BlockState,
    ) -> Result<bool, LevelError> {
        Ok(!self.is_source(target_state.data().fluid)
            && Self::can_hold_any(target_state, level)?
            && can_pass_through_wall(direction, level, pos, state, target, target_state)?)
    }
    fn can_pass(
        self,
        level: &Level,
        pos: BlockPos,
        state: BlockState,
        direction: Direction,
        target: BlockPos,
        target_state: BlockState,
    ) -> Result<bool, LevelError> {
        Ok(
            self.can_maybe_pass(level, pos, state, direction, target, target_state)?
                && Self::can_hold_specific(target_state, self.flowing_id(), level)?,
        )
    }
    fn is_water_hole(
        self,
        level: &Level,
        pos: BlockPos,
        state: BlockState,
        below: BlockPos,
        below_state: BlockState,
    ) -> Result<bool, LevelError> {
        Ok(
            can_pass_through_wall(Direction::Down, level, pos, state, below, below_state)?
                && (self.same(below_state.data().fluid)
                    || (Self::can_hold_any(below_state, level)?
                        && Self::can_hold_specific(below_state, self.flowing_id(), level)?)),
        )
    }
    pub fn get_new_liquid(
        self,
        level: &Level,
        pos: BlockPos,
        state: BlockState,
    ) -> Result<FluidState, LevelError> {
        let mut highest = 0;
        let mut sources = 0;
        for direction in Plane::Horizontal {
            let target = pos.relative(direction, 1);
            let target_state = level.get_block_state(target)?;
            let fluid = target_state.data().fluid;
            if self.same(fluid)
                && can_pass_through_wall(direction, level, pos, state, target, target_state)?
            {
                sources += u8::from(fluid.source);
                highest = highest.max(fluid.amount);
            }
        }
        if sources >= 2 && self.can_convert_to_source(level) {
            let below = level.get_block_state(pos.below(1))?;
            if below.is_solid() || self.is_source(below.data().fluid) {
                return Ok(self.source());
            }
        }
        let above_pos = pos.above(1);
        let above = level.get_block_state(above_pos)?;
        if self.same(above.data().fluid)
            && !above.data().fluid.empty
            && can_pass_through_wall(Direction::Up, level, pos, state, above_pos, above)?
        {
            return Ok(self.flowing(8, true));
        }
        let amount = highest.saturating_sub(self.drop_off(level)?);
        Ok(if amount == 0 {
            empty_fluid()
        } else {
            self.flowing(amount, false)
        })
    }
    pub(crate) fn slope_distance(
        self,
        level: &Level,
        pos: BlockPos,
        pass: i32,
        from: Direction,
        state: BlockState,
    ) -> Result<i32, LevelError> {
        let mut lowest = 1000;
        for direction in Plane::Horizontal {
            if direction == from {
                continue;
            }
            let target = pos.relative(direction, 1);
            let target_state = level.get_block_state(target)?;
            if self.can_pass(level, pos, state, direction, target, target_state)? {
                let below = target.below(1);
                if self.is_water_hole(
                    level,
                    target,
                    target_state,
                    below,
                    level.get_block_state(below)?,
                )? {
                    return Ok(pass);
                }
                if pass < self.slope_find_distance(level)? {
                    lowest = lowest.min(self.slope_distance(
                        level,
                        target,
                        pass + 1,
                        direction.opposite(),
                        target_state,
                    )?);
                }
            }
        }
        Ok(lowest)
    }
    fn get_spread(
        self,
        level: &Level,
        pos: BlockPos,
        state: BlockState,
    ) -> Result<Vec<(Direction, FluidState)>, LevelError> {
        let mut lowest = 1000;
        let mut result = Vec::new();
        for direction in Plane::Horizontal {
            let target = pos.relative(direction, 1);
            let target_state = level.get_block_state(target)?;
            if !self.can_maybe_pass(level, pos, state, direction, target, target_state)? {
                continue;
            }
            let fluid = self.get_new_liquid(level, target, target_state)?;
            if !Self::can_hold_specific(target_state, fluid.fluid_type, level)? {
                continue;
            }
            let below = target.below(1);
            let distance = if self.is_water_hole(
                level,
                target,
                target_state,
                below,
                level.get_block_state(below)?,
            )? {
                0
            } else {
                self.slope_distance(level, target, 1, direction.opposite(), target_state)?
            };
            if distance < lowest {
                result.clear();
            }
            if distance <= lowest {
                if self.can_replace(
                    target_state.data().fluid,
                    fluid.fluid_type,
                    direction,
                    level,
                    target,
                )? {
                    result.push((direction, fluid));
                }
                lowest = distance;
            }
        }
        // Maps.newEnumMap iterates Direction declaration order, which differs
        // from the N,E,S,W search order used above.
        result.sort_by_key(|(direction, _)| *direction as u8);
        Ok(result)
    }
    fn spread_to(
        self,
        level: &mut Level,
        pos: BlockPos,
        state: BlockState,
        direction: Direction,
        target: FluidState,
        ticking_fluid: Id,
    ) -> Result<(), LevelError> {
        if self == Self::Lava
            && direction == Direction::Down
            && level
                .fluid_tags()?
                .contains_key(FluidTags::LAVA, ticking_fluid)
            && level
                .fluid_tags()?
                .contains_key(FluidTags::WATER, state.data().fluid.fluid_type)
        {
            if state.block().class() == BlockClass::LiquidBlock {
                level.set_block(
                    pos,
                    Block::STONE.default_state(),
                    UpdateFlags::ALL,
                    UPDATE_LIMIT,
                )?;
            }
            fizz(level, pos);
            return Ok(());
        }
        if state
            .block()
            .is_instance_of(BlockClass::LiquidBlockContainer)
        {
            level
                .required_block_behavior(state.block(), super::BlockMethod::PlaceLiquid)?
                .place_liquid(state, pos, target, &mut LevelAccessRef::Server(level))?;
        } else {
            if !state.is_air() {
                if self == Self::Water {
                    let block_entity = if state.has_block_entity() {
                        level.get_or_promote_block_entity(pos)?
                    } else {
                        None
                    };
                    crate::block::drops::drop_resources(state, pos, block_entity, level)?;
                } else {
                    fizz(level, pos);
                }
            }
            level.set_block(pos, legacy_block(target), UpdateFlags::ALL, UPDATE_LIMIT)?;
        }
        Ok(())
    }
    fn spread_to_sides(
        self,
        level: &mut Level,
        pos: BlockPos,
        fluid: FluidState,
        state: BlockState,
        ticking_fluid: Id,
    ) -> Result<(), LevelError> {
        let amount = if fluid.falling {
            7
        } else {
            fluid.amount.saturating_sub(self.drop_off(level)?)
        };
        if amount > 0 {
            for (direction, new_fluid) in self.get_spread(level, pos, state)? {
                let target = pos.relative(direction, 1);
                self.spread_to(
                    level,
                    target,
                    level.get_block_state(target)?,
                    direction,
                    new_fluid,
                    ticking_fluid,
                )?;
            }
        }
        Ok(())
    }
    fn spread(
        self,
        level: &mut Level,
        pos: BlockPos,
        state: BlockState,
        fluid: FluidState,
        ticking_fluid: Id,
    ) -> Result<(), LevelError> {
        if fluid.empty {
            return Ok(());
        }
        let below = pos.below(1);
        let below_state = level.get_block_state(below)?;
        if self.can_maybe_pass(level, pos, state, Direction::Down, below, below_state)? {
            let new = self.get_new_liquid(level, below, below_state)?;
            if self.can_replace(
                below_state.data().fluid,
                new.fluid_type,
                Direction::Down,
                level,
                below,
            )? && Self::can_hold_specific(below_state, new.fluid_type, level)?
            {
                self.spread_to(
                    level,
                    below,
                    below_state,
                    Direction::Down,
                    new,
                    ticking_fluid,
                )?;
                let mut sources = 0;
                for direction in Plane::Horizontal {
                    sources += u8::from(
                        self.is_source(
                            level
                                .get_block_state(pos.relative(direction, 1))?
                                .data()
                                .fluid,
                        ),
                    );
                }
                if sources >= 3 {
                    self.spread_to_sides(level, pos, fluid, state, ticking_fluid)?;
                }
                return Ok(());
            }
        }
        if fluid.source || !self.is_water_hole(level, pos, state, below, below_state)? {
            self.spread_to_sides(level, pos, fluid, state, ticking_fluid)?;
        }
        Ok(())
    }
    pub fn tick(
        self,
        level: &mut Level,
        pos: BlockPos,
        mut state: BlockState,
        mut fluid: FluidState,
    ) -> Result<(), LevelError> {
        let ticking_fluid = fluid.fluid_type;
        if !fluid.source {
            let new = self.get_new_liquid(level, pos, level.get_block_state(pos)?)?;
            let mut delay = self.tick_delay_for(level)?;
            if self == Self::Lava
                && !fluid.empty
                && !new.empty
                && !fluid.falling
                && !new.falling
                && self.height(new, level, pos)? > self.height(fluid, level, pos)?
                && level.random().next_int_bound(4) != 0
            {
                delay *= 4;
            }
            if new.empty {
                fluid = new;
                state = Block::AIR.default_state();
                level.set_block(pos, state, UpdateFlags::ALL, UPDATE_LIMIT)?;
            } else if !same_state(new, fluid) {
                fluid = new;
                state = legacy_block(fluid);
                level.set_block(pos, state, UpdateFlags::ALL, UPDATE_LIMIT)?;
                level.schedule_fluid_tick_after(pos, fluid.fluid_type, delay)?;
            }
        }
        self.spread(level, pos, state, fluid, ticking_fluid)
    }
    pub fn random_tick(
        self,
        level: &mut Level,
        _pos: BlockPos,
        _fluid: FluidState,
    ) -> Result<(), LevelError> {
        if self == Self::Lava && level.rules().spread_fire {
            return Err(LevelError::UnsupportedFluidRandomTick);
        }
        Ok(())
    }
}
pub fn empty_fluid() -> FluidState {
    Block::AIR.default_state().data().fluid
}
fn same_state(a: FluidState, b: FluidState) -> bool {
    a.fluid_type == b.fluid_type
        && a.amount == b.amount
        && a.falling == b.falling
        && a.source == b.source
}
pub fn legacy_block(fluid: FluidState) -> BlockState {
    let Some(flow) = FlowingFluid::from_id(fluid.fluid_type) else {
        return Block::AIR.default_state();
    };
    let block = if flow == FlowingFluid::Water {
        Block::WATER
    } else {
        Block::LAVA
    };
    let level = if fluid.source {
        0
    } else {
        8 - fluid.amount.min(8) + if fluid.falling { 8 } else { 0 }
    };
    block
        .default_state()
        .with(properties::LEVEL, level)
        .expect("LiquidBlock level 0..15")
}
pub(crate) fn can_pass_through_wall(
    direction: Direction,
    level: &Level,
    source_pos: BlockPos,
    source: BlockState,
    target_pos: BlockPos,
    target: BlockState,
) -> Result<bool, LevelError> {
    let reader = super::LevelReaderRef::Server(level);
    // Vanilla queries the target before the source and preserves Shapes.block
    // identity shortcuts, then uses native voxel occupancy for partial faces.
    let target_shape = reader.collision_shape_without_context(target, target_pos)?;
    if target_shape.ptr_eq(&ferraloom_util::shapes::block()) {
        return Ok(false);
    }
    let source_shape = reader.collision_shape_without_context(source, source_pos)?;
    Ok(!ferraloom_util::shapes::merged_face_occludes(
        &source_shape,
        &target_shape,
        direction,
    ))
}

fn fizz(level: &mut Level, pos: BlockPos) {
    level.emit(LevelEffect::LevelEvent {
        pos,
        id: 1501,
        data: 0,
    });
}

pub struct LiquidBlock;
pub static LIQUID_BLOCK: LiquidBlock = LiquidBlock;
impl LiquidBlock {
    fn should_spread_liquid(
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<bool, LevelError> {
        if level.fluid_tags()?.contains_key(
            FluidTags::LAVA,
            FlowingFluid::from_id(state.data().fluid.fluid_type)
                .unwrap()
                .source_id(),
        ) {
            let soul_soil = level.get_block_state(pos.below(1))?.block() == Block::SOUL_SOIL;
            for direction in [
                Direction::Down,
                Direction::South,
                Direction::North,
                Direction::East,
                Direction::West,
            ] {
                let target = pos.relative(direction.opposite(), 1);
                let neighbor = level.get_block_state(target)?;
                let convert = if level
                    .fluid_tags()?
                    .contains_key(FluidTags::WATER, neighbor.data().fluid.fluid_type)
                {
                    Some(if level.get_block_state(pos)?.data().fluid.source {
                        Block::OBSIDIAN
                    } else {
                        Block::COBBLESTONE
                    })
                } else if soul_soil && neighbor.block() == Block::BLUE_ICE {
                    Some(Block::BASALT)
                } else {
                    None
                };
                if let Some(block) = convert {
                    level.set_block(pos, block.default_state(), UpdateFlags::ALL, UPDATE_LIMIT)?;
                    fizz(level, pos);
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
    fn schedule_bubble(
        state: BlockState,
        pos: BlockPos,
        below: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<(), LevelError> {
        if level.as_reader().fluid_tags()?.contains_key(
            FluidTags::BUBBLE_COLUMN_CAN_OCCUPY,
            state.data().fluid.fluid_type,
        ) && state.data().fluid.source
            && state.data().fluid.amount == 8
        {
            use ferraloom_data::tags::BlockTags;
            let reader = level.as_reader();
            let tags = reader.block_tags()?;
            let enabled = tags
                .contains_key(BlockTags::ENABLES_BUBBLE_COLUMN_DRAG_DOWN, below.block())
                || tags.contains_key(BlockTags::ENABLES_BUBBLE_COLUMN_PUSH_UP, below.block());
            if enabled {
                level.schedule_block_tick(pos, state.block(), 20)?;
            }
        }
        Ok(())
    }
}
impl BlockBehavior for LiquidBlock {
    /// Port of LiquidBlock#isPathfindable. Test the block's source fluid, not
    /// the flowing state at LEVEL>0, including when fluid tags are reloaded.
    fn is_pathfindable(
        &self,
        state: BlockState,
        _computation: crate::entity::ai::pathfinder::PathComputationType,
        level: &super::LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Ok(!level.fluid_tags()?.contains_key(
            FluidTags::LAVA,
            state.block().default_state().data().fluid.fluid_type,
        ))
    }
    /// Port of LiquidBlock#getDrops's empty override, without a loot lookup.
    fn get_drops(
        &self,
        _state: BlockState,
        _context: super::drops::BlockLootContext,
        _level: &mut Level,
    ) -> Result<Vec<ferraloom_protocol::component::ItemStack>, LevelError> {
        Ok(Vec::new())
    }
    fn collision_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &super::BlockCollisionContext<'_>,
        level: &super::LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        use ferraloom_util::shapes::CollisionContext;
        if context.always_collide_with_fluid() {
            return Ok(ferraloom_util::shapes::block());
        }
        if state.get(properties::LEVEL) != Some(0) {
            return Ok(ferraloom_util::shapes::empty());
        }
        if let super::BlockCollisionContext::Entity(entity_context) = context {
            let liquid_shape = match level {
                super::LevelReaderRef::Server(level) => {
                    entity_context.entity.liquid_collision_shape(level)?
                }
                super::LevelReaderRef::PathNavigation(reader) => {
                    reader.liquid_collision_shape(&entity_context.entity)?
                }
                super::LevelReaderRef::Worldgen(_) => {
                    return Err(LevelError::UnsupportedBlockMethod(
                        state.block(),
                        super::BlockMethod::CollisionShape,
                    ));
                }
            };
            if let Some(shape) = liquid_shape
                && context.is_above(&shape, pos, true)
                && context.can_stand_on_fluid(
                    &level.get_block_state(pos.above(1))?.data().fluid,
                    &state.data().fluid,
                )
            {
                return Ok(shape);
            }
        }
        Ok(ferraloom_util::shapes::empty())
    }
    fn shape(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &super::LevelReaderRef<'_, '_>,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        Ok(ferraloom_util::shapes::empty())
    }
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        _old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let flow = FlowingFluid::from_id(state.data().fluid.fluid_type).unwrap();
        if Self::should_spread_liquid(state, pos, level)? {
            level.schedule_fluid_tick_after(
                pos,
                state.data().fluid.fluid_type,
                flow.tick_delay_for(level)?,
            )?;
        }
        if level.fluid_tags()?.contains_key(
            FluidTags::BUBBLE_COLUMN_CAN_OCCUPY,
            state.data().fluid.fluid_type,
        ) && state.data().fluid.source
            && state.data().fluid.amount == 8
        {
            let below = level.get_block_state(pos.below(1))?;
            Self::schedule_bubble(state, pos, below, &mut LevelAccessRef::Server(level))?;
        }
        Ok(())
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        _source: Block,
        _orientation: Option<crate::level::redstone::Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        self.on_place(state, pos, state, false, level)
    }
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if state.data().fluid.source || neighbor.data().fluid.source {
            let flow = FlowingFluid::from_id(state.data().fluid.fluid_type).unwrap();
            let fast_lava = flow == FlowingFluid::Lava && level.fast_lava()?;
            let delay = flow.tick_delay(fast_lava);
            level.schedule_fluid_tick(pos, state.data().fluid.fluid_type, delay)?;
        }
        if direction == Direction::Down {
            Self::schedule_bubble(state, pos, neighbor, level)?;
        }
        Ok(state)
    }
    fn random_tick(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        FlowingFluid::from_id(state.data().fluid.fluid_type)
            .unwrap()
            .random_tick(level, pos, state.data().fluid)
    }
    fn pickup_block(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<Item>, LevelError> {
        if state.get(properties::LEVEL) != Some(0) {
            return Ok(None);
        }
        level.set_block(pos, Block::AIR.default_state(), UpdateFlags::ALL_IMMEDIATE)?;
        Ok(Some(
            if FlowingFluid::from_id(state.data().fluid.fluid_type) == Some(FlowingFluid::Water) {
                Item::WATER_BUCKET
            } else {
                Item::LAVA_BUCKET
            },
        ))
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        if level.fluid_tags()?.contains_key(
            FluidTags::BUBBLE_COLUMN_CAN_OCCUPY,
            state.data().fluid.fluid_type,
        ) && state.data().fluid.source
            && state.data().fluid.amount == 8
        {
            use ferraloom_data::tags::BlockTags;
            let below = level.get_block_state(pos.below(1))?;
            if level
                .block_tags()?
                .contains_key(BlockTags::ENABLES_BUBBLE_COLUMN_DRAG_DOWN, below.block())
                || level
                    .block_tags()?
                    .contains_key(BlockTags::ENABLES_BUBBLE_COLUMN_PUSH_UP, below.block())
            {
                return Err(LevelError::UnsupportedBlockMethod(
                    Block::BUBBLE_COLUMN,
                    super::BlockMethod::BubbleColumnUpdate,
                ));
            }
        }
        Ok(())
    }
}

pub struct SimpleWaterloggedBlock;
pub static SIMPLE_WATERLOGGED_BLOCK: SimpleWaterloggedBlock = SimpleWaterloggedBlock;
impl BlockBehavior for SimpleWaterloggedBlock {
    fn can_place_liquid(&self, _state: BlockState, fluid: Id) -> bool {
        fluid == Id::WATER
    }
    fn place_liquid(
        &self,
        state: BlockState,
        pos: BlockPos,
        fluid: FluidState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        Self::place_liquid(state, pos, fluid, level)
    }
    fn pickup_block(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<Item>, LevelError> {
        Self::pickup_block(state, pos, level)
    }
}
impl SimpleWaterloggedBlock {
    pub fn place_liquid(
        state: BlockState,
        pos: BlockPos,
        fluid: FluidState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        if state.get(properties::WATERLOGGED) != Some(false) || fluid.fluid_type != Id::WATER {
            return Ok(false);
        }
        level.set_block(
            pos,
            state.with(properties::WATERLOGGED, true).unwrap(),
            UpdateFlags::ALL,
        )?;
        level.schedule_fluid_tick(pos, fluid.fluid_type, 5)?;
        Ok(true)
    }
    pub fn pickup_block(
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<Option<Item>, LevelError> {
        if state.get(properties::WATERLOGGED) != Some(true) {
            return Ok(None);
        }
        level.set_block(
            pos,
            state.with(properties::WATERLOGGED, false).unwrap(),
            UpdateFlags::ALL,
        )?;
        let reader = level.as_reader();
        let survives = reader.can_survive(state, pos)?;
        if !survives {
            level.destroy_block(pos, true)?;
        }
        Ok(Some(Item::WATER_BUCKET))
    }
}
