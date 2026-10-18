use super::trunk::Attachment;
use super::*;
use ferraloom_util::random::WorldgenRandom;
/// Port of all twelve registered 26.3 FoliagePlacer implementations.
#[derive(Debug)]
pub(super) struct FoliagePlacer {
    radius: IntProvider,
    offset: IntProvider,
    kind: Kind,
}
#[derive(Debug)]
enum Kind {
    Blob(i32),
    Bush(i32),
    Fancy(i32),
    Jungle(i32),
    Acacia,
    DarkOak,
    Pine(IntProvider),
    Spruce(IntProvider),
    MegaPine(IntProvider),
    Spread {
        height: IntProvider,
        attempts: i32,
    },
    Cherry {
        height: IntProvider,
        wide: f32,
        corner: f32,
        hanging: f32,
        extension: f32,
    },
    Poplar {
        height: IntProvider,
        hole: f32,
    },
}
impl FoliagePlacer {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let ty = kind(v)?;
        Ok(Self {
            radius: provider_int(v, "radius", 0, 16)?,
            offset: provider_int(v, "offset", 0, 16)?,
            kind: match ty.to_string().as_str() {
                "minecraft:blob_foliage_placer" => Kind::Blob(int(v, "height", 0, 16)?),
                "minecraft:bush_foliage_placer" => Kind::Bush(int(v, "height", 0, 16)?),
                "minecraft:fancy_foliage_placer" => Kind::Fancy(int(v, "height", 0, 16)?),
                "minecraft:jungle_foliage_placer" => Kind::Jungle(int(v, "height", 0, 16)?),
                "minecraft:acacia_foliage_placer" => Kind::Acacia,
                "minecraft:dark_oak_foliage_placer" => Kind::DarkOak,
                "minecraft:pine_foliage_placer" => Kind::Pine(provider_int(v, "height", 0, 24)?),
                "minecraft:spruce_foliage_placer" => {
                    Kind::Spruce(provider_int(v, "trunk_height", 0, 24)?)
                }
                "minecraft:mega_pine_foliage_placer" => {
                    Kind::MegaPine(provider_int(v, "crown_height", 0, 24)?)
                }
                "minecraft:random_spread_foliage_placer" => Kind::Spread {
                    height: provider_int(v, "foliage_height", 1, 512)?,
                    attempts: int(v, "leaf_placement_attempts", 0, 256)?,
                },
                "minecraft:cherry_foliage_placer" => Kind::Cherry {
                    height: provider_int(v, "height", 4, 16)?,
                    wide: chance(v, "wide_bottom_layer_hole_chance")?,
                    corner: chance(v, "corner_hole_chance")?,
                    hanging: chance(v, "hanging_leaves_chance")?,
                    extension: chance(v, "hanging_leaves_extension_chance")?,
                },
                "minecraft:poplar_foliage_placer" => Kind::Poplar {
                    height: provider_int(v, "height", 5, 16)?,
                    hole: chance(v, "side_hole_chance")?,
                },
                _ => return Err(Error(format!("unknown foliage placer {ty}"))),
            },
        })
    }
    pub fn height(&self, r: &mut WorldgenRandom, tree_height: i32) -> i32 {
        match &self.kind {
            Kind::Blob(h) | Kind::Bush(h) | Kind::Fancy(h) | Kind::Jungle(h) => *h,
            Kind::Acacia => 0,
            Kind::DarkOak => 4,
            Kind::Spruce(h) => (tree_height - h.sample(r)).max(4),
            Kind::Pine(h)
            | Kind::MegaPine(h)
            | Kind::Spread { height: h, .. }
            | Kind::Cherry { height: h, .. }
            | Kind::Poplar { height: h, .. } => h.sample(r),
        }
    }
    pub fn radius(&self, r: &mut WorldgenRandom, trunk: i32) -> i32 {
        let radius = self.radius.sample(r);
        radius
            + if matches!(self.kind, Kind::Pine(_)) {
                r.next_int_bound((trunk + 1).max(1))
            } else {
                0
            }
    }
    pub fn place(
        &self,
        t: &TreeFeature,
        c: &mut FeatureContext<'_, '_>,
        a: Attachment,
        height: i32,
        radius: i32,
        out: &mut Positions,
    ) -> Result<(), Error> {
        let offset = self.offset.sample(c.random);
        let h = height + a.height;
        let mut row = |c: &mut FeatureContext<'_, '_>, p: Pos, r: i32, y: i32| {
            self.row(t, c, p, r, y, a.double, out)
        };
        match &self.kind {
            Kind::Blob(_) | Kind::Bush(_) => {
                for y in (offset - h..=offset).rev() {
                    let r = if matches!(self.kind, Kind::Bush(_)) {
                        radius + a.radius - 1 - y
                    } else {
                        (radius + a.radius - 1 - y / 2).max(0)
                    };
                    row(c, a.pos, r, y)?;
                }
            }
            Kind::Fancy(_) => {
                for y in (offset - height..=offset).rev() {
                    row(
                        c,
                        a.pos,
                        radius + i32::from(y != offset && y != offset - height),
                        y,
                    )?;
                }
            }
            Kind::Acacia => {
                let p = add(a.pos, [0, offset, 0]);
                row(c, p, radius + a.radius, -1 - h)?;
                row(c, p, radius - 1, -h)?;
                row(c, p, radius + a.radius - 1, 0)?;
            }
            Kind::DarkOak => {
                let p = add(a.pos, [0, offset, 0]);
                if a.double {
                    row(c, p, radius + 2, -1)?;
                    row(c, p, radius + 3, 0)?;
                    row(c, p, radius + 2, 1)?;
                    if c.random.next_boolean() {
                        row(c, p, radius, 2)?;
                    }
                } else {
                    row(c, p, radius + 2, -1)?;
                    row(c, p, radius + 1, 0)?;
                }
            }
            Kind::Jungle(_) => {
                let h = if a.double {
                    height
                } else {
                    1 + c.random.next_int_bound(2)
                } + a.height;
                for y in (offset - h..=offset).rev() {
                    row(c, a.pos, radius + a.radius + 1 - y, y)?;
                }
            }
            Kind::MegaPine(_) => {
                let mut prev = 0;
                for yy in a.pos[1] - h + offset..=a.pos[1] + offset {
                    let yo = a.pos[1] - yy;
                    let smooth = radius + a.radius + (yo as f32 / h as f32 * 3.5).floor() as i32;
                    let jagged = if yo > 0 && smooth == prev && yy & 1 == 0 {
                        smooth + 1
                    } else {
                        smooth
                    };
                    row(c, [a.pos[0], yy, a.pos[2]], jagged, 0)?;
                    prev = smooth;
                }
            }
            Kind::Pine(_) => {
                let mut r = 0;
                for y in (offset - h..=offset).rev() {
                    row(c, a.pos, r, y)?;
                    if r >= 1 && y == offset - h + 1 {
                        r -= 1;
                    } else if r < radius + a.radius {
                        r += 1;
                    }
                }
            }
            Kind::Spruce(_) => {
                let mut r = c.random.next_int_bound(2);
                let mut max = 1;
                let mut min = 0;
                for y in (-h..=offset).rev() {
                    row(c, a.pos, r, y)?;
                    if r >= max {
                        r = min;
                        min = 1;
                        max = (max + 1).min(radius + a.radius);
                    } else {
                        r += 1;
                    }
                }
            }
            Kind::Spread { attempts, .. } => {
                for _ in 0..*attempts {
                    let delta = [
                        c.random.next_int_bound(radius) - c.random.next_int_bound(radius),
                        c.random.next_int_bound(height) - c.random.next_int_bound(height),
                        c.random.next_int_bound(radius) - c.random.next_int_bound(radius),
                    ];
                    leaf(t, c, add(a.pos, delta), out)?;
                }
            }
            Kind::Cherry {
                hanging, extension, ..
            } => {
                let p = add(a.pos, [0, offset, 0]);
                let r = radius + a.radius - 1;
                row(c, p, r - 2, h - 3)?;
                row(c, p, r - 1, h - 4)?;
                for y in (0..=h - 5).rev() {
                    row(c, p, r, y)?;
                }
                self.hanging(t, c, p, r, -1, a.double, *hanging, *extension, out)?;
                self.hanging(t, c, p, r - 1, -2, a.double, *hanging, *extension, out)?;
            }
            Kind::Poplar { hole, .. } => {
                let p = add(a.pos, [0, offset, 0]);
                let r = radius + a.radius - 1;
                let flip = c.random.next_boolean();
                for (r, y) in [(r - 2, h - 1), (r - 1, h - 2), (r - 1, h - 3)]
                    .into_iter()
                    .chain((1..=h - 4).rev().map(|y| (r, y)))
                    .chain([(r - 1, 0), ((r - 2).clamp(1, 2), -1)])
                {
                    // The two bottom rows are deferred until after the log replacement.
                    if y == 0 || y == -1 {
                        continue;
                    }
                    self.poplar_row(t, c, p, r, y, a.double, h, flip, *hole, out)?;
                }
                for x in -r..=r + i32::from(a.double) {
                    for z in -r..=r + i32::from(a.double) {
                        let cut = poplar_cut(x, z, r, h, h - 4, flip);
                        if x.abs() + z.abs() <= r * 2 - (cut + 2)
                            && (z == 0 && r - x.abs() >= 4 || x == 0 && r - z.abs() >= 4)
                        {
                            let q = add(p, [x, h - 4, z]);
                            let old = c.region.block(q)?;
                            let expected = t.foliage_provider.get_state(c, q)?;
                            if old == expected {
                                let s = axis_state(
                                    t.trunk_provider.get_state(c, q)?,
                                    if z == 0 { Axis::X } else { Axis::Z },
                                );
                                put(c, q, s, out)?;
                            }
                        }
                    }
                }
                self.poplar_row(t, c, p, r - 1, 0, a.double, h, flip, *hole, out)?;
                self.poplar_row(
                    t,
                    c,
                    p,
                    (r - 2).clamp(1, 2),
                    -1,
                    a.double,
                    h,
                    flip,
                    *hole,
                    out,
                )?;
            }
        }
        Ok(())
    }
    fn skip(&self, rng: &mut WorldgenRandom, x: i32, y: i32, z: i32, r: i32, double: bool) -> bool {
        match &self.kind {
            Kind::Blob(_) => x == r && z == r && (rng.next_int_bound(2) == 0 || y == 0),
            Kind::Bush(_) => x == r && z == r && rng.next_int_bound(2) == 0,
            Kind::Fancy(_) => (x as f32 + 0.5).powi(2) + (z as f32 + 0.5).powi(2) > (r * r) as f32,
            Kind::Acacia => {
                if y == 0 {
                    (x > 1 || z > 1) && x != 0 && z != 0
                } else {
                    x == r && z == r && r > 0
                }
            }
            Kind::DarkOak => {
                if y == -1 && !double {
                    x == r && z == r
                } else {
                    y == 1 && x + z > r * 2 - 2
                }
            }
            Kind::Jungle(_) | Kind::MegaPine(_) => x + z >= 7 || x * x + z * z > r * r,
            Kind::Pine(_) | Kind::Spruce(_) => x == r && z == r && r > 0,
            Kind::Cherry { wide, corner, .. } => {
                if y == -1 && (x == r || z == r) && rng.next_float() < *wide {
                    return true;
                }
                let is_corner = x == r && z == r;
                if r > 2 {
                    is_corner || x + z > r * 2 - 2 && rng.next_float() < *corner
                } else {
                    is_corner && rng.next_float() < *corner
                }
            }
            Kind::Spread { .. } => false,
            Kind::Poplar { .. } => unreachable!("poplar uses signed coordinates"),
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn row(
        &self,
        t: &TreeFeature,
        c: &mut FeatureContext<'_, '_>,
        p: Pos,
        r: i32,
        y: i32,
        double: bool,
        out: &mut Positions,
    ) -> Result<(), Error> {
        for x in -r..=r + i32::from(double) {
            for z in -r..=r + i32::from(double) {
                if matches!(self.kind, Kind::DarkOak)
                    && y == 0
                    && double
                    && (x == -r || x >= r)
                    && (z == -r || z >= r)
                {
                    continue;
                }
                let dx = if double {
                    x.abs().min((x - 1).abs())
                } else {
                    x.abs()
                };
                let dz = if double {
                    z.abs().min((z - 1).abs())
                } else {
                    z.abs()
                };
                if !self.skip(c.random, dx, y, dz, r, double) {
                    leaf(t, c, add(p, [x, y, z]), out)?;
                }
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn hanging(
        &self,
        t: &TreeFeature,
        c: &mut FeatureContext<'_, '_>,
        p: Pos,
        r: i32,
        y: i32,
        double: bool,
        chance: f32,
        extension: f32,
        out: &mut Positions,
    ) -> Result<(), Error> {
        self.row(t, c, p, r, y, double, out)?;
        let log = add(p, [0, -1, 0]);
        for along in HORIZONTAL {
            let edge = along.clockwise();
            let positive = matches!(edge, Direction::East | Direction::South);
            let distance = r + if positive { i32::from(double) } else { 0 };
            let mut q = relative(relative(add(p, [0, y - 1, 0]), edge, distance), along, -r);
            for _ in -r..r + i32::from(double) {
                if out.contains(add(q, [0, 1, 0]))
                    && manhattan(q, log) < 7
                    && c.random.next_float() <= chance
                    && leaf(t, c, q, out)?
                {
                    let below = add(q, [0, -1, 0]);
                    if manhattan(below, log) < 7 && c.random.next_float() <= extension {
                        leaf(t, c, below, out)?;
                    }
                }
                q = step(q, along);
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn poplar_row(
        &self,
        t: &TreeFeature,
        c: &mut FeatureContext<'_, '_>,
        p: Pos,
        r: i32,
        y: i32,
        double: bool,
        h: i32,
        flip: bool,
        hole: f32,
        out: &mut Positions,
    ) -> Result<(), Error> {
        for x in -r..=r + i32::from(double) {
            for z in -r..=r + i32::from(double) {
                let partial = y == h - 1 || y == h - 2;
                if partial && (x.abs() == r || z.abs() == r) {
                    continue;
                }
                let extra = i32::from(c.random.next_float() <= hole);
                if x.abs() + z.abs() <= r * 2 - (poplar_cut(x, z, r, h, y, flip) + extra) {
                    leaf(t, c, add(p, [x, y, z]), out)?;
                }
            }
        }
        Ok(())
    }
}
fn poplar_cut(x: i32, z: i32, r: i32, h: i32, y: i32, flip: bool) -> i32 {
    let small = if flip {
        x > 0 && z > 0 || x < 0 && z < 0
    } else {
        x > 0 && z < 0 || z > 0 && x < 0
    };
    if small {
        r - 1
    } else if y == h - 1 || y == h - 2 {
        r + 1
    } else {
        r
    }
}
fn leaf(
    t: &TreeFeature,
    c: &mut FeatureContext<'_, '_>,
    p: Pos,
    out: &mut Positions,
) -> Result<bool, Error> {
    let old = c.region.block(p)?;
    if old.get(properties::PERSISTENT).unwrap_or(false) || !t.valid(c, p)? {
        return Ok(false);
    }
    let mut s = t.foliage_provider.get_state(c, p)?;
    let fluid = old.data().fluid;
    s = s
        .with(
            properties::WATERLOGGED,
            fluid.fluid_type == ferraloom_data::registries::fluid::Id::WATER && fluid.source,
        )
        .unwrap_or(s);
    put(c, p, s, out)?;
    Ok(true)
}
