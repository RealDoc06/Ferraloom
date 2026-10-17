//! Vanilla cave features. Block callbacks use WorldgenBlockBehavior; geometric
//! scans and cached state facts remain in worldgen.
use super::{Feature, FeatureContext, FeatureLoader, state_provider::BlockStateProvider};
use crate::{
    Error,
    codec::*,
    generator::is_air,
    placement::{
        predicate::{BlockPredicate, move_direction, offset},
        providers::IntProvider,
    },
    region::{SavedTick, WorldGenRegion},
    structure::rule::BlockTags,
};
use ferraloom_data::{Block, BlockState, datapack::decode_block_state, registries::fluid};
use ferraloom_util::{Direction, random::RandomSource};
use serde_json::Value;
use std::sync::Arc;

mod geode;
mod lake;
mod monster_room;
mod speleothem;
mod spreading;
mod vegetation;

pub fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("replace_single_block".parse()?, |v, l| {
        Ok(Arc::new(ReplaceBlockFeature {
            targets: super::ore::replacements(v)?,
            tags: BlockTags(l.data.block_tags.clone()),
        }))
    })?;
    loader.register("disk".parse()?, |v, l| {
        Ok(Arc::new(DiskFeature {
            provider: l.state_provider(field(v, "state_provider")?)?,
            target: BlockPredicate::decode(field(v, "target")?, &l.data)?,
            radius: IntProvider::decode_bounded(field(v, "radius")?, 0, 8)?,
            half_height: int_range(field(v, "half_height")?, 0, 4)?,
        }))
    })?;
    loader.register("spring_feature".parse()?, |v, l| {
        let state = field(v, "state")?;
        let name = id(field(state, "id")?)?;
        let fluid = fluid::Id::from_name(&name.to_string())
            .ok_or_else(|| Error(format!("unknown fluid {name}")))?;
        let falling = state
            .get("properties")
            .and_then(|v| v.get("falling"))
            .and_then(Value::as_str)
            == Some("true");
        let level = match fluid {
            // FlowingFluid#getLegacyLevel ignores FALLING on source fluids.
            fluid::Id::WATER | fluid::Id::LAVA => 0,
            fluid::Id::FLOWING_WATER | fluid::Id::FLOWING_LAVA => {
                let amount = state
                    .get("properties")
                    .and_then(|v| v.get("level"))
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse::<i32>().ok())
                    .filter(|n| (1..=8).contains(n))
                    .unwrap_or(7);
                8 - amount + if falling { 8 } else { 0 }
            }
            _ => 0,
        };
        let block = if fluid == fluid::Id::EMPTY {
            Block::AIR
        } else if fluid == fluid::Id::WATER || fluid == fluid::Id::FLOWING_WATER {
            Block::WATER
        } else {
            Block::LAVA
        };
        let block_state =
            BlockState::from_properties(block, [("level", level.to_string().as_str())]).unwrap();
        Ok(Arc::new(SpringFeature {
            fluid,
            state: block_state,
            requires_below: optional(v, "requires_block_below", true, boolean)?,
            rock_count: optional(v, "rock_count", 4, integer)?,
            hole_count: optional(v, "hole_count", 1, integer)?,
            valid: l.data.blocks(field(v, "valid_blocks")?)?,
        }))
    })?;
    loader.register("underwater_magma".parse()?, |v, _| {
        Ok(Arc::new(UnderwaterMagmaFeature {
            range: int_range(field(v, "floor_search_range")?, 0, 512)?,
            radius: int_range(field(v, "placement_radius_around_floor")?, 0, 64)?,
            probability: bounded(
                field(v, "placement_probability_per_valid_position")?,
                0.0,
                1.0,
            )? as f32,
        }))
    })?;
    speleothem::register(loader)?;
    geode::register(loader)?;
    lake::register(loader)?;
    spreading::register(loader)?;
    vegetation::register(loader)?;
    monster_room::register(loader)?;
    Ok(())
}

/// Port of Feature#markAboveForPostProcessing.
fn mark_above(region: &mut WorldGenRegion<'_>, mut p: [i32; 3]) -> Result<(), Error> {
    for _ in 0..2 {
        p = move_direction(p, Direction::Up);
        if is_air(region.block(p)?) {
            break;
        }
        region.mark_post_processing(p)?;
    }
    Ok(())
}
fn empty_or_water(state: BlockState) -> bool {
    is_air(state) || state.block() == Block::WATER
}
fn empty_or_liquid(state: BlockState) -> bool {
    empty_or_water(state) || state.block() == Block::LAVA
}
const HORIZONTAL: [Direction; 4] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
];

/// Port of Column's optional floor and ceiling, including unbounded rays.
struct Column {
    floor: Option<i32>,
    ceiling: Option<i32>,
}

