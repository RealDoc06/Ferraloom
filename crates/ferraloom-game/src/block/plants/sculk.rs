//! Ports of SculkSpreader, ChargeCursor and the participating SculkBehaviour methods.
//! Both level arms use the same cursor algorithm and native caller random stream.
use super::{multiface, multiface_spreader::SpreaderConfig};
use crate::{
    block::{LevelAccessRef, LevelReaderRef, UpdateFlags},
    level::LevelError,
};
use ferraloom_data::{Block, BlockState, properties as p, tags::BlockTags};
use ferraloom_protocol::play::SoundSource;
use ferraloom_util::{
    BlockPos, Direction, Vec3,
    collections::{JavaHashMap, Object2IntOpenHashMap},
    random::RandomSource,
    shapes::{self, BooleanOp},
};
use ferraloom_worldgen::feature::FeatureRandomSelector;

#[derive(Clone, Copy, Debug)]
pub struct SpreadSettings {
    pub world_generation: bool,
    pub growth_spawn_cost: i32,
    pub no_growth_radius: i32,
    pub charge_decay_rate: i32,
    pub additional_decay_rate: i32,
}
impl SpreadSettings {
    pub fn replaceable_blocks(self) -> ferraloom_data::datapack::TagKey<Block> {
        if self.world_generation {
            BlockTags::SCULK_REPLACEABLE_WORLD_GEN
        } else {
            BlockTags::SCULK_REPLACEABLE
        }
    }
}

/// Port of SculkSpreader.ChargeCursor; None and an empty facing set are distinct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargeCursor {
    pub pos: BlockPos,
    pub charge: i32,
    pub decay_delay: i32,
    pub update_delay: i32,
    pub facings: Option<u8>,
}
impl ChargeCursor {
    pub fn new(pos: BlockPos, charge: i32) -> Self {
        Self {
            pos,
            charge,
            decay_delay: 1,
            update_delay: 0,
            facings: None,
        }
    }
    fn update(
        &mut self,
        level: &mut LevelAccessRef<'_, '_>,
        origin: BlockPos,
        random: &mut FeatureRandomSelector<'_>,
        settings: SpreadSettings,
        spread_veins: bool,
    ) -> Result<(), LevelError> {
        if self.charge <= 0
            || !settings.world_generation
                && !matches!(level,
            LevelAccessRef::Server(live) if live.should_tick_blocks_at(origin))
        {
            return Ok(());
        }
        if self.update_delay > 0 {
            self.update_delay -= 1;
            return Ok(());
        }
        let mut state = level.get_block_state(self.pos)?;
        let mut behavior = SculkBehaviour::of(state);
        if spread_veins
            && behavior.attempt_spread_vein(
                level,
                self.pos,
                state,
                self.facings,
                settings.world_generation,
            )?
        {
            if behavior.can_change_block_state_on_spread() {
                state = level.get_block_state(self.pos)?;
                behavior = SculkBehaviour::of(state);
            }
            play_spread_sound(level, self.pos)?;
        }
        self.charge =
            behavior.attempt_use_charge(self, level, origin, random, settings, spread_veins)?;
        if self.charge <= 0 {
            behavior.on_discharged(level, state, self.pos)?;
        } else {
            if let Some(next) = movement_pos(level, self.pos, origin, random, settings)? {
                behavior.on_discharged(level, state, self.pos)?;
                self.pos = next;
                state = level.get_block_state(next)?;
            } else if settings.world_generation {
                behavior.on_discharged(level, state, self.pos)?;
                self.charge = 0;
                return Ok(());
            }
            if is_sculk_behavior(state) {
                self.facings = Some(if state.block() == Block::SCULK_VEIN {
                    face_mask(state)
                } else {
                    0
                });
            }
            self.decay_delay = behavior.update_decay_delay(self.decay_delay);
            self.update_delay = behavior.sculk_spread_delay();
        }
        Ok(())
    }
}

