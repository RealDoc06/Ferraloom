//! Huge mushroom and fungus placement, ported from the 26.3 feature records.
use super::*;
use crate::placement::predicate::BlockPredicate;
use ferraloom_data::datapack::decode_block_state;

/// Port of AbstractHugeMushroomFeature and its red/brown cap implementations.
#[derive(Debug)]
pub(super) struct HugeMushroom {
    cap: Provider,
    stem: Provider,
    radius: i32,
    can_place_on: BlockPredicate,
    leaves: Vec<Block>,
    replaceable: Vec<Block>,
    red: bool,
}
impl HugeMushroom {
    pub fn decode(v: &Value, l: &mut FeatureLoader<'_>, red: bool) -> Result<Self, Error> {
        Ok(Self {
            cap: l.state_provider(field(v, "cap_provider")?)?,
            stem: l.state_provider(field(v, "stem_provider")?)?,
            radius: optional(v, "foliage_radius", 2, integer)?,
            can_place_on: BlockPredicate::decode(field(v, "can_place_on")?, &l.data)?,
            leaves: tag(l, "leaves")?,
            replaceable: tag(l, "replaceable_by_mushrooms")?,
            red,
        })
    }
    fn put(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        p: Pos,
        state: BlockState,
    ) -> Result<(), Error> {
        let old = ctx.level.block(p)?;
        if is_air(old) || self.replaceable.contains(&old.block()) {
            ctx.level.set_block(p, state, 3)?;
        }
        Ok(())
    }
}
impl Feature for HugeMushroom {
    fn huge_mushroom_foliage_radius(&self) -> Option<i32> {
        Some(self.radius)
    }
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let origin = ctx.origin;
        let mut height = ctx.random()?.next_int_bound(3) + 4;
        if ctx.random()?.next_int_bound(12) == 0 {
            height *= 2;
        }
        if origin[1] < ctx.level.min_y() + 1
            || origin[1] + height + 1 > ctx.level.min_y() + ctx.level.height() - 1
            || !self
                .can_place_on
                .test(&ctx.level, add(origin, [0, -1, 0]), ctx.generator)?
        {
            return Ok(false);
        }
        for y in 0..=height {
            // Vanilla passes (-1,-1) into the red mushroom radius calculation.
            let radius = if self.red || y <= 3 { 0 } else { self.radius };
            for x in -radius..=radius {
                for z in -radius..=radius {
                    let state = ctx.level.block(add(origin, [x, y, z]))?;
                    if !is_air(state) && !self.leaves.contains(&state.block()) {
                        return Ok(false);
                    }
                }
            }
        }
        if self.red {
            for y in height - 3..=height {
                let r = if y < height {
                    self.radius
                } else {
                    self.radius - 1
                };
                let center = self.radius - 2;
                for x in -r..=r {
                    for z in -r..=r {
                        let xe = x.abs() == r;
                        let ze = z.abs() == r;
                        if y >= height || xe != ze {
                            let mut state = self.cap.get_state(ctx, origin)?;
                            if has_sides(state) && state.get(properties::UP).is_some() {
                                state =
                                    sides(state, x < -center, x > center, z < -center, z > center);
                                state = state.with(properties::UP, y >= height - 1).unwrap();
                            }
                            self.put(ctx, add(origin, [x, y, z]), state)?;
                        }
                    }
                }
            }
        } else {
            let r = self.radius;
            for x in -r..=r {
                for z in -r..=r {
                    let xe = x.abs() == r;
                    let ze = z.abs() == r;
                    if !xe || !ze {
                        let mut state = self.cap.get_state(ctx, origin)?;
                        if has_sides(state) {
                            state = sides(
                                state,
                                x == -r || ze && x == 1 - r,
                                x == r || ze && x == r - 1,
                                z == -r || xe && z == 1 - r,
                                z == r || xe && z == r - 1,
                            );
                        }
                        self.put(ctx, add(origin, [x, height, z]), state)?;
                    }
                }
            }
        }
        for y in 0..height {
            let state = self.stem.get_state(ctx, origin)?;
            self.put(ctx, add(origin, [0, y, 0]), state)?;
        }
        Ok(true)
    }
}
pub(super) fn has_sides(s: BlockState) -> bool {
    // Global property tokens share a name across domains. Walls and redstone
    // also have these names, but lack the mushroom BooleanProperty domain.
    s.with(properties::WEST, properties::West::True).is_some()
        && s.with(properties::EAST, properties::East::True).is_some()
        && s.with(properties::NORTH, properties::North::True).is_some()
        && s.with(properties::SOUTH, properties::South::True).is_some()
}
fn sides(s: BlockState, w: bool, e: bool, n: bool, so: bool) -> BlockState {
    s.with(
        properties::WEST,
        if w {
            properties::West::True
        } else {
            properties::West::False
        },
    )
    .unwrap()
    .with(
        properties::EAST,
        if e {
            properties::East::True
        } else {
            properties::East::False
        },
    )
    .unwrap()
    .with(
        properties::NORTH,
        if n {
            properties::North::True
        } else {
            properties::North::False
        },
    )
    .unwrap()
    .with(
        properties::SOUTH,
        if so {
            properties::South::True
        } else {
            properties::South::False
        },
    )
    .unwrap()
}