/// Port of Column#scan. A missing column and a column with no edges are distinct.
fn scan(
    region: &WorldGenRegion<'_>,
    origin: [i32; 3],
    range: i32,
    inside: impl Fn(BlockState) -> bool,
    edge: impl Fn(BlockState) -> bool,
) -> Result<Option<Column>, Error> {
    if !inside(region.block(origin)?) {
        return Ok(None);
    }
    let mut ends = [None, None];
    for (i, direction) in [Direction::Down, Direction::Up].into_iter().enumerate() {
        let mut p = origin;
        let mut n = 1;
        while n < range && inside(region.block(p)?) {
            p = move_direction(p, direction);
            n += 1;
        }
        if edge(region.block(p)?) {
            ends[i] = Some(p[1]);
        }
    }
    if matches!((ends[0], ends[1]), (Some(floor), Some(ceiling)) if ceiling - floor - 1 < 0) {
        return Err(Error("Column of negative height".into()));
    }
    Ok(Some(Column {
        floor: ends[0],
        ceiling: ends[1],
    }))
}

/// Port of ReplaceBlockFeature.
#[derive(Debug)]
struct ReplaceBlockFeature {
    targets: Vec<super::ore::BlockReplacement>,
    tags: BlockTags,
}
impl Feature for ReplaceBlockFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        for target in &self.targets {
            if target.target.test(
                ctx.region.block(ctx.origin)?,
                ctx.origin,
                &self.tags,
                ctx.random,
            ) {
                ctx.region.set_block(ctx.origin, target.state, 2)?;
                break;
            }
        }
        Ok(true)
    }
}
/// Port of DiskFeature.
#[derive(Debug)]
struct DiskFeature {
    provider: Arc<BlockStateProvider>,
    target: BlockPredicate,
    radius: IntProvider,
    half_height: i32,
}
impl Feature for DiskFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let radius = self.radius.sample(ctx.random);
        let mut placed = false;
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dz * dz > radius * radius {
                    continue;
                }
                let mut above = false;
                for y in (ctx.origin[1] - self.half_height..=ctx.origin[1] + self.half_height).rev()
                {
                    let p = [ctx.origin[0] + dx, y, ctx.origin[2] + dz];
                    if self.target.test(ctx.region, p, ctx.generator)? {
                        if let Some(state) = self.provider.get_optional_state(ctx, p)? {
                            ctx.region.set_block(p, state, 2)?;
                            if !above {
                                mark_above(ctx.region, p)?;
                            }
                            placed = true;
                            above = true;
                        }
                    } else {
                        above = false;
                    }
                }
            }
        }
        Ok(placed)
    }
}
/// Port of SpringFeature, including ProtoChunkTicks' fluid identity.
#[derive(Debug)]
struct SpringFeature {
    fluid: fluid::Id,
    state: BlockState,
    requires_below: bool,
    rock_count: i32,
    hole_count: i32,
    valid: Vec<Block>,
}
impl Feature for SpringFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let p = ctx.origin;
        let valid = |p| ctx.region.block(p).map(|s| self.valid.contains(&s.block()));
        if !valid(move_direction(p, Direction::Up))?
            || self.requires_below && !valid(move_direction(p, Direction::Down))?
        {
            return Ok(false);
        }
        let current = ctx.region.block(p)?;
        if !is_air(current) && !self.valid.contains(&current.block()) {
            return Ok(false);
        }
        let directions = [
            Direction::West,
            Direction::East,
            Direction::North,
            Direction::South,
            Direction::Down,
        ];
        let mut rocks = 0;
        let mut holes = 0;
        for d in directions {
            if valid(move_direction(p, d))? {
                rocks += 1;
            }
        }
        for d in directions {
            if is_air(ctx.region.block(move_direction(p, d))?) {
                holes += 1;
            }
        }
        if rocks != self.rock_count || holes != self.hole_count {
            return Ok(false);
        }
        ctx.region.set_block(p, self.state, 2)?;
        ctx.region.schedule_fluid_tick(SavedTick {
            position: p,
            kind: self.fluid,
            priority: 0,
        })?;
        Ok(true)
    }
}
/// Port of UnderwaterMagmaFeature, with state-cached occlusion (not sturdy faces).
#[derive(Debug)]
struct UnderwaterMagmaFeature {
    range: i32,
    radius: i32,
    probability: f32,
}
impl Feature for UnderwaterMagmaFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let Some(Column { floor: Some(y), .. }) = scan(
            ctx.region,
            ctx.origin,
            self.range,
            |s| s.block() == Block::WATER,
            |s| s.block() != Block::WATER,
        )?
        else {
            return Ok(false);
        };
        let mut placed = false;
        for dz in -self.radius..=self.radius {
            for dy in -self.radius..=self.radius {
                for dx in -self.radius..=self.radius {
                    if ctx.random.next_float() >= self.probability {
                        continue;
                    }
                    let p = [ctx.origin[0] + dx, y + dy, ctx.origin[2] + dz];
                    if empty_or_water(ctx.region.block(p)?)
                        || !ctx
                            .region
                            .block(move_direction(p, Direction::Down))?
                            .is_face_occlusion_full(Direction::Up)
                    {
                        continue;
                    }
                    let mut enclosed = true;
                    for d in HORIZONTAL {
                        if !ctx
                            .region
                            .block(move_direction(p, d))?
                            .is_face_occlusion_full(d.opposite())
                        {
                            enclosed = false;
                            break;
                        }
                    }
                    if enclosed {
                        ctx.region
                            .set_block(p, Block::MAGMA_BLOCK.default_state(), 2)?;
                        placed = true;
                    }
                }
            }
        }
        Ok(placed)
    }
}

#[cfg(test)]
mod tests;