/// Port of SculkSpreader. Cursor insertion and native fastutil event order matter.
#[derive(Debug)]
pub struct SculkSpreader {
    pub settings: SpreadSettings,
    cursors: Vec<ChargeCursor>,
}
impl SculkSpreader {
    pub fn create_level_spreader() -> Self {
        Self::new(SpreadSettings {
            world_generation: false,
            growth_spawn_cost: 10,
            no_growth_radius: 4,
            charge_decay_rate: 10,
            additional_decay_rate: 5,
        })
    }
    pub fn create_world_gen_spreader() -> Self {
        Self::new(SpreadSettings {
            world_generation: true,
            growth_spawn_cost: 50,
            no_growth_radius: 1,
            charge_decay_rate: 5,
            additional_decay_rate: 10,
        })
    }
    fn new(settings: SpreadSettings) -> Self {
        Self {
            settings,
            cursors: Vec::new(),
        }
    }
    pub fn cursors(&self) -> &[ChargeCursor] {
        &self.cursors
    }
    pub fn clear(&mut self) {
        self.cursors.clear();
    }
    pub fn add_cursors(&mut self, pos: BlockPos, mut charge: i32) {
        while charge > 0 {
            let amount = charge.min(1000);
            if self.cursors.len() < 32 {
                self.cursors.push(ChargeCursor::new(pos, amount));
            }
            charge -= amount;
        }
    }
    pub fn update_cursors(
        &mut self,
        level: &mut LevelAccessRef<'_, '_>,
        origin: BlockPos,
        random: &mut FeatureRandomSelector<'_>,
        spread_veins: bool,
    ) -> Result<(), LevelError> {
        if self.cursors.is_empty() {
            return Ok(());
        }
        let mut processed = Vec::new();
        let mut mergeable = JavaHashMap::<BlockPos, usize>::new();
        let mut charges = Object2IntOpenHashMap::<BlockPos>::new();
        for index in 0..self.cursors.len() {
            let cursor = &mut self.cursors[index];
            if chessboard_distance(cursor.pos, origin) > 1024 {
                continue;
            }
            cursor.update(level, origin, random, self.settings, spread_veins)?;
            if cursor.charge <= 0 {
                level_event(level, cursor.pos, 0)?;
            } else {
                let pos = cursor.pos;
                let charge = cursor.charge;
                let total = charges.get_or_insert_with(pos, || 0);
                *total = total.wrapping_add(charge);
                if let Some(&existing) = mergeable.get(&pos) {
                    if !self.settings.world_generation
                        && charge.wrapping_add(self.cursors[existing].charge) <= 1000
                    {
                        self.cursors[existing].charge =
                            self.cursors[existing].charge.wrapping_add(charge);
                        self.cursors[existing].update_delay = self.cursors[existing]
                            .update_delay
                            .min(self.cursors[index].update_delay);
                        self.cursors[index].charge = 0;
                    } else {
                        processed.push(index);
                        if charge < self.cursors[existing].charge {
                            mergeable.insert(pos, index);
                        }
                    }
                } else {
                    mergeable.insert(pos, index);
                    processed.push(index);
                }
            }
        }
        for (pos, charge) in charges.iter() {
            if *charge > 0
                && let Some(mask) = mergeable
                    .get(pos)
                    .and_then(|index| self.cursors[*index].facings)
            {
                let particles = (f64::from(*charge).ln_1p() / 2.299999952316284) as i32 + 1;
                level_event(level, *pos, (particles << 6) + i32::from(mask))?;
            }
        }
        self.cursors = processed
            .into_iter()
            .map(|index| self.cursors[index].clone())
            .collect();
        Ok(())
    }
}

