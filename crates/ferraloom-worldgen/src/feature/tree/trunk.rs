use super::*;
use ferraloom_util::{mth, random::RandomSource};
/// Port of FoliagePlacer.FoliageAttachment. Poplar uses the height-offset field.
#[derive(Clone, Copy, Debug)]
pub(super) struct Attachment {
    pub pos: Pos,
    pub radius: i32,
    pub height: i32,
    pub double: bool,
}
impl Attachment {
    fn new(pos: Pos, radius: i32, double: bool) -> Self {
        Self {
            pos,
            radius,
            height: 0,
            double,
        }
    }
}
/// Port of every registered 26.3 TrunkPlacer.
#[derive(Debug)]
pub(super) struct TrunkPlacer {
    base: i32,
    rand_a: i32,
    rand_b: i32,
    kind: Kind,
}
#[derive(Debug)]
enum Kind {
    Straight,
    Giant,
    MegaJungle,
    DarkOak,
    Fancy,
    Forking,
    Bending {
        min_leaves: i32,
        length: IntProvider,
    },
    Cherry {
        count: IntProvider,
        length: IntProvider,
        start: [i32; 2],
        end: IntProvider,
    },
    Poplar {
        above: IntProvider,
        amount: IntProvider,
    },
    Upwards {
        steps: IntProvider,
        probability: f32,
        length: IntProvider,
        grow: Vec<Block>,
    },
}
impl TrunkPlacer {
    pub fn decode(v: &Value, l: &FeatureLoader<'_>) -> Result<Self, Error> {
        let ty = kind(v)?;
        Ok(Self {
            base: int(v, "base_height", 0, 32)?,
            rand_a: int(v, "height_rand_a", 0, 24)?,
            rand_b: int(v, "height_rand_b", 0, 24)?,
            kind: match ty.to_string().as_str() {
                "minecraft:straight_trunk_placer" => Kind::Straight,
                "minecraft:giant_trunk_placer" => Kind::Giant,
                "minecraft:mega_jungle_trunk_placer" => Kind::MegaJungle,
                "minecraft:dark_oak_trunk_placer" => Kind::DarkOak,
                "minecraft:fancy_trunk_placer" => Kind::Fancy,
                "minecraft:forking_trunk_placer" => Kind::Forking,
                "minecraft:bending_trunk_placer" => Kind::Bending {
                    min_leaves: optional(v, "min_height_for_leaves", 1, |v| {
                        int_range(v, 1, i32::MAX)
                    })?,
                    length: provider_int(v, "bend_length", 1, 64)?,
                },
                "minecraft:cherry_trunk_placer" => {
                    let s = field(v, "branch_start_offset_from_top")?;
                    let a = int(s, "min_inclusive", -16, 0)?;
                    let b = int(s, "max_inclusive", -16, 0)?;
                    if b - a < 1 {
                        return Err(Error("cherry branch start needs two heights".into()));
                    }
                    Kind::Cherry {
                        count: provider_int(v, "branch_count", 1, 3)?,
                        length: provider_int(v, "branch_horizontal_length", 2, 16)?,
                        start: [a, b],
                        end: provider_int(v, "branch_end_offset_from_top", -16, 16)?,
                    }
                }
                "minecraft:poplar_trunk_placer" => Kind::Poplar {
                    above: provider_int(v, "trunk_height_above_branches", 0, 8)?,
                    amount: provider_int(v, "branch_amount", 1, 4)?,
                },
                "minecraft:upwards_branching_trunk_placer" => Kind::Upwards {
                    steps: provider_int(v, "extra_branch_steps", 1, i32::MAX)?,
                    probability: chance(v, "place_branch_per_log_probability")?,
                    length: provider_int(v, "extra_branch_length", 0, i32::MAX)?,
                    grow: l.data.blocks(field(v, "can_grow_through")?)?,
                },
                _ => return Err(Error(format!("unknown trunk placer {ty}"))),
            },
        })
    }
    /// Port of TrunkPlacer#getBaseHeight; no randomized height sampling.
    pub(super) fn base_height(&self) -> i32 {
        self.base
    }
    pub fn height(&self, r: &mut impl RandomSource) -> i32 {
        self.base + r.next_int_bound(self.rand_a + 1) + r.next_int_bound(self.rand_b + 1)
    }
    fn valid(&self, t: &TreeFeature, c: &FeatureContext<'_, '_>, p: Pos) -> Result<bool, Error> {
        Ok(t.valid(c, p)?
            || matches!(&self.kind,Kind::Upwards{grow,..} if grow.contains(&c.level.block(p)?.block())))
    }
    pub fn free(&self, t: &TreeFeature, c: &FeatureContext<'_, '_>, p: Pos) -> Result<bool, Error> {
        Ok(self.valid(t, c, p)? || t.logs.contains(&c.level.block(p)?.block()))
    }
    fn log(
        &self,
        t: &TreeFeature,
        c: &mut FeatureContext<'_, '_>,
        p: Pos,
        axis: Option<Axis>,
        out: &mut Positions,
    ) -> Result<bool, Error> {
        if !self.valid(t, c, p)? {
            return Ok(false);
        }
        let s = t.trunk_provider.get_state(c, p)?;
        put(c, p, axis.map_or(s, |a| axis_state(s, a)), out)?;
        Ok(true)
    }
    pub fn place(
        &self,
        t: &TreeFeature,
        c: &mut FeatureContext<'_, '_>,
        o: Pos,
        h: i32,
        out: &mut Positions,
    ) -> Result<Vec<Attachment>, Error> {
        let mut attachments = Vec::new();
        match &self.kind {
            Kind::Straight => {
                t.below(c, add(o, [0, -1, 0]), out)?;
                for y in 0..h {
                    self.log(t, c, add(o, [0, y, 0]), None, out)?;
                }
                attachments.push(Attachment::new(add(o, [0, h, 0]), 0, false));
            }
            Kind::Giant | Kind::MegaJungle => {
                for d in [[0, -1, 0], [1, -1, 0], [0, -1, 1], [1, -1, 1]] {
                    t.below(c, add(o, d), out)?;
                }
                for y in 0..h {
                    for [x, z] in [[0, 0], [1, 0], [1, 1], [0, 1]] {
                        if [x, z] != [0, 0] && y >= h - 1 {
                            continue;
                        }
                        let p = add(o, [x, y, z]);
                        if self.free(t, c, p)? {
                            self.log(t, c, p, None, out)?;
                        }
                    }
                }
                attachments.push(Attachment::new(add(o, [0, h, 0]), 0, true));
                if matches!(self.kind, Kind::MegaJungle) {
                    let mut y = h - 2 - c.random()?.next_int_bound(4);
                    while y > h / 2 {
                        let angle = c.random()?.next_float() * std::f32::consts::TAU;
                        let mut x = 0;
                        let mut z = 0;
                        for b in 0..5 {
                            x = (1.5 + mth::cos(f64::from(angle)) * b as f32) as i32;
                            z = (1.5 + mth::sin(f64::from(angle)) * b as f32) as i32;
                            self.log(t, c, add(o, [x, y - 3 + b / 2, z]), None, out)?;
                        }
                        attachments.push(Attachment::new(add(o, [x, y, z]), -2, false));
                        y -= 2 + c.random()?.next_int_bound(4);
                    }
                }
            }
            Kind::DarkOak => {
                for d in [[0, -1, 0], [1, -1, 0], [0, -1, 1], [1, -1, 1]] {
                    t.below(c, add(o, d), out)?;
                }
                let d = horizontal(&mut c.random()?);
                let lean = h - c.random()?.next_int_bound(4);
                let mut steps = 2 - c.random()?.next_int_bound(3);
                let mut p = o;
                for y in 0..h {
                    if y >= lean && steps > 0 {
                        p = step(p, d);
                        steps -= 1;
                    }
                    p[1] = o[1] + y;
                    if t.air_or_leaves(c, p)? {
                        for delta in [[0, 0, 0], [1, 0, 0], [0, 0, 1], [1, 0, 1]] {
                            self.log(t, c, add(p, delta), None, out)?;
                        }
                    }
                }
                p[1] = o[1] + h - 1;
                attachments.push(Attachment::new(p, 0, true));
                for x in -1..=2 {
                    for z in -1..=2 {
                        if (!(0..=1).contains(&x) || !(0..=1).contains(&z))
                            && c.random()?.next_int_bound(3) <= 0
                        {
                            let length = c.random()?.next_int_bound(3) + 2;
                            for y in 0..length {
                                self.log(t, c, add(o, [x, h - y - 2, z]), None, out)?;
                            }
                            attachments.push(Attachment::new(add(o, [x, h - 1, z]), 0, false));
                        }
                    }
                }
            }
            Kind::Forking => {
                t.below(c, add(o, [0, -1, 0]), out)?;
                let d = horizontal(&mut c.random()?);
                let lean = h - c.random()?.next_int_bound(4) - 1;
                let mut steps = 3 - c.random()?.next_int_bound(3);
                let mut p = o;
                let mut top = None;
                for y in 0..h {
                    if y >= lean && steps > 0 {
                        p = step(p, d);
                        steps -= 1;
                    }
                    p[1] = o[1] + y;
                    if self.log(t, c, p, None, out)? {
                        top = Some(p[1] + 1);
                    }
                }
                if let Some(y) = top {
                    attachments.push(Attachment::new([p[0], y, p[2]], 1, false));
                }
                let mut p = o;
                let b = horizontal(&mut c.random()?);
                if b != d {
                    let start = lean - c.random()?.next_int_bound(2) - 1;
                    let steps = 1 + c.random()?.next_int_bound(3);
                    let mut top = None;
                    for y in start..h.min(start + steps) {
                        if y >= 1 {
                            p = step(p, b);
                            p[1] = o[1] + y;
                            if self.log(t, c, p, None, out)? {
                                top = Some(p[1] + 1);
                            }
                        }
                    }
                    if let Some(y) = top {
                        attachments.push(Attachment::new([p[0], y, p[2]], 0, false));
                    }
                }
            }
            Kind::Bending { min_leaves, length } => {
                let d = horizontal(&mut c.random()?);
                let mut p = o;
                t.below(c, add(p, [0, -1, 0]), out)?;
                for y in 0..h {
                    if y + 1 >= h - 1 + c.random()?.next_int_bound(2) {
                        p = step(p, d);
                    }
                    if t.valid(c, p)? {
                        self.log(t, c, p, None, out)?;
                    }
                    if y >= *min_leaves {
                        attachments.push(Attachment::new(p, 0, false));
                    }
                    p[1] += 1;
                }
                let length = length.sample(&mut c.random()?);
                for _ in 0..=length {
                    if t.valid(c, p)? {
                        self.log(t, c, p, None, out)?;
                    }
                    attachments.push(Attachment::new(p, 0, false));
                    p = step(p, d);
                }
            }
            Kind::Cherry {
                count,
                length,
                start,
                end,
            } => {
                t.below(c, add(o, [0, -1, 0]), out)?;
                let first =
                    (h - 1 + start[0] + c.random()?.next_int_bound(start[1] - start[0] + 1)).max(0);
                let mut second =
                    (h - 1 + start[0] + c.random()?.next_int_bound(start[1] - start[0])).max(0);
                if second >= first {
                    second += 1;
                }
                let count = count.sample(&mut c.random()?);
                let trunk_h = if count == 3 {
                    h
                } else if count >= 2 {
                    first.max(second) + 1
                } else {
                    first + 1
                };
                for y in 0..trunk_h {
                    self.log(t, c, add(o, [0, y, 0]), None, out)?;
                }
                if count == 3 {
                    attachments.push(Attachment::new(add(o, [0, trunk_h, 0]), 0, false));
                }
                let d = horizontal(&mut c.random()?);
                for (start, d) in std::iter::once((first, d))
                    .chain((count >= 2).then_some((second, d.opposite())))
                {
                    let mut p = add(o, [0, start, 0]);
                    let ey = h - 1 + end.sample(&mut c.random()?);
                    let extend = start < trunk_h - 1 || ey < start;
                    let distance = length.sample(&mut c.random()?) + i32::from(extend);
                    let target = relative(add(o, [0, ey, 0]), d, distance);
                    for _ in 0..if extend { 2 } else { 1 } {
                        p = step(p, d);
                        self.log(t, c, p, Some(d.axis()), out)?;
                    }
                    let v = if target[1] > p[1] {
                        Direction::Up
                    } else {
                        Direction::Down
                    };
                    while p != target {
                        let chance = (target[1] - p[1]).abs() as f32 / manhattan(p, target) as f32;
                        let vertical = c.random()?.next_float() < chance;
                        p = step(p, if vertical { v } else { d });
                        self.log(t, c, p, if vertical { None } else { Some(d.axis()) }, out)?;
                    }
                    attachments.push(Attachment::new(add(target, [0, 1, 0]), 0, false));
                }
            }
            Kind::Poplar { above, amount } => {
                t.below(c, add(o, [0, -1, 0]), out)?;
                let top = h - above.sample(&mut c.random()?);
                for y in 0..h {
                    self.log(t, c, add(o, [0, y, 0]), None, out)?;
                    let mut directions = Direction::VALUES;
                    shuffle(&mut directions, &mut c.random()?);
                    if top - 1 == y {
                        let amount = amount.sample(&mut c.random()?) as usize;
                        for d in directions
                            .into_iter()
                            .filter(|d| d.axis() != Axis::Y)
                            .take(amount)
                        {
                            self.log(t, c, step(add(o, [0, y, 0]), d), Some(d.axis()), out)?;
                        }
                    }
                }
                attachments.push(Attachment::new(add(o, [0, top, 0]), 0, false));
            }
            Kind::Upwards {
                steps,
                probability,
                length,
                ..
            } => {
                for y in 0..h {
                    let mut p = add(o, [0, y, 0]);
                    if self.log(t, c, p, None, out)?
                        && y < h - 1
                        && c.random()?.next_float() < *probability
                    {
                        let d = horizontal(&mut c.random()?);
                        let len = length.sample(&mut c.random()?);
                        let start = (len - length.sample(&mut c.random()?) - 1).max(0);
                        let steps = steps.sample(&mut c.random()?);
                        let mut top = p[1] + start;
                        for i in start..h.min(start + steps) {
                            if i >= 1 {
                                p = step(p, d);
                                p[1] = o[1] + y + i;
                                top = p[1];
                                if self.log(t, c, p, None, out)? {
                                    top += 1;
                                }
                                attachments.push(Attachment::new(p, 0, false));
                            }
                        }
                        if top - (o[1] + y) > 1 {
                            let p = [p[0], top, p[2]];
                            attachments.push(Attachment::new(p, 0, false));
                            attachments.push(Attachment::new(add(p, [0, -2, 0]), 0, false));
                        }
                    }
                    if y == h - 1 {
                        attachments.push(Attachment::new(add(o, [0, y + 1, 0]), 0, false));
                    }
                }
            }
            Kind::Fancy => return self.fancy(t, c, o, h, out),
        }
        Ok(attachments)
    }
    fn limb(
        &self,
        t: &TreeFeature,
        c: &mut FeatureContext<'_, '_>,
        start: Pos,
        end: Pos,
        place: bool,
        out: &mut Positions,
    ) -> Result<bool, Error> {
        if !place && start == end {
            return Ok(true);
        }
        let delta = std::array::from_fn::<_, 3, _>(|i| end[i] - start[i]);
        let steps = delta.iter().map(|d| d.abs()).max().unwrap();
        let ds = delta.map(|d| d as f32 / steps as f32);
        for i in 0..=steps {
            let p = add(start, ds.map(|d| mth::floor(f64::from(0.5 + i as f32 * d))));
            if place {
                let dx = (p[0] - start[0]).abs();
                let dz = (p[2] - start[2]).abs();
                let axis = if dx.max(dz) == 0 {
                    Axis::Y
                } else if dx >= dz {
                    Axis::X
                } else {
                    Axis::Z
                };
                self.log(t, c, p, Some(axis), out)?;
            } else if !self.free(t, c, p)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn fancy(
        &self,
        t: &TreeFeature,
        c: &mut FeatureContext<'_, '_>,
        o: Pos,
        h: i32,
        out: &mut Positions,
    ) -> Result<Vec<Attachment>, Error> {
        let height = h + 2;
        let trunk_height = (f64::from(height) * 0.618).floor() as i32;
        let top = o[1] + trunk_height;
        t.below(c, add(o, [0, -1, 0]), out)?;
        let clusters = 1.min((1.382 + (f64::from(height) / 13.0).powi(2)).floor() as i32);
        let mut coords = vec![(add(o, [0, height - 5, 0]), top)];
        for y in (0..=height - 5).rev() {
            if (y as f32) < height as f32 * 0.3 {
                continue;
            }
            let radius = height as f32 / 2.0;
            let adjacent = radius - y as f32;
            let shape = if adjacent == 0.0 {
                radius
            } else if adjacent.abs() >= radius {
                0.0
            } else {
                (radius * radius - adjacent * adjacent).sqrt()
            } * 0.5;
            for _ in 0..clusters {
                let r = f64::from(shape) * (f64::from(c.random()?.next_float()) + 0.328);
                let angle = f64::from(c.random()?.next_float() * 2.0) * std::f64::consts::PI;
                let p = add(
                    o,
                    [
                        mth::floor(r * angle.sin() + 0.5),
                        y - 1,
                        mth::floor(r * angle.cos() + 0.5),
                    ],
                );
                if self.limb(t, c, p, add(p, [0, 5, 0]), false, out)? {
                    let dx = o[0] - p[0];
                    let dz = o[2] - p[2];
                    let bh = f64::from(p[1]) - f64::from(dx * dx + dz * dz).sqrt() * 0.381;
                    let by = if bh > f64::from(top) { top } else { bh as i32 };
                    if self.limb(t, c, [o[0], by, o[2]], p, false, out)? {
                        coords.push((p, by));
                    }
                }
            }
        }
        self.limb(t, c, o, add(o, [0, trunk_height, 0]), true, out)?;
        for &(p, base) in &coords {
            let b = [o[0], base, o[2]];
            if b != p && f64::from(base - o[1]) >= f64::from(height) * 0.2 {
                self.limb(t, c, b, p, true, out)?;
            }
        }
        Ok(coords
            .into_iter()
            .filter(|(_, base)| f64::from(*base - o[1]) >= f64::from(height) * 0.2)
            .map(|(p, _)| Attachment::new(p, 0, false))
            .collect())
    }
}
