use super::*;

/// Port of MultifaceGrowthFeature. Placement/spreading use the game block's
/// shared behavior; even unsuccessful searches retain vanilla's shuffle draws.
#[derive(Debug)]
struct MultifaceGrowthFeature {
    block: Block,
    range: i32,
    directions: Vec<Direction>,
    chance: f32,
    support: Vec<Block>,
}
/// Port of SculkPatchFeature. SculkSpreader cursors belong to shared game behavior.
#[derive(Debug)]
struct SculkPatchFeature {
    charges: i32,
    amount: i32,
    attempts: i32,
    growth: i32,
    spread: i32,
}
pub(super) fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("multiface_growth".parse()?, |v, l| {
        let id = id(field(v, "block")?)?;
        let block = Block::from_name(&id.to_string())
            .ok_or_else(|| Error(format!("unknown block {id}")))?;
        if ![Block::GLOW_LICHEN, Block::SCULK_VEIN].contains(&block) {
            return Err(Error(
                "Growth block should be a multiface spreadeable block".into(),
            ));
        }
        let mut directions = Vec::new();
        if optional(v, "can_place_on_ceiling", false, boolean)? {
            directions.push(Direction::Up);
        }
        if optional(v, "can_place_on_floor", false, boolean)? {
            directions.push(Direction::Down);
        }
        if optional(v, "can_place_on_wall", false, boolean)? {
            directions.extend(HORIZONTAL);
        }
        Ok(Arc::new(MultifaceGrowthFeature {
            block,
            range: optional(v, "search_range", 10, |v| int_range(v, 1, 64))?,
            directions,
            chance: optional(v, "chance_of_spreading", 0.5, |v| float_range(v, 0.0, 1.0))?,
            support: l.data.blocks(field(v, "can_be_placed_on")?)?,
        }))
    })?;
    loader.register("sculk_patch".parse()?, |v, _| {
        Ok(Arc::new(SculkPatchFeature {
            charges: int_range(field(v, "charge_count")?, 1, 32)?,
            amount: int_range(field(v, "amount_per_charge")?, 1, 500)?,
            attempts: int_range(field(v, "spread_attempts")?, 1, 64)?,
            growth: int_range(field(v, "growth_rounds")?, 0, 8)?,
            spread: int_range(field(v, "spread_rounds")?, 0, 8)?,
        }))
    })
}
fn shuffle(values: &mut [Direction], random: &mut impl RandomSource) {
    for i in (2..=values.len()).rev() {
        let j = random.next_int_bound(i as i32) as usize;
        values.swap(i - 1, j);
    }
}
impl MultifaceGrowthFeature {
    fn place_growth(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        p: [i32; 3],
        old: BlockState,
        directions: &[Direction],
    ) -> Result<bool, Error> {
        for &face in directions {
            if !self
                .support
                .contains(&ctx.region.block(move_direction(p, face))?.block())
            {
                continue;
            }
            let Some(state) = ctx
                .region
                .multiface_state_for_placement(self.block, old, p, face)?
            else {
                return Ok(false);
            };
            ctx.region.set_block(p, state, 3)?;
            ctx.region.mark_post_processing(p)?;
            if ctx.random.next_float() < self.chance {
                ctx.region
                    .multiface_spread(state, p, face, ctx.random, true)?;
            }
            return Ok(true);
        }
        Ok(false)
    }
}
impl Feature for MultifaceGrowthFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let old = ctx.region.block(ctx.origin)?;
        if !empty_or_water(old) {
            return Ok(false);
        }
        let mut directions = self.directions.clone();
        shuffle(&mut directions, ctx.random);
        if self.place_growth(ctx, ctx.origin, old, &directions)? {
            return Ok(true);
        }
        for search in directions {
            let mut placement = self
                .directions
                .iter()
                .copied()
                .filter(|&d| d != search.opposite())
                .collect::<Vec<_>>();
            shuffle(&mut placement, ctx.random);
            for _ in 0..self.range {
                // Vanilla 26.3 resets from origin on every iteration here.
                let p = move_direction(ctx.origin, search);
                let old = ctx.region.block(p)?;
                if !empty_or_water(old) && old.block() != self.block {
                    break;
                }
                if self.place_growth(ctx, p, old, &placement)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}
impl Feature for SculkPatchFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        if !ctx.region.sculk_can_spread_from(ctx.origin)? {
            return Ok(false);
        }
        ctx.region.sculk_spread(
            ctx.origin,
            ctx.random,
            self.charges,
            self.amount,
            self.attempts,
            self.growth,
            self.spread,
        )?;
        Ok(true)
    }
}