/// Concrete native SculkBehaviour dispatch. Only sculk and vein implement it;
/// catalyst, sensor and shrieker methods are independent framework virtuals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SculkBehaviour {
    Default,
    Sculk,
    Vein,
}
impl SculkBehaviour {
    pub fn of(state: BlockState) -> Self {
        match state.block() {
            Block::SCULK => Self::Sculk,
            Block::SCULK_VEIN => Self::Vein,
            _ => Self::Default,
        }
    }
    pub fn sculk_spread_delay(self) -> i32 {
        1
    }
    pub fn deposit_charge(self) -> bool {
        false
    }
    pub fn can_change_block_state_on_spread(self) -> bool {
        self != Self::Sculk
    }
    pub fn update_decay_delay(self, age: i32) -> i32 {
        if self == Self::Default {
            age.wrapping_sub(1).max(0)
        } else {
            1
        }
    }
    pub fn attempt_spread_vein(
        self,
        level: &mut LevelAccessRef<'_, '_>,
        pos: BlockPos,
        state: BlockState,
        facings: Option<u8>,
        post_process: bool,
    ) -> Result<bool, LevelError> {
        if self == Self::Default {
            match facings {
                None => {
                    let current = level.get_block_state(pos)?;
                    return Ok(SpreaderConfig::SculkVeinSameSpace.spread_all(
                        level,
                        current,
                        pos,
                        post_process,
                    )? > 0);
                }
                Some(mask) if mask != 0 => {
                    if !state.is_air()
                        && state.data().fluid.fluid_type
                            != ferraloom_data::registries::fluid::Id::WATER
                    {
                        return Ok(false);
                    }
                    return regrow(level, pos, state, mask);
                }
                _ => {}
            }
        }
        Ok(SpreaderConfig::SculkVein.spread_all(level, state, pos, post_process)? > 0)
    }
    pub fn on_discharged(
        self,
        level: &mut LevelAccessRef<'_, '_>,
        mut state: BlockState,
        pos: BlockPos,
    ) -> Result<(), LevelError> {
        if self == Self::Vein && state.block() == Block::SCULK_VEIN {
            for direction in Direction::VALUES {
                if multiface::has_face(state, direction)
                    && level.get_block_state(pos.relative(direction, 1))?.block() == Block::SCULK
                {
                    state = multiface::with_face(state, direction, false).unwrap();
                }
            }
            if face_mask(state) == 0 {
                state = if level.as_reader().get_fluid_state(pos)?.empty {
                    Block::AIR
                } else {
                    Block::WATER
                }
                .default_state();
            }
            level.set_block(pos, state, UpdateFlags::ALL)?;
        }
        Ok(())
    }
    pub fn attempt_use_charge(
        self,
        cursor: &ChargeCursor,
        level: &mut LevelAccessRef<'_, '_>,
        origin: BlockPos,
        random: &mut FeatureRandomSelector<'_>,
        settings: SpreadSettings,
        spread_veins: bool,
    ) -> Result<i32, LevelError> {
        let charge = cursor.charge;
        match self {
            Self::Default => Ok(if cursor.decay_delay > 0 { charge } else { 0 }),
            Self::Vein => {
                if spread_veins && attempt_place_sculk(level, cursor.pos, random, settings)? {
                    Ok(charge.wrapping_sub(1))
                } else {
                    Ok(
                        if next_int(level, random, settings.charge_decay_rate)? == 0 {
                            (charge as f32 * 0.5).floor() as i32
                        } else {
                            charge
                        },
                    )
                }
            }
            Self::Sculk => {
                if charge == 0 || next_int(level, random, settings.charge_decay_rate)? != 0 {
                    return Ok(charge);
                }
                let close = distance_squared(cursor.pos, origin)
                    < f64::from(settings.no_growth_radius).powi(2);
                if !close && can_place_growth(&level.as_reader(), cursor.pos)? {
                    let cost = settings.growth_spawn_cost;
                    if next_int(level, random, cost)? < charge {
                        let at = cursor.pos.above(1);
                        let mut growth = if next_int(level, random, 11)? == 0 {
                            Block::SCULK_SHRIEKER
                                .default_state()
                                .with(p::CAN_SUMMON, settings.world_generation)
                                .unwrap()
                        } else {
                            Block::SCULK_SENSOR.default_state()
                        };
                        if growth.get(p::WATERLOGGED).is_some()
                            && !level.as_reader().get_fluid_state(at)?.empty
                        {
                            growth = growth.with(p::WATERLOGGED, true).unwrap();
                        }
                        level.set_block(at, growth, UpdateFlags::ALL)?;
                        play_sound(level, cursor.pos, growth.data().sound.place_sound)?;
                    }
                    Ok(charge.wrapping_sub(cost).max(0))
                } else if next_int(level, random, settings.additional_decay_rate)? != 0 {
                    Ok(charge)
                } else {
                    Ok(charge.wrapping_sub(if close {
                        1
                    } else {
                        decay_penalty(settings, cursor.pos, origin, charge)
                    }))
                }
            }
        }
    }
}

