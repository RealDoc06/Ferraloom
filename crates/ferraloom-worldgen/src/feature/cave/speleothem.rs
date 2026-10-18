use super::*;
use crate::placement::providers::FloatProvider;
use ferraloom_data::properties::{self, Thickness, VerticalDirection};
use ferraloom_util::mth;

/// Port of SpeleothemUtils' state-only operations. The supplied states configure
/// block types; vanilla deliberately starts growth from each type's default.
#[derive(Debug)]
struct SpeleothemBlocks {
    base: Block,
    pointed: Block,
    replaceable: Vec<Block>,
    water: Vec<fluid::Id>,
}
impl SpeleothemBlocks {
    fn decode(v: &Value, l: &FeatureLoader<'_>) -> Result<Self, Error> {
        Ok(Self {
            base: decode_block_state(field(v, "base_block")?)?.block(),
            pointed: decode_block_state(field(v, "pointed_block")?)?.block(),
            replaceable: l.data.blocks(field(v, "replaceable_blocks")?)?,
            water: l.data.fluids(&Value::String("#minecraft:water".into()))?,
        })
    }
    fn is_base(&self, state: BlockState) -> bool {
        state.block() == self.base || self.replaceable.contains(&state.block())
    }
    fn place_base(&self, ctx: &mut FeatureContext<'_, '_>, p: [i32; 3]) -> Result<bool, Error> {
        if !self.replaceable.contains(&ctx.level.block(p)?.block()) {
            return Ok(false);
        }
        ctx.level.set_block(p, self.base.default_state(), 2)?;
        Ok(true)
    }
    fn grow(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        mut p: [i32; 3],
        direction: Direction,
        height: i32,
        merged: bool,
    ) -> Result<(), Error> {
        if !self.is_base(ctx.level.block(move_direction(p, direction.opposite()))?) {
            return Ok(());
        }
        for i in 0..height {
            let thickness = if i == height - 1 {
                if merged {
                    Thickness::TipMerge
                } else {
                    Thickness::Tip
                }
            } else if i == height - 2 {
                Thickness::Frustum
            } else if i == 0 {
                Thickness::Base
            } else {
                Thickness::Middle
            };
            let direction = if direction == Direction::Up {
                VerticalDirection::Up
            } else {
                VerticalDirection::Down
            };
            let mut state = self
                .pointed
                .default_state()
                .with(properties::VERTICAL_DIRECTION, direction)
                .ok_or_else(|| Error("pointed block lacks vertical_direction".into()))?
                .with(properties::THICKNESS, thickness)
                .ok_or_else(|| Error("pointed block lacks thickness".into()))?;
            // 26.3 applies waterlogging to pointed_block, including sulfur spikes.
            let fluid = ctx.level.fluid(p)?;
            state = state
                .with(
                    properties::WATERLOGGED,
                    self.water.contains(&fluid.fluid_type),
                )
                .ok_or_else(|| Error("pointed block lacks waterlogged".into()))?;
            ctx.level.set_block(p, state, 2)?;
            p[1] += if direction == VerticalDirection::Up {
                1
            } else {
                -1
            };
        }
        Ok(())
    }
}
/// Port of SpeleothemFeature.
#[derive(Debug)]
struct SpeleothemFeature {
    blocks: SpeleothemBlocks,
    taller: f32,
    spread: f32,
    radius2: f32,
    radius3: f32,
}
pub(super) fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("speleothem".parse()?, |v, l| {
        Ok(Arc::new(SpeleothemFeature {
            blocks: SpeleothemBlocks::decode(v, l)?,
            taller: optional(v, "chance_of_taller_generation", 0.2, |v| {
                float_range(v, 0.0, 1.0)
            })?,
            spread: optional(v, "chance_of_directional_spread", 0.7, |v| {
                float_range(v, 0.0, 1.0)
            })?,
            radius2: optional(v, "chance_of_spread_radius2", 0.5, |v| {
                float_range(v, 0.0, 1.0)
            })?,
            radius3: optional(v, "chance_of_spread_radius3", 0.5, |v| {
                float_range(v, 0.0, 1.0)
            })?,
        }))
    })?;
    loader.register("speleothem_cluster".parse()?, |v, l| {
        Ok(Arc::new(SpeleothemClusterFeature {
            blocks: SpeleothemBlocks::decode(v, l)?,
            stone: l
                .data
                .blocks(&Value::String("#minecraft:base_stone_overworld".into()))?,
            range: int_range(field(v, "floor_to_ceiling_search_range")?, 1, 512)?,
            height: IntProvider::decode_bounded(field(v, "height")?, 1, 128)?,
            radius: IntProvider::decode_bounded(field(v, "radius")?, 1, 128)?,
            difference: int_range(field(v, "max_stalagmite_stalactite_height_diff")?, 0, 64)?,
            deviation: int_range(field(v, "height_deviation")?, 1, 64)?,
            thickness: IntProvider::decode_bounded(
                field(v, "speleothem_block_layer_thickness")?,
                0,
                128,
            )?,
            density: FloatProvider::decode_bounded(field(v, "density")?, 0.0, 2.0)?,
            wetness: FloatProvider::decode_bounded(field(v, "wetness")?, 0.0, 2.0)?,
            edge_chance: float_range(
                field(v, "chance_of_speleothem_at_max_distance_from_center")?,
                0.0,
                1.0,
            )?,
            edge_distance: int_range(
                field(v, "max_distance_from_edge_affecting_chance_of_speleothem")?,
                1,
                64,
            )?,
            height_bias: int_range(
                field(v, "max_distance_from_center_affecting_height_bias")?,
                1,
                64,
            )?,
        }))
    })?;
    loader.register("large_dripstone".parse()?, |v, l| {
        Ok(Arc::new(LargeDripstoneFeature {
            replaceable: l.data.blocks(field(v, "replaceable_blocks")?)?,
            stone: l
                .data
                .blocks(&Value::String("#minecraft:base_stone_overworld".into()))?,
            range: optional(v, "floor_to_ceiling_search_range", 30, |v| {
                int_range(v, 1, 512)
            })?,
            radius: IntProvider::decode_bounded(field(v, "column_radius")?, 1, 16)?,
            scale: FloatProvider::decode_bounded(field(v, "height_scale")?, 0.0, 20.0)?,
            ratio: float_range(
                field(v, "max_column_radius_to_cave_height_ratio")?,
                0.1,
                1.0,
            )?,
            stalactite: FloatProvider::decode_bounded(
                field(v, "stalactite_bluntness")?,
                0.1,
                10.0,
            )?,
            stalagmite: FloatProvider::decode_bounded(
                field(v, "stalagmite_bluntness")?,
                0.1,
                10.0,
            )?,
            wind: FloatProvider::decode_bounded(field(v, "wind_speed")?, 0.0, 2.0)?,
            wind_radius: int_range(field(v, "min_radius_for_wind")?, 0, 100)?,
            wind_bluntness: float_range(field(v, "min_bluntness_for_wind")?, 0.0, 5.0)?,
        }))
    })
}
impl Feature for SpeleothemFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let above = self
            .blocks
            .is_base(ctx.level.block(move_direction(ctx.origin, Direction::Up))?);
        let below = self.blocks.is_base(
            ctx.level
                .block(move_direction(ctx.origin, Direction::Down))?,
        );
        let direction = match (above, below) {
            (true, true) => {
                if ctx.random()?.next_boolean() {
                    Direction::Down
                } else {
                    Direction::Up
                }
            }
            (true, false) => Direction::Down,
            (false, true) => Direction::Up,
            (false, false) => return Ok(false),
        };
        let root = move_direction(ctx.origin, direction.opposite());
        self.blocks.place_base(ctx, root)?;
        for d in HORIZONTAL {
            if ctx.random()?.next_float() > self.spread {
                continue;
            }
            let p = move_direction(root, d);
            self.blocks.place_base(ctx, p)?;
            if ctx.random()?.next_float() > self.radius2 {
                continue;
            }
            let d = Direction::VALUES[ctx.random()?.next_int_bound(6) as usize];
            let p = move_direction(p, d);
            self.blocks.place_base(ctx, p)?;
            if ctx.random()?.next_float() > self.radius3 {
                continue;
            }
            let d = Direction::VALUES[ctx.random()?.next_int_bound(6) as usize];
            self.blocks.place_base(ctx, move_direction(p, d))?;
        }
        let height = if ctx.random()?.next_float() < self.taller
            && empty_or_water(ctx.level.block(move_direction(ctx.origin, direction))?)
        {
            2
        } else {
            1
        };
        self.blocks
            .grow(ctx, ctx.origin, direction, height, false)?;
        Ok(true)
    }
}
/// Port of SpeleothemClusterFeature.
#[derive(Debug)]
struct SpeleothemClusterFeature {
    blocks: SpeleothemBlocks,
    stone: Vec<Block>,
    range: i32,
    height: IntProvider,
    radius: IntProvider,
    difference: i32,
    deviation: i32,
    thickness: IntProvider,
    density: FloatProvider,
    wetness: FloatProvider,
    edge_chance: f32,
    edge_distance: i32,
    height_bias: i32,
}
impl SpeleothemClusterFeature {
    fn can_pool(&self, ctx: &FeatureContext<'_, '_>, p: [i32; 3]) -> Result<bool, Error> {
        let state = ctx.level.block(p)?;
        if [Block::WATER, self.blocks.base, self.blocks.pointed].contains(&state.block()) {
            return Ok(false);
        }
        let water = |s: BlockState| self.blocks.water.contains(&s.data().fluid.fluid_type);
        if water(ctx.level.block(move_direction(p, Direction::Up))?) {
            return Ok(false);
        }
        for d in HORIZONTAL.into_iter().chain([Direction::Down]) {
            let state = ctx.level.block(move_direction(p, d))?;
            if !self.stone.contains(&state.block()) && !water(state) {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn replace_base(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        mut p: [i32; 3],
        count: i32,
        d: Direction,
    ) -> Result<(), Error> {
        for _ in 0..count {
            if !self.blocks.place_base(ctx, p)? {
                break;
            }
            p = move_direction(p, d);
        }
        Ok(())
    }
    fn height(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        dx: i32,
        dz: i32,
        density: f32,
        max: i32,
    ) -> i32 {
        if ctx.random()?.next_float() > density {
            return 0;
        }
        let distance = (dx.abs() + dz.abs()) as f64;
        let mean = mth::clamped_map(
            distance,
            0.0,
            self.height_bias as f64,
            max as f64 / 2.0,
            0.0,
        ) as f32;
        (mean + ctx.random()?.next_gaussian() as f32 * self.deviation as f32).clamp(0.0, max as f32)
            as i32
    }
    #[allow(clippy::too_many_arguments)]
    fn column(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        p: [i32; 3],
        dx: i32,
        dz: i32,
        wetness: f32,
        chance: f64,
        height: i32,
        density: f32,
    ) -> Result<(), Error> {
        let Some(Column { mut floor, ceiling }) =
            scan(&ctx.level, p, self.range, empty_or_water, |s| {
                !empty_or_water(s)
            })?
        else {
            return Ok(());
        };
        if floor.is_none() && ceiling.is_none() {
            return Ok(());
        }
        let want_pool = ctx.random()?.next_float() < wetness;
        if want_pool
            && let Some(y) = floor
            && self.can_pool(ctx, [p[0], y, p[2]])?
        {
            ctx.level
                .set_block([p[0], y, p[2]], Block::WATER.default_state(), 2)?;
            floor = Some(y - 1);
        }
        let want = ctx.random()?.next_double() < chance;
        let mut down = 0;
        if let Some(y) = ceiling
            && want
            && ctx.level.block([p[0], y, p[2]])?.block() != Block::LAVA
        {
            let n = self.thickness.sample(&mut ctx.random()?);
            self.replace_base(ctx, [p[0], y, p[2]], n, Direction::Up)?;
            let max = floor.map_or(height, |f| height.min(y - f));
            down = self.height(ctx, dx, dz, density, max);
        }
        let want = ctx.random()?.next_double() < chance;
        let mut up = 0;
        if let Some(y) = floor
            && want
            && ctx.level.block([p[0], y, p[2]])?.block() != Block::LAVA
        {
            let n = self.thickness.sample(&mut ctx.random()?);
            self.replace_base(ctx, [p[0], y, p[2]], n, Direction::Down)?;
            up = if ceiling.is_some() {
                (down
                    + mth::random_between_inclusive(
                        &mut ctx.random()?,
                        -self.difference,
                        self.difference,
                    ))
                .max(0)
            } else {
                self.height(ctx, dx, dz, density, height)
            };
        }
        if let (Some(f), Some(c)) = (floor, ceiling)
            && c - down <= f + up
        {
            let bottom = mth::random_between_inclusive(
                &mut ctx.random()?,
                (c - down).max(f + 1),
                (f + up).min(c - 1) + 1,
            );
            down = c - bottom;
            up = bottom - 1 - f;
        }
        let merge = ctx.random()?.next_boolean()
            && down > 0
            && up > 0
            && matches!((floor,ceiling),(Some(f),Some(c)) if down+up==c-f-1);
        if let Some(c) = ceiling {
            self.blocks
                .grow(ctx, [p[0], c - 1, p[2]], Direction::Down, down, merge)?;
        }
        if let Some(f) = floor {
            self.blocks
                .grow(ctx, [p[0], f + 1, p[2]], Direction::Up, up, merge)?;
        }
        Ok(())
    }
}
impl Feature for SpeleothemClusterFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        if !empty_or_water(ctx.level.block(ctx.origin)?) {
            return Ok(false);
        }
        let height = self.height.sample(&mut ctx.random()?);
        let wetness = self.wetness.sample(&mut ctx.random()?);
        let density = self.density.sample(&mut ctx.random()?);
        let xr = self.radius.sample(&mut ctx.random()?);
        let zr = self.radius.sample(&mut ctx.random()?);
        for dx in -xr..=xr {
            for dz in -zr..=zr {
                let distance = (xr - dx.abs()).min(zr - dz.abs());
                let chance = mth::clamped_map_f32(
                    distance as f32,
                    0.0,
                    self.edge_distance as f32,
                    self.edge_chance,
                    1.0,
                ) as f64;
                self.column(
                    ctx,
                    offset(ctx.origin, [dx, 0, dz]),
                    dx,
                    dz,
                    wetness,
                    chance,
                    height,
                    density,
                )?;
            }
        }
        Ok(true)
    }
}

/// Port of LargeDripstoneFeature. Wind and radius shrinking are geometric and
/// do not substitute for block behavior.
#[derive(Debug)]
struct LargeDripstoneFeature {
    replaceable: Vec<Block>,
    stone: Vec<Block>,
    range: i32,
    radius: IntProvider,
    scale: FloatProvider,
    ratio: f32,
    stalactite: FloatProvider,
    stalagmite: FloatProvider,
    wind: FloatProvider,
    wind_radius: i32,
    wind_bluntness: f32,
}
struct LargeDripstone {
    root: [i32; 3],
    up: bool,
    radius: i32,
    bluntness: f64,
    scale: f64,
}
struct Wind {
    y: i32,
    speed: Option<[f64; 2]>,
    max: i32,
}
impl Wind {
    fn offset(&self, p: [i32; 3]) -> [i32; 3] {
        if let Some([x, z]) = self.speed {
            let dy = (self.y - p[1]) as f64;
            offset(
                p,
                [
                    mth::floor(x * dy).clamp(-self.max, self.max),
                    0,
                    mth::floor(z * dy).clamp(-self.max, self.max),
                ],
            )
        } else {
            p
        }
    }
}
impl LargeDripstone {
    fn height(&self, mut distance: f64) -> i32 {
        distance = distance.max(self.bluntness);
        let r = distance / self.radius as f64 * 0.384;
        let a = 0.75 * r.powf(1.3333333333333333);
        let b = r.powf(0.6666666666666666);
        let c = 0.3333333333333333 * r.ln();
        ((self.scale * (a - b - c)).max(0.0) / 0.384 * self.radius as f64) as i32
    }
    fn embedded(&mut self, ctx: &FeatureContext<'_, '_>, wind: &Wind) -> Result<bool, Error> {
        while self.radius > 1 {
            let mut root = self.root;
            for _ in 0..10.min(self.height(0.0)) {
                if ctx.level.block(root)?.block() == Block::LAVA {
                    return Ok(false);
                }
                let center = wind.offset(root);
                let mut embedded = !empty_or_liquid(ctx.level.block(center)?);
                if embedded {
                    let mut angle = 0.0f32;
                    while angle < std::f32::consts::TAU {
                        let dx = (mth::cos(angle as f64) * self.radius as f32) as i32;
                        let dz = (mth::sin(angle as f64) * self.radius as f32) as i32;
                        if empty_or_liquid(ctx.level.block(offset(center, [dx, 0, dz]))?) {
                            embedded = false;
                            break;
                        }
                        angle += 6.0 / self.radius as f32;
                    }
                }
                if embedded {
                    self.root = root;
                    return Ok(true);
                }
                root[1] += if self.up { -1 } else { 1 };
            }
            self.radius /= 2;
        }
        Ok(false)
    }
    fn place(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        wind: &Wind,
        stone: &[Block],
    ) -> Result<(), Error> {
        for dx in -self.radius..=self.radius {
            for dz in -self.radius..=self.radius {
                let r = ((dx * dx + dz * dz) as f32).sqrt();
                if r > self.radius as f32 {
                    continue;
                }
                let mut height = self.height(r as f64);
                if height <= 0 {
                    continue;
                }
                if (ctx.random()?.next_float() as f64) < 0.2 {
                    height =
                        (height as f32 * mth::random_between(&mut ctx.random()?, 0.8, 1.0)) as i32;
                }
                let mut p = offset(self.root, [dx, 0, dz]);
                let mut outside = false;
                let max_y = if self.up {
                    ctx.level
                        .height_at(crate::region::Heightmap::WorldSurfaceWg, p[0], p[2])?
                } else {
                    i32::MAX
                };
                for _ in 0..height {
                    if p[1] >= max_y {
                        break;
                    }
                    let q = wind.offset(p);
                    let old = ctx.level.block(q)?;
                    if empty_or_liquid(old) {
                        outside = true;
                        ctx.level
                            .set_block(q, Block::DRIPSTONE_BLOCK.default_state(), 2)?;
                    } else if outside && stone.contains(&old.block()) {
                        break;
                    }
                    p[1] += if self.up { 1 } else { -1 };
                }
            }
        }
        Ok(())
    }
}
impl Feature for LargeDripstoneFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let Some(Column {
            floor: Some(floor),
            ceiling: Some(ceiling),
        }) = scan(&ctx.level, ctx.origin, self.range, empty_or_water, |s| {
            s.block() == Block::DRIPSTONE_BLOCK
                || s.block() == Block::LAVA
                || self.replaceable.contains(&s.block())
        })?
        else {
            return Ok(false);
        };
        let height = ceiling - floor - 1;
        if height < 4 {
            return Ok(false);
        }
        let (min, max) = self.radius.range();
        let max = ((height as f32 * self.ratio) as i32).clamp(min, max);
        let radius = mth::random_between_inclusive(&mut ctx.random()?, min, max);
        let mut down = LargeDripstone {
            root: [ctx.origin[0], ceiling - 1, ctx.origin[2]],
            up: false,
            radius,
            bluntness: self.stalactite.sample(&mut ctx.random()?) as f64,
            scale: self.scale.sample(&mut ctx.random()?) as f64,
        };
        let mut up = LargeDripstone {
            root: [ctx.origin[0], floor + 1, ctx.origin[2]],
            up: true,
            radius,
            bluntness: self.stalagmite.sample(&mut ctx.random()?) as f64,
            scale: self.scale.sample(&mut ctx.random()?) as f64,
        };
        let wind = if radius >= self.wind_radius
            && down.bluntness >= self.wind_bluntness as f64
            && up.bluntness >= self.wind_bluntness as f64
        {
            let speed = self.wind.sample(&mut ctx.random()?);
            let d = mth::random_between(&mut ctx.random()?, 0.0, std::f32::consts::PI);
            Wind {
                y: ctx.origin[1],
                speed: Some([
                    (mth::cos(d as f64) * speed) as f64,
                    (mth::sin(d as f64) * speed) as f64,
                ]),
                max: 16 - radius,
            }
        } else {
            Wind {
                y: 0,
                speed: None,
                max: 0,
            }
        };
        let place_down = down.embedded(ctx, &wind)?;
        let place_up = up.embedded(ctx, &wind)?;
        if place_down {
            down.place(ctx, &wind, &self.stone)?;
        }
        if place_up {
            up.place(ctx, &wind, &self.stone)?;
        }
        Ok(true)
    }
}