/// Port of HugeFungusFeature, including its hat and weeping-vine random draws.
#[derive(Debug)]
pub(super) struct HugeFungus {
    base: BlockState,
    stem: BlockState,
    hat: BlockState,
    decor: BlockState,
    replaceable: BlockPredicate,
    planted: bool,
}
impl HugeFungus {
    pub fn decode(v: &Value, l: &mut FeatureLoader<'_>) -> Result<Self, Error> {
        Ok(Self {
            base: decode_block_state(field(v, "valid_base_block")?)?,
            stem: decode_block_state(field(v, "stem_state")?)?,
            hat: decode_block_state(field(v, "hat_state")?)?,
            decor: decode_block_state(field(v, "decor_state")?)?,
            replaceable: BlockPredicate::decode(field(v, "replaceable_blocks")?, &l.data)?,
            planted: optional(v, "planted", false, boolean)?,
        })
    }
    fn replaceable(&self, c: &FeatureContext<'_, '_>, p: Pos, plants: bool) -> Result<bool, Error> {
        Ok(c.level.block(p)?.data().replaceable
            || plants && self.replaceable.test(&c.level, p, c.generator)?)
    }
    fn hat_block(
        &self,
        c: &mut FeatureContext<'_, '_>,
        p: Pos,
        decor: f32,
        hat: f32,
        vines: f32,
    ) -> Result<(), Error> {
        if c.random()?.next_float() < decor {
            c.level.set_block(p, self.decor, 3)?;
        } else if c.random()?.next_float() < hat {
            c.level.set_block(p, self.hat, 3)?;
            if c.random()?.next_float() < vines {
                Self::vines(c, p)?;
            }
        }
        Ok(())
    }
    fn vines(c: &mut FeatureContext<'_, '_>, p: Pos) -> Result<(), Error> {
        let mut p = add(p, [0, -1, 0]);
        if !is_air(c.level.block(p)?) {
            return Ok(());
        }
        let mut height = c.random()?.next_int_bound(5) + 1;
        if c.random()?.next_int_bound(7) == 0 {
            height *= 2;
        }
        for y in 0..=height {
            if is_air(c.level.block(p)?) {
                if y == height || !is_air(c.level.block(add(p, [0, -1, 0]))?) {
                    let state = Block::WEEPING_VINES
                        .default_state()
                        .with_integer_property("age", c.random()?.next_int_bound(3) + 23)
                        .map_err(|e| Error(format!("{e:?}")))?;
                    c.level.set_block(p, state, 2)?;
                    break;
                }
                c.level
                    .set_block(p, Block::WEEPING_VINES_PLANT.default_state(), 2)?;
            }
            p[1] -= 1;
        }
        Ok(())
    }
}
impl Feature for HugeFungus {
    fn place(&self, c: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let origin = c.origin;
        if c.level.block(add(origin, [0, -1, 0]))?.block() != self.base.block() {
            return Ok(false);
        }
        let mut height = c.random()?.next_int_bound(10) + 4;
        if c.random()?.next_int_bound(12) == 0 {
            height *= 2;
        }
        if !self.planted && origin[1] + height + 1 >= c.generator.gen_depth() {
            return Ok(false);
        }
        let huge = !self.planted && c.random()?.next_float() < 0.06;
        c.level.set_block(origin, Block::AIR.default_state(), 260)?;
        let radius = if huge { 1i32 } else { 0 };
        for x in -radius..=radius {
            for z in -radius..=radius {
                let corner = huge && x.abs() == radius && z.abs() == radius;
                for y in 0..height {
                    let p = add(origin, [x, y, z]);
                    if self.replaceable(c, p, true)? {
                        if self.planted && !is_air(c.level.block(add(p, [0, -1, 0]))?) {
                            c.level.destroy_block(p, true)?;
                        }
                        if self.planted || !corner || c.random()?.next_float() < 0.1 {
                            c.level.set_block(p, self.stem, 3)?;
                        }
                    }
                }
            }
        }
        let vines = self.hat.block() == Block::NETHER_WART_BLOCK;
        let hat_height = (c.random()?.next_int_bound(1 + height / 3) + 5).min(height);
        let start = height - hat_height;
        for y in start..=height {
            let mut r = if y < height - c.random()?.next_int_bound(3) {
                2i32
            } else {
                1
            };
            if hat_height > 8 && y < start + 4 {
                r = 3;
            }
            if huge {
                r += 1;
            }
            for x in -r..=r {
                for z in -r..=r {
                    let xe = x.abs() == r;
                    let ze = z.abs() == r;
                    let inside = !xe && !ze && y != height;
                    let p = add(origin, [x, y, z]);
                    if self.replaceable(c, p, false)? {
                        if self.planted && !is_air(c.level.block(add(p, [0, -1, 0]))?) {
                            c.level.destroy_block(p, true)?;
                        }
                        if y < start + 3 {
                            if !inside {
                                if c.level.block(add(p, [0, -1, 0]))?.block() == self.hat.block() {
                                    c.level.set_block(p, self.hat, 3)?;
                                } else if f64::from(c.random()?.next_float()) < 0.15 {
                                    c.level.set_block(p, self.hat, 3)?;
                                    if vines && c.random()?.next_int_bound(11) == 0 {
                                        Self::vines(c, p)?;
                                    }
                                }
                            }
                        } else {
                            let (decor, hat, vine) = if inside {
                                (0.1, 0.2, 0.1)
                            } else if xe && ze {
                                (0.01, 0.7, 0.083)
                            } else {
                                (0.0005, 0.98, 0.07)
                            };
                            self.hat_block(c, p, decor, hat, if vines { vine } else { 0.0 })?;
                        }
                    }
                }
            }
        }
        Ok(true)
    }
}