fn is_sculk_behavior(state: BlockState) -> bool {
    matches!(state.block(), Block::SCULK | Block::SCULK_VEIN)
}
fn face_mask(state: BlockState) -> u8 {
    Direction::VALUES.into_iter().fold(0, |mask, direction| {
        mask | if multiface::has_face(state, direction) {
            1 << direction as u8
        } else {
            0
        }
    })
}
fn regrow(
    level: &mut LevelAccessRef<'_, '_>,
    pos: BlockPos,
    old: BlockState,
    faces: u8,
) -> Result<bool, LevelError> {
    let mut state = Block::SCULK_VEIN.default_state();
    let mut present = false;
    for direction in Direction::VALUES {
        if faces & (1 << direction as u8) != 0
            && multiface::can_attach_to(&level.as_reader(), pos, direction)?
        {
            state = multiface::with_face(state, direction, true).unwrap();
            present = true;
        }
    }
    if !present {
        return Ok(false);
    }
    if !old.data().fluid.empty {
        state = state.with(p::WATERLOGGED, true).unwrap();
    }
    level.set_block(pos, state, UpdateFlags::ALL)?;
    Ok(true)
}
fn attempt_place_sculk(
    level: &mut LevelAccessRef<'_, '_>,
    pos: BlockPos,
    random: &mut FeatureRandomSelector<'_>,
    settings: SpreadSettings,
) -> Result<bool, LevelError> {
    let state = level.get_block_state(pos)?;
    let mut directions = Direction::VALUES;
    shuffle(&mut directions, level, random)?;
    for support in directions {
        if multiface::has_face(state, support) {
            let at = pos.relative(support, 1);
            let old = level.get_block_state(at)?;
            if level
                .as_reader()
                .block_tags()?
                .contains_key(settings.replaceable_blocks(), old.block())
            {
                let sculk = Block::SCULK.default_state();
                level.set_block(at, sculk, UpdateFlags::ALL)?;
                match level {
                    LevelAccessRef::Server(live) => {
                        live.push_entities_up(old, sculk, at)?;
                    }
                    LevelAccessRef::Worldgen(_) => {
                        // WorldGenRegion#getEntities is empty. Native still queries
                        // both contextual shapes before the entity lookup.
                        let reader = level.as_reader();
                        let old_shape = reader.collision_shape_without_context(old, at)?;
                        let new_shape = reader.collision_shape_without_context(sculk, at)?;
                        let _ =
                            shapes::join_unoptimized(&old_shape, &new_shape, BooleanOp::OnlySecond);
                    }
                }
                play_spread_sound(level, at)?;
                SpreaderConfig::SculkVein.spread_all(
                    level,
                    sculk,
                    at,
                    settings.world_generation,
                )?;
                for direction in Direction::VALUES {
                    if direction != support.opposite() {
                        let vein_pos = at.relative(direction, 1);
                        let vein = level.get_block_state(vein_pos)?;
                        if vein.block() == Block::SCULK_VEIN {
                            SculkBehaviour::Vein.on_discharged(level, vein, vein_pos)?;
                        }
                    }
                }
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn can_place_growth(level: &LevelReaderRef<'_, '_>, pos: BlockPos) -> Result<bool, LevelError> {
    let above = level.get_block_state(pos.above(1))?;
    let can_grow_into = above.is_air()
        || above.block() == Block::WATER
            && above.data().fluid.fluid_type == ferraloom_data::registries::fluid::Id::WATER;
    if !can_grow_into {
        return Ok(false);
    }
    let (min_y, max_y) = match level {
        LevelReaderRef::Server(live) => (live.min_build_height(), live.max_build_height()),
        LevelReaderRef::PathNavigation(reader) => {
            (reader.min_build_height(), reader.max_build_height())
        }
        LevelReaderRef::Worldgen(region) => (region.min_y(), region.min_y() + region.height() - 1),
    };
    let from = pos.offset(-4, 0, -4);
    let to = pos.offset(4, 2, 4);
    let mut count = 0;
    for cx in (from.x >> 4)..=(to.x >> 4) {
        for cz in (from.z >> 4)..=(to.z >> 4) {
            let chunk_pos = ferraloom_util::ChunkPos::new(cx, cz);
            match level {
                LevelReaderRef::Server(live) => {
                    live.chunk(chunk_pos)
                        .ok_or(LevelError::MissingChunk(chunk_pos))?;
                }
                LevelReaderRef::PathNavigation(reader) => reader.validate_chunk(chunk_pos)?,
                LevelReaderRef::Worldgen(region) => {
                    region
                        .chunk([cx, cz], ferraloom_worldgen::region::ChunkStatus::Empty)
                        .map_err(|_| LevelError::MissingChunk(chunk_pos))?;
                }
            }
            for cy in (from.y.max(min_y) >> 4)..=(to.y.min(max_y) >> 4) {
                for y in from.y.max(cy << 4)..=to.y.min((cy << 4) + 15) {
                    for z in from.z.max(cz << 4)..=to.z.min((cz << 4) + 15) {
                        for x in from.x.max(cx << 4)..=to.x.min((cx << 4) + 15) {
                            if level.block_tags()?.contains_key(
                                BlockTags::SCULK_GROWTH_INHIBITORS,
                                level.get_block_state(BlockPos::new(x, y, z))?.block(),
                            ) {
                                count += 1;
                                if count > 2 {
                                    return Ok(false);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(true)
}
fn decay_penalty(settings: SpreadSettings, pos: BlockPos, origin: BlockPos, charge: i32) -> i32 {
    let distance = distance_squared(pos, origin).sqrt() as f32 - settings.no_growth_radius as f32;
    let outer = distance * distance;
    let reach = 24_i32.wrapping_sub(settings.no_growth_radius);
    let factor = ferraloom_util::java::min_f32(1.0, outer / reach.wrapping_mul(reach) as f32);
    ((charge as f32 * factor * 0.5) as i32).max(1)
}
fn distance_squared(a: BlockPos, b: BlockPos) -> f64 {
    let x = f64::from(a.x) - f64::from(b.x);
    let y = f64::from(a.y) - f64::from(b.y);
    let z = f64::from(a.z) - f64::from(b.z);
    x * x + y * y + z * z
}
fn chessboard_distance(a: BlockPos, b: BlockPos) -> i32 {
    a.x.wrapping_sub(b.x)
        .wrapping_abs()
        .max(a.y.wrapping_sub(b.y).wrapping_abs())
        .max(a.z.wrapping_sub(b.z).wrapping_abs())
}
fn movement_pos(
    level: &mut LevelAccessRef<'_, '_>,
    pos: BlockPos,
    origin: BlockPos,
    random: &mut FeatureRandomSelector<'_>,
    settings: SpreadSettings,
) -> Result<Option<BlockPos>, LevelError> {
    let mut offsets = Vec::with_capacity(18);
    for z in -1..=1 {
        for y in -1..=1 {
            for x in -1..=1 {
                if (x == 0 || y == 0 || z == 0) && (x != 0 || y != 0 || z != 0) {
                    offsets.push((x, y, z));
                }
            }
        }
    }
    shuffle(&mut offsets, level, random)?;
    let mut target = None;
    for (x, y, z) in offsets {
        let at = pos.offset(x, y, z);
        let dx = origin.x.wrapping_sub(at.x);
        let dz = origin.z.wrapping_sub(at.z);
        if settings.world_generation && dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)) > 144
        {
            continue;
        }
        let state = level.get_block_state(at)?;
        if is_sculk_behavior(state) && movement_unobstructed(&level.as_reader(), pos, at)? {
            target = Some(at);
            if has_substrate_access(&level.as_reader(), state, at)? {
                break;
            }
        }
    }
    Ok(target)
}
fn movement_unobstructed(
    level: &LevelReaderRef<'_, '_>,
    from: BlockPos,
    to: BlockPos,
) -> Result<bool, LevelError> {
    if from.dist_manhattan(to.into()) == 1 {
        return Ok(true);
    }
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let dz = to.z - from.z;
    let x = if dx < 0 {
        Direction::West
    } else {
        Direction::East
    };
    let y = if dy < 0 {
        Direction::Down
    } else {
        Direction::Up
    };
    let z = if dz < 0 {
        Direction::North
    } else {
        Direction::South
    };
    let (first, second) = if dx == 0 {
        (y, z)
    } else if dy == 0 {
        (x, z)
    } else {
        (x, y)
    };
    for face in [first, second] {
        let at = from.relative(face, 1);
        if !level.is_face_sturdy(level.get_block_state(at)?, at, face.opposite())? {
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn has_substrate_access(
    level: &LevelReaderRef<'_, '_>,
    state: BlockState,
    pos: BlockPos,
) -> Result<bool, LevelError> {
    if state.block() != Block::SCULK_VEIN {
        return Ok(false);
    }
    for direction in Direction::VALUES {
        if multiface::has_face(state, direction)
            && level.block_tags()?.contains_key(
                BlockTags::SCULK_REPLACEABLE,
                level.get_block_state(pos.relative(direction, 1))?.block(),
            )
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn next_int(
    level: &mut LevelAccessRef<'_, '_>,
    random: &mut FeatureRandomSelector<'_>,
    bound: i32,
) -> Result<i32, LevelError> {
    Ok(match random {
        FeatureRandomSelector::Canonical => level.random()?.next_int_bound(bound),
        FeatureRandomSelector::External(random) => random.next_int_bound(bound),
    })
}
fn shuffle<T>(
    values: &mut [T],
    level: &mut LevelAccessRef<'_, '_>,
    random: &mut FeatureRandomSelector<'_>,
) -> Result<(), LevelError> {
    for size in (2..=values.len()).rev() {
        values.swap(size - 1, next_int(level, random, size as i32)? as usize);
    }
    Ok(())
}
fn play_sound(
    level: &mut LevelAccessRef<'_, '_>,
    pos: BlockPos,
    sound: ferraloom_data::registries::sound_event::Id,
) -> Result<(), LevelError> {
    // WorldGenRegion sound and level-event methods are native no-ops.
    if let LevelAccessRef::Server(live) = level {
        live.play_sound(
            None,
            Vec3::at_center_of(pos.into()),
            sound,
            SoundSource::Blocks,
            1.0,
            1.0,
        )?;
    }
    Ok(())
}
fn play_spread_sound(level: &mut LevelAccessRef<'_, '_>, pos: BlockPos) -> Result<(), LevelError> {
    play_sound(
        level,
        pos,
        ferraloom_data::registries::sound_event::Id::BLOCK_SCULK_SPREAD,
    )
}
fn level_event(
    level: &mut LevelAccessRef<'_, '_>,
    pos: BlockPos,
    data: i32,
) -> Result<(), LevelError> {
    if let LevelAccessRef::Server(live) = level {
        live.level_event(None, 3006, pos, data)?;
    }
    Ok(())
}

/// Port of SculkPatchFeature#place after its canSpreadFrom gate.
#[allow(clippy::too_many_arguments)]
pub fn spread_patch(
    level: &mut LevelAccessRef<'_, '_>,
    origin: BlockPos,
    random: &mut FeatureRandomSelector<'_>,
    charge_count: i32,
    amount_per_charge: i32,
    spread_attempts: i32,
    growth_rounds: i32,
    spread_rounds: i32,
) -> Result<(), LevelError> {
    let mut spreader = SculkSpreader::create_world_gen_spreader();
    for round in 0..spread_rounds.wrapping_add(growth_rounds) {
        for _ in 0..charge_count {
            spreader.add_cursors(origin, amount_per_charge);
        }
        for _ in 0..spread_attempts {
            spreader.update_cursors(level, origin, random, round < spread_rounds)?;
        }
        spreader.clear();
    }
    Ok(())
}

#[cfg(test)]
mod tests;
