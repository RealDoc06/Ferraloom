use super::*;
use crate::{feature::PlacedFeature, placement::predicate::BlockPredicate, region::Heightmap};
use ferraloom_data::registries::fluid;
use ferraloom_util::random::RandomSource;
/// Port of MangroveRootPlacer, RootPlacer and AboveRootPlacement.
#[derive(Debug)]
pub(super) struct RootPlacer {
    offset: IntProvider,
    provider: Provider,
    above: Option<(Provider, f32)>,
    grow: Vec<Block>,
    mud: Vec<Block>,
    mud_provider: Provider,
    width: i32,
    length: i32,
    skew: f32,
    replaceable: Vec<Block>,
    water: Vec<fluid::Id>,
}
impl RootPlacer {
    pub fn decode(v: &Value, l: &mut FeatureLoader<'_>) -> Result<Self, Error> {
        if kind(v)?.to_string() != "minecraft:mangrove_root_placer" {
            return Err(Error("unknown root placer".into()));
        }
        let m = field(v, "mangrove_root_placement")?;
        let above = if let Some(a) = v.get("above_root_placement") {
            Some((
                l.state_provider(field(a, "above_root_provider")?)?,
                chance(a, "above_root_placement_chance")?,
            ))
        } else {
            None
        };
        Ok(Self {
            offset: IntProvider::decode(field(v, "trunk_offset_y")?)?,
            provider: l.state_provider(field(v, "root_provider")?)?,
            above,
            grow: l.data.blocks(field(m, "can_grow_through")?)?,
            mud: l.data.blocks(field(m, "muddy_roots_in")?)?,
            mud_provider: l.state_provider(field(m, "muddy_roots_provider")?)?,
            width: int(m, "max_root_width", 1, 12)?,
            length: int(m, "max_root_length", 1, 64)?,
            skew: chance(m, "random_skew_chance")?,
            replaceable: tag(l, "replaceable_by_trees")?,
            water: l.data.fluids(&Value::String("#minecraft:water".into()))?,
        })
    }
    pub fn origin(&self, o: Pos, r: &mut impl RandomSource) -> Pos {
        add(o, [0, self.offset.sample(r), 0])
    }
    fn valid(&self, c: &FeatureContext<'_, '_>, p: Pos) -> Result<bool, Error> {
        let s = c.level.block(p)?;
        Ok(is_air(s) || self.replaceable.contains(&s.block()) || self.grow.contains(&s.block()))
    }
    fn simulate(
        &self,
        c: &mut FeatureContext<'_, '_>,
        p: Pos,
        d: Direction,
        o: Pos,
        out: &mut Vec<Pos>,
        depth: i32,
    ) -> Result<bool, Error> {
        if depth == self.length || out.len() > self.length as usize {
            return Ok(false);
        }
        let below = add(p, [0, -1, 0]);
        let next = step(p, d);
        let width = manhattan(p, o);
        let positions = if width > self.width - 3 && width <= self.width {
            if c.random.next_float() < self.skew {
                vec![below, add(next, [0, -1, 0])]
            } else {
                vec![below]
            }
        } else if width > self.width || c.random.next_float() < self.skew {
            vec![below]
        } else if c.random.next_boolean() {
            vec![next]
        } else {
            vec![below]
        };
        for q in positions {
            if self.valid(c, q)? {
                out.push(q);
                if !self.simulate(c, q, d, o, out, depth + 1)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
    fn state(
        &self,
        c: &FeatureContext<'_, '_>,
        p: Pos,
        s: BlockState,
    ) -> Result<BlockState, Error> {
        Ok(s.with(
            properties::WATERLOGGED,
            self.water.contains(&c.level.fluid(p)?.fluid_type),
        )
        .unwrap_or(s))
    }
    pub fn place(
        &self,
        _t: &TreeFeature,
        c: &mut FeatureContext<'_, '_>,
        o: Pos,
        trunk: Pos,
        out: &mut Positions,
    ) -> Result<bool, Error> {
        for y in o[1]..trunk[1] {
            if !self.valid(c, [o[0], y, o[2]])? {
                return Ok(false);
            }
        }
        let mut positions = vec![add(trunk, [0, -1, 0])];
        for d in HORIZONTAL {
            let p = step(trunk, d);
            let mut branch = Vec::new();
            if !self.simulate(c, p, d, trunk, &mut branch, 0)? {
                return Ok(false);
            }
            positions.extend(branch);
            positions.push(p);
        }
        for p in positions {
            if self.mud.contains(&c.level.block(p)?.block()) {
                let s = self.mud_provider.get_state(c, p)?;
                let s = self.state(c, p, s)?;
                put(c, p, s, out)?;
            } else if self.valid(c, p)? {
                let s = self.provider.get_state(c, p)?;
                let s = self.state(c, p, s)?;
                put(c, p, s, out)?;
                if let Some((provider, chance)) = &self.above {
                    let above = add(p, [0, 1, 0]);
                    if c.random.next_float() < *chance && is_air(c.level.block(above)?) {
                        let s = provider.get_state(c, above)?;
                        let s = self.state(c, above, s)?;
                        put(c, above, s, out)?;
                    }
                }
            }
        }
        Ok(true)
    }
}
/// Port of RootSystemFeature, including the 26.3 slope tests.
#[derive(Debug)]
pub(super) struct RootSystem {
    tree: Arc<PlacedFeature>,
    space: i32,
    test_distance: i32,
    deviation: i32,
    radius: i32,
    replaceable: Vec<Block>,
    provider: Provider,
    attempts: i32,
    column: i32,
    hanging_radius: i32,
    hanging_span: i32,
    hanging: Provider,
    hanging_attempts: i32,
    water_height: i32,
    allowed: BlockPredicate,
    water: Vec<fluid::Id>,
    lava: Vec<fluid::Id>,
}
impl RootSystem {
    pub fn decode(v: &Value, l: &mut FeatureLoader<'_>) -> Result<Self, Error> {
        Ok(Self {
            tree: l.placed(field(v, "feature")?)?,
            space: int(v, "required_vertical_space_for_tree", 1, 64)?,
            test_distance: int(v, "level_test_distance", 0, 16)?,
            deviation: int(v, "max_level_deviation", 0, 64)?,
            radius: int(v, "root_radius", 1, 64)?,
            replaceable: l.data.blocks(field(v, "root_replaceable")?)?,
            provider: l.state_provider(field(v, "root_state_provider")?)?,
            attempts: int(v, "root_placement_attempts", 1, 256)?,
            column: int(v, "root_column_max_height", 1, 4096)?,
            hanging_radius: int(v, "hanging_root_radius", 1, 64)?,
            hanging_span: int(v, "hanging_roots_vertical_span", 1, 16)?,
            hanging: l.state_provider(field(v, "hanging_root_state_provider")?)?,
            hanging_attempts: int(v, "hanging_root_placement_attempts", 1, 256)?,
            water_height: int(v, "allowed_vertical_water_for_tree", 1, 64)?,
            allowed: BlockPredicate::decode(field(v, "allowed_tree_position")?, &l.data)?,
            water: l.data.fluids(&Value::String("#minecraft:water".into()))?,
            lava: l.data.fluids(&Value::String("#minecraft:lava".into()))?,
        })
    }
    fn space(&self, c: &FeatureContext<'_, '_>, p: Pos) -> Result<bool, Error> {
        for y in 1..=self.space {
            let s = c.level.block(add(p, [0, y, 0]))?;
            if !is_air(s)
                && !(y < self.water_height && self.water.contains(&s.data().fluid.fluid_type))
            {
                return Ok(false);
            }
        }
        if self.test_distance > 0 {
            for i in 0..4 {
                let p = relative(p, Direction::from_2d_data_value(i), self.test_distance);
                if is_air(c.level.block(add(p, [0, -self.deviation, 0]))?)
                    || !is_air(c.level.block(add(p, [0, self.deviation, 0]))?)
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
    fn tree(&self, c: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let origin = c.origin;
        for y in 0..self.column {
            let p = add(origin, [0, y + 1, 0]);
            if c.level.height_at(Heightmap::WorldSurface, p[0], p[2])? < p[1] {
                return Ok(false);
            }
            if self.allowed.test(&mut c.level, p, c.generator)? && self.space(c, p)? {
                let below = add(p, [0, -1, 0]);
                if self.lava.contains(&c.level.fluid(below)?.fluid_type)
                    || !c.level.block(below)?.is_solid()
                {
                    return Ok(false);
                }
                c.origin = p;
                let result = self.tree.place(c, false);
                c.origin = origin;
                if result? {
                    for dy in 0..y {
                        for _ in 0..self.attempts {
                            let x = c.random.next_int_bound(self.radius)
                                - c.random.next_int_bound(self.radius);
                            let z = c.random.next_int_bound(self.radius)
                                - c.random.next_int_bound(self.radius);
                            let q = add(origin, [x, dy, z]);
                            if self.replaceable.contains(&c.level.block(q)?.block()) {
                                let s = self.provider.get_state(c, q)?;
                                c.level.set_block(q, s, 2)?;
                            }
                        }
                    }
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}
impl Feature for RootSystem {
    fn place(&self, c: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        if !is_air(c.level.block(c.origin)?) {
            return Ok(false);
        }
        if self.tree(c)? {
            for _ in 0..self.hanging_attempts {
                let delta = [
                    c.random.next_int_bound(self.hanging_radius)
                        - c.random.next_int_bound(self.hanging_radius),
                    c.random.next_int_bound(self.hanging_span)
                        - c.random.next_int_bound(self.hanging_span),
                    c.random.next_int_bound(self.hanging_radius)
                        - c.random.next_int_bound(self.hanging_radius),
                ];
                let p = add(c.origin, delta);
                if is_air(c.level.block(p)?) {
                    let s = self.hanging.get_state(c, p)?;
                    let above = c.level.block(add(p, [0, 1, 0]))?;
                    if c.level.can_survive(s, p)?
                        && c.level.state_has_sturdy_face(above, p, Direction::Down)?
                    {
                        c.level.set_block(p, s, 2)?;
                    }
                }
            }
        }
        Ok(true)
    }
}
