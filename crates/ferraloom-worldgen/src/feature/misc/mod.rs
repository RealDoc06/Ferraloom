//! Selector composition and miscellaneous world generation.
use super::{Feature, FeatureContext, FeatureLoader, PlacedFeature};
use crate::{
    Error, codec::*, generator::is_air, placement::predicate::move_direction, region::Heightmap,
};
use ferraloom_data::Block;
use ferraloom_util::{Direction, random::RandomSource};
use serde_json::Value;
use std::sync::Arc;

pub fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    for name in [
        "random_boolean_selector",
        "random_selector",
        "simple_random_selector",
        "weighted_random_selector",
        "sequence",
    ] {
        loader.register(name.parse()?, decode_selector)?;
    }
    loader.register("void_start_platform".parse()?, |_, _| {
        Ok(Arc::new(VoidStartPlatformFeature))
    })?;
    loader.register("bonus_chest".parse()?, |_, _| {
        Ok(Arc::new(BonusChestFeature))
    })?;
    Ok(())
}
/// Port of RandomBooleanSelectorFeature, RandomSelectorFeature,
/// SimpleRandomSelectorFeature, WeightedRandomSelectorFeature and SequenceFeature.
#[derive(Debug)]
enum Selector {
    Boolean {
        yes: Arc<PlacedFeature>,
        no: Arc<PlacedFeature>,
    },
    Random {
        features: Vec<(Arc<PlacedFeature>, f32)>,
        default: Arc<PlacedFeature>,
    },
    Simple(Vec<Arc<PlacedFeature>>),
    Weighted {
        features: Vec<(Arc<PlacedFeature>, i32)>,
        total: i32,
    },
    Sequence(Vec<Arc<PlacedFeature>>),
}
fn placed_set(v: &Value, l: &mut FeatureLoader<'_>) -> Result<Vec<Arc<PlacedFeature>>, Error> {
    let values = if let Some(tag) = v.as_str().and_then(|v| v.strip_prefix('#')) {
        let tags = ferraloom_data::datapack::load_tags(
            l.data.resources,
            "worldgen/placed_feature",
            |id, _| Some(id.clone()),
        );
        let ids = tags
            .values
            .get(&tag.parse()?)
            .ok_or_else(|| Error(format!("missing placed feature tag {tag}")))?;
        ids.iter()
            .map(|id| Value::String(id.to_string()))
            .collect::<Vec<_>>()
    } else if v.is_string() {
        vec![v.clone()]
    } else {
        list(v)?.to_vec()
    };
    if values.is_empty() {
        return Err(Error("feature holder set must be nonempty".into()));
    }
    values.iter().map(|v| l.placed(v)).collect()
}
fn decode_selector(v: &Value, l: &mut FeatureLoader<'_>) -> Result<Arc<dyn Feature>, Error> {
    let selector = match kind(v)?.path() {
        "random_boolean_selector" => Selector::Boolean {
            yes: l.placed(field(v, "feature_true")?)?,
            no: l.placed(field(v, "feature_false")?)?,
        },
        "random_selector" => Selector::Random {
            features: list(field(v, "features")?)?
                .iter()
                .map(|v| {
                    let feature = l.placed(field(v, "feature")?)?;
                    let chance = float(field(v, "chance")?)?;
                    if !(0.0..=1.0).contains(&chance) {
                        return Err(Error("selector chance outside [0,1]".into()));
                    }
                    Ok((feature, chance))
                })
                .collect::<Result<_, Error>>()?,
            default: l.placed(field(v, "default")?)?,
        },
        "simple_random_selector" => Selector::Simple(placed_set(field(v, "features")?, l)?),
        "sequence" => Selector::Sequence(placed_set(field(v, "features")?, l)?),
        "weighted_random_selector" => {
            let mut total = 0i32;
            let features = list(field(v, "features")?)?
                .iter()
                .map(|v| {
                    let weight = int_range(field(v, "weight")?, 0, i32::MAX)?;
                    total = total
                        .checked_add(weight)
                        .ok_or_else(|| Error("weight total exceeds i32::MAX".into()))?;
                    Ok((l.placed(field(v, "data")?)?, weight))
                })
                .collect::<Result<_, Error>>()?;
            Selector::Weighted { features, total }
        }
        _ => return Err(Error("unknown selector".into())),
    };
    Ok(Arc::new(selector))
}
impl Feature for Selector {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        match self {
            Self::Boolean { yes, no } => {
                if ctx.random.next_boolean() { yes } else { no }.place(ctx, false)
            }
            Self::Random { features, default } => {
                for (feature, chance) in features {
                    if ctx.random.next_float() < *chance {
                        return feature.place(ctx, false);
                    }
                }
                default.place(ctx, false)
            }
            Self::Simple(features) => {
                let i = ctx.random.next_int_bound(features.len() as i32) as usize;
                features[i].place(ctx, false)
            }
            Self::Weighted { features, total } => {
                if *total == 0 {
                    return Ok(false);
                }
                let mut choice = ctx.random.next_int_bound(*total);
                for (feature, weight) in features {
                    choice -= weight;
                    if choice < 0 {
                        return feature.place(ctx, false);
                    }
                }
                unreachable!("validated weight total")
            }
            Self::Sequence(features) => {
                for feature in features {
                    if !feature.place(ctx, false)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
        }
    }
}
/// Port of VoidStartPlatformFeature.
#[derive(Debug)]
struct VoidStartPlatformFeature;
impl Feature for VoidStartPlatformFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let [cx, cz] = [ctx.origin[0] >> 4, ctx.origin[2] >> 4];
        if cx.abs().max(cz.abs()) > 1 {
            return Ok(true);
        }
        for z in cz * 16..cz * 16 + 16 {
            for x in cx * 16..cx * 16 + 16 {
                if (x - 8).abs().max((z - 8).abs()) <= 16 {
                    let block = if x == 8 && z == 8 {
                        Block::COBBLESTONE
                    } else {
                        Block::STONE
                    };
                    ctx.region
                        .set_block([x, ctx.origin[1] + 3, z], block.default_state(), 2)?;
                }
            }
        }
        Ok(true)
    }
}
/// Port of BonusChestFeature. Collision, loot assignment and torch survival use
/// the shared game bridge, rather than approximating block behavior here.
#[derive(Debug)]
struct BonusChestFeature;
impl Feature for BonusChestFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let mut xs = (0..16)
            .map(|n| (ctx.origin[0] >> 4) * 16 + n)
            .collect::<Vec<_>>();
        let mut zs = (0..16)
            .map(|n| (ctx.origin[2] >> 4) * 16 + n)
            .collect::<Vec<_>>();
        for values in [&mut xs, &mut zs] {
            for i in (2..=values.len()).rev() {
                let j = ctx.random.next_int_bound(i as i32) as usize;
                values.swap(i - 1, j);
            }
        }
        for x in xs {
            for &z in &zs {
                let p = [
                    x,
                    ctx.region
                        .height_at(Heightmap::MotionBlockingNoLeaves, x, z)?,
                    z,
                ];
                if is_air(ctx.region.block(p)?) || ctx.region.is_collision_empty(p)? {
                    ctx.region.set_block(p, Block::CHEST.default_state(), 2)?;
                    ctx.region.set_bonus_chest_loot(p, ctx.random)?;
                    let torch = Block::TORCH.default_state();
                    for d in [
                        Direction::North,
                        Direction::East,
                        Direction::South,
                        Direction::West,
                    ] {
                        let p = move_direction(p, d);
                        if ctx.region.can_survive(torch, p)? {
                            ctx.region.set_block(p, torch, 2)?;
                        }
                    }
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests;
