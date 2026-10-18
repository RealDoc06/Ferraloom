use super::*;
/// Port of FallenTreeFeature. The stump is placed even if the log is obstructed.
#[derive(Debug)]
pub(super) struct FallenTree {
    provider: Provider,
    length: IntProvider,
    stump: Vec<Decorator>,
    log: Vec<Decorator>,
    replaceable: Vec<Block>,
}
impl FallenTree {
    pub fn decode(v: &Value, l: &mut FeatureLoader<'_>) -> Result<Self, Error> {
        Ok(Self {
            provider: l.state_provider(field(v, "trunk_provider")?)?,
            length: provider_int(v, "log_length", 0, 16)?,
            stump: Decorator::list(field(v, "stump_decorators")?, l)?,
            log: Decorator::list(field(v, "log_decorators")?, l)?,
            replaceable: tag(l, "replaceable_by_trees")?,
        })
    }
    fn valid(&self, c: &FeatureContext<'_, '_>, p: Pos) -> Result<bool, Error> {
        let s = c.level.block(p)?;
        Ok(is_air(s) || self.replaceable.contains(&s.block()))
    }
    fn supported(c: &FeatureContext<'_, '_>, p: Pos) -> Result<bool, Error> {
        let below = c.level.block(add(p, [0, -1, 0]))?;
        c.level.state_has_sturdy_face(below, p, Direction::Up)
    }
    fn log(
        &self,
        c: &mut FeatureContext<'_, '_>,
        p: Pos,
        axis: Option<Axis>,
        out: &mut Positions,
    ) -> Result<(), Error> {
        let s = self.provider.get_state(c, p)?;
        c.level
            .set_block(p, axis.map_or(s, |a| axis_state(s, a)), 3)?;
        out.insert(p);
        for y in 1..=2 {
            let q = add(p, [0, y, 0]);
            if is_air(c.level.block(q)?) {
                break;
            }
            c.level.mark_post_processing(q)?;
        }
        Ok(())
    }
}
impl Feature for FallenTree {
    fn place(&self, c: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let mut stump = Positions::default();
        self.log(c, c.origin, None, &mut stump)?;
        decorators::decorate(
            &self.stump,
            c,
            &stump,
            &Positions::default(),
            &Positions::default(),
            &mut Positions::default(),
        )?;
        let d = horizontal(c);
        let length = self.length.sample(&mut c.random) - 2;
        let distance = 2 + c.random.next_int_bound(2);
        let mut p = add(relative(c.origin, d, distance), [0, 1, 0]);
        for _ in 0..6 {
            if self.valid(c, p)? && Self::supported(c, p)? {
                break;
            }
            p[1] -= 1;
        }
        let start = p;
        let mut gap = 0;
        for _ in 0..length {
            if !self.valid(c, p)? {
                return Ok(true);
            }
            if !Self::supported(c, p)? {
                gap += 1;
                if gap > 2 {
                    return Ok(true);
                }
            } else {
                gap = 0;
            }
            p = step(p, d);
        }
        let mut logs = Positions::default();
        let mut p = start;
        for _ in 0..length {
            self.log(c, p, Some(d.axis()), &mut logs)?;
            p = step(p, d);
        }
        decorators::decorate(
            &self.log,
            c,
            &logs,
            &Positions::default(),
            &Positions::default(),
            &mut Positions::default(),
        )?;
        Ok(true)
    }
}
