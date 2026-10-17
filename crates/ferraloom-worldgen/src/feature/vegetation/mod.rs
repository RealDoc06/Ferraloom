//! Vegetation feature records. Block methods remain on the shared game bridge.
use super::{
    Feature, FeatureContext, FeatureLoader, PlacedFeature, state_provider::BlockStateProvider,
};
use crate::{
    Error,
    codec::*,
    placement::{
        predicate::{BlockPredicate, direction, offset, vertical_direction},
        providers::IntProvider,
    },
    region::{Heightmap, SavedTick},
};
use ferraloom_data::{Block, BlockClass, BlockState};
use ferraloom_util::{Direction, random::RandomSource};
use serde_json::Value;
use std::sync::Arc;

pub fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("bamboo".parse()?, |v, l| {
        Ok(Arc::new(BambooFeature {
            probability: probability(field(v, "probability")?)?,
            podzol_replaceable: l.data.blocks(&Value::String(
                "#minecraft:beneath_bamboo_podzol_replaceable".into(),
            ))?,
        }))
    })?;
    loader.register("block_column".parse()?, |v, l| {
        Ok(Arc::new(BlockColumnFeature {
            layers: list(field(v, "layers")?)?
                .iter()
                .map(|layer| {
                    Ok(Layer {
                        height: IntProvider::decode_bounded(field(layer, "height")?, 0, i32::MAX)?,
                        state: l.state_provider(field(layer, "provider")?)?,
                    })
                })
                .collect::<Result<_, Error>>()?,
            direction: direction(field(v, "direction")?)?,
            allowed_placement: BlockPredicate::decode(field(v, "allowed_placement")?, &l.data)?,
            prioritize_tip: boolean(field(v, "prioritize_tip")?)?,
        }))
    })?;
    loader.register("block_pile".parse()?, |v, l| {
        Ok(Arc::new(BlockPileFeature {
            state: l.state_provider(field(v, "state_provider")?)?,
        }))
    })?;
    loader.register("simple_block".parse()?, |v, l| {
        Ok(Arc::new(SimpleBlockFeature {
            state: l.state_provider(field(v, "to_place")?)?,
            schedule_tick: optional(v, "schedule_tick", false, boolean)?,
        }))
    })?;
    loader.register("vines".parse()?, |_, _| Ok(Arc::new(VinesFeature)))?;
    loader.register("random_neighbor_spread".parse()?, |v, l| {
        Ok(Arc::new(RandomNeighborSpreadFeature {
            block: l.state_provider(field(v, "block")?)?,
            neighbors: l.data.blocks(field(v, "accepted_neighbors")?)?,
            can_replace: BlockPredicate::decode(field(v, "can_replace")?, &l.data)?,
            attempts: IntProvider::decode_bounded(field(v, "attempts")?, 1, 3000)?,
            xz_offset: IntProvider::decode_bounded(field(v, "xz_offset")?, -16, 16)?,
            y_offset: IntProvider::decode_bounded(field(v, "y_offset")?, -16, 16)?,
        }))
    })?;
    loader.register("projected_random_patchy_square".parse()?, |v, l| {
        Ok(Arc::new(ProjectedRandomPatchySquare {
            block: l.state_provider(field(v, "block")?)?,
            project_through: BlockPredicate::decode(field(v, "project_through")?, &l.data)?,
            size: IntProvider::decode_bounded(field(v, "size")?, 1, 16)?,
            max_projection_height: int_range(field(v, "max_projection_height")?, 0, i32::MAX)?,
        }))
    })?;
    loader.register("overlay".parse()?, |v, l| {
        let value = field(v, "features")?;
        let values = if let Some(tag) = value.as_str().and_then(|s| s.strip_prefix('#')) {
            let tags = ferraloom_data::datapack::load_tags(
                l.data.resources,
                "worldgen/placed_feature",
                |id, _| Some(id.clone()),
            );
            if !tags.diagnostics.is_empty() {
                return Err(Error("invalid placed feature tags".into()));
            }
            tags.values
                .get(&tag.parse()?)
                .ok_or_else(|| Error(format!("missing placed feature tag {tag}")))?
                .iter()
                .map(|id| Value::String(id.to_string()))
                .collect()
        } else if value.is_string() {
            vec![value.clone()]
        } else {
            list(value)?.to_vec()
        };
        if values.is_empty() {
            return Err(Error("overlay requires at least one feature".into()));
        }
        Ok(Arc::new(OverlayFeature {
            features: values
                .iter()
                .map(|v| l.placed(v))
                .collect::<Result<_, _>>()?,
        }))
    })?;
    loader.register("single_block_pillar".parse()?, |v, l| {
        Ok(Arc::new(SingleBlockPillarFeature {
            block: l.state_provider(field(v, "block")?)?,
            can_replace: optional(v, "can_replace", BlockPredicate::True, |v| {
                BlockPredicate::decode(v, &l.data)
            })?,
            direction: vertical_direction(field(v, "direction")?)?,
            chance: optional(v, "chance_to_continue", 1.0, probability)?,
            cap: v.get("cap_feature").map(|v| l.placed(v)).transpose()?,
        }))
    })?;
    Ok(())
}
fn probability(v: &Value) -> Result<f32, Error> {
    let f = float(v)?;
    if !(0.0..=1.0).contains(&f) {
        return Err(Error("probability outside [0,1]".into()));
    }
    Ok(f)
}
pub(crate) fn relative(p: [i32; 3], d: Direction) -> [i32; 3] {
    let s = d.step();
    offset(p, [s.x, s.y, s.z])
}
fn bamboo(leaves: &str, stage: &str) -> BlockState {
    BlockState::from_properties(
        Block::BAMBOO,
        [("age", "1"), ("leaves", leaves), ("stage", stage)],
    )
    .unwrap()
}
/// Port of BambooFeature.
#[derive(Debug)]
struct BambooFeature {
    probability: f32,
    podzol_replaceable: Vec<Block>,
}
impl Feature for BambooFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let origin = ctx.origin;
        if !ctx.region.block(origin)?.is_air() {
            return Ok(false);
        }
        if ctx
            .region
            .can_survive(Block::BAMBOO.default_state(), origin)?
        {
            let height = ctx.random.next_int_bound(12) + 5;
            if ctx.random.next_float() < self.probability {
                let r = ctx.random.next_int_bound(4) + 1;
                for x in origin[0] - r..=origin[0] + r {
                    for z in origin[2] - r..=origin[2] + r {
                        if (x - origin[0]).pow(2) + (z - origin[2]).pow(2) <= r * r {
                            let p = [
                                x,
                                ctx.region.height_at(Heightmap::WorldSurface, x, z)? - 1,
                                z,
                            ];
                            if self
                                .podzol_replaceable
                                .contains(&ctx.region.block(p)?.block())
                            {
                                ctx.region.set_block(p, Block::PODZOL.default_state(), 2)?;
                            }
                        }
                    }
                }
            }
            let mut p = origin;
            for _ in 0..height {
                if !ctx.region.block(p)?.is_air() {
                    break;
                }
                ctx.region.set_block(p, bamboo("none", "0"), 2)?;
                p[1] += 1;
            }
            if p[1] - origin[1] >= 3 {
                ctx.region.set_block(p, bamboo("large", "1"), 2)?;
                p[1] -= 1;
                ctx.region.set_block(p, bamboo("large", "0"), 2)?;
                p[1] -= 1;
                ctx.region.set_block(p, bamboo("small", "0"), 2)?;
            }
        }
        Ok(true)
    }
}
/// Port of BlockColumnFeature.Layer.
#[derive(Debug)]
struct Layer {
    height: IntProvider,
    state: Arc<BlockStateProvider>,
}
/// Port of BlockColumnFeature, including its look-ahead truncation.
#[derive(Debug)]
struct BlockColumnFeature {
    layers: Vec<Layer>,
    direction: Direction,
    allowed_placement: BlockPredicate,
    prioritize_tip: bool,
}
impl Feature for BlockColumnFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let mut heights = self
            .layers
            .iter()
            .map(|l| l.height.sample(ctx.random))
            .collect::<Vec<_>>();
        let total = heights.iter().fold(0i32, |a, b| a.wrapping_add(*b));
        if total == 0 {
            return Ok(false);
        }
        let mut next = relative(ctx.origin, self.direction);
        for y in 0..total {
            if !self
                .allowed_placement
                .test(ctx.region, next, ctx.generator)?
            {
                let mut remove = total - y;
                for index in 0..heights.len() {
                    let i = if self.prioritize_tip {
                        index
                    } else {
                        heights.len() - 1 - index
                    };
                    let n = heights[i].min(remove);
                    heights[i] -= n;
                    remove -= n;
                    if remove == 0 {
                        break;
                    }
                }
                break;
            }
            next = relative(next, self.direction);
        }
        let mut p = ctx.origin;
        for (layer, height) in self.layers.iter().zip(heights) {
            for _ in 0..height {
                let state = layer.state.get_state(ctx, p)?;
                ctx.region.set_block(p, state, 2)?;
                p = relative(p, self.direction);
            }
        }
        Ok(true)
    }
}
/// Port of BlockPileFeature.
#[derive(Debug)]
struct BlockPileFeature {
    state: Arc<BlockStateProvider>,
}
impl Feature for BlockPileFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        if ctx.origin[1] < ctx.region.min_y() + 5 {
            return Ok(false);
        }
        let xr = 2 + ctx.random.next_int_bound(2);
        let zr = 2 + ctx.random.next_int_bound(2);
        for z in -zr..=zr {
            for y in 0..=1 {
                for x in -xr..=xr {
                    let selected = (x * x + z * z) as f32
                        <= ctx.random.next_float() * 10.0 - ctx.random.next_float() * 6.0;
                    if selected || f64::from(ctx.random.next_float()) < 0.031 {
                        let p = offset(ctx.origin, [x, y, z]);
                        if ctx.region.block(p)?.is_air() {
                            let below = relative(p, Direction::Down);
                            let allowed = if ctx.region.block(below)?.block() == Block::DIRT_PATH {
                                ctx.random.next_boolean()
                            } else {
                                ctx.region.has_sturdy_face(below, Direction::Up)?
                            };
                            if allowed {
                                let state = self.state.get_state(ctx, p)?;
                                ctx.region.set_block(p, state, 260)?;
                            }
                        }
                    }
                }
            }
        }
        Ok(true)
    }
}
/// Port of SimpleBlockFeature. Class membership comes from vanilla's registry.
#[derive(Debug)]
struct SimpleBlockFeature {
    state: Arc<BlockStateProvider>,
    schedule_tick: bool,
}
impl Feature for SimpleBlockFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let p = ctx.origin;
        let Some(state) = self.state.get_optional_state(ctx, p)? else {
            return Ok(false);
        };
        if !ctx.region.can_survive(state, p)? {
            return Ok(false);
        }
        if state.block().is_instance_of(BlockClass::DoublePlantBlock) {
            let above = ctx.region.block(relative(p, Direction::Up))?;
            if !above.is_air()
                && (state.data().fluid != above.data().fluid || !above.data().replaceable)
            {
                return Ok(false);
            }
            ctx.region.place_double_plant(state, p, 2)?;
        } else if state.block().is_instance_of(BlockClass::MossyCarpetBlock) {
            ctx.region.place_mossy_carpet(p, 2)?;
        } else {
            ctx.region.set_block(p, state, 2)?;
        }
        if self.schedule_tick {
            ctx.region.schedule_block_tick(SavedTick {
                kind: ctx.region.block(p)?.block(),
                position: p,
                priority: 0,
            })?;
        }
        Ok(true)
    }
}
/// Port of VinesFeature.
#[derive(Debug)]
struct VinesFeature;
impl Feature for VinesFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        if !ctx.region.block(ctx.origin)?.is_air() {
            return Ok(false);
        }
        for d in Direction::VALUES {
            if d != Direction::Down
                && ctx
                    .region
                    .vine_acceptable_neighbor(relative(ctx.origin, d), d)?
            {
                let state = BlockState::from_properties(Block::VINE, [(d.name(), "true")]).unwrap();
                ctx.region.set_block(ctx.origin, state, 2)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}
/// Port of RandomNeighborSpreadFeature.
#[derive(Debug)]
struct RandomNeighborSpreadFeature {
    block: Arc<BlockStateProvider>,
    neighbors: Vec<Block>,
    can_replace: BlockPredicate,
    attempts: IntProvider,
    xz_offset: IntProvider,
    y_offset: IntProvider,
}
impl Feature for RandomNeighborSpreadFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let state = self.block.get_state(ctx, ctx.origin)?;
        ctx.region.set_block(ctx.origin, state, 2)?;
        let attempts = self.attempts.sample(ctx.random);
        for _ in 0..attempts {
            let p = offset(
                ctx.origin,
                [
                    self.xz_offset.sample(ctx.random),
                    self.y_offset.sample(ctx.random),
                    self.xz_offset.sample(ctx.random),
                ],
            );
            if self.can_replace.test(ctx.region, p, ctx.generator)? {
                let mut count = 0;
                for d in Direction::VALUES {
                    if self
                        .neighbors
                        .contains(&ctx.region.block(relative(p, d))?.block())
                    {
                        count += 1;
                    }
                    if count > 1 {
                        break;
                    }
                }
                if count == 1 {
                    let state = self.block.get_state(ctx, p)?;
                    ctx.region.set_block(p, state, 2)?;
                }
            }
        }
        Ok(true)
    }
}
/// Port of ProjectedRandomPatchySquare.
#[derive(Debug)]
struct ProjectedRandomPatchySquare {
    block: Arc<BlockStateProvider>,
    project_through: BlockPredicate,
    size: IntProvider,
    max_projection_height: i32,
}
impl Feature for ProjectedRandomPatchySquare {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let size = self.size.sample(ctx.random);
        let bound = size * size + 1;
        for x in -size..=size {
            for z in -size..=size {
                if ctx.random.next_int_bound(bound) < bound - x.abs() * z.abs() {
                    let mut p = offset(ctx.origin, [x, 0, z]);
                    let mut drop = self.max_projection_height;
                    while self.project_through.test(
                        ctx.region,
                        relative(p, Direction::Down),
                        ctx.generator,
                    )? {
                        p[1] -= 1;
                        drop -= 1;
                        if drop <= 0 {
                            break;
                        }
                    }
                    if let Some(state) = self.block.get_optional_state(ctx, p)? {
                        ctx.region.set_block(p, state, 2)?;
                    }
                }
            }
        }
        Ok(true)
    }
}
/// Port of OverlayFeature. Every child runs, even after one succeeds.
#[derive(Debug)]
struct OverlayFeature {
    features: Vec<Arc<PlacedFeature>>,
}
impl Feature for OverlayFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let mut placed = false;
        for feature in &self.features {
            placed |= feature.place(ctx, false)?;
        }
        Ok(placed)
    }
}
/// Port of SingleBlockPillarFeature.
#[derive(Debug)]
struct SingleBlockPillarFeature {
    block: Arc<BlockStateProvider>,
    can_replace: BlockPredicate,
    direction: Direction,
    chance: f32,
    cap: Option<Arc<PlacedFeature>>,
}
impl Feature for SingleBlockPillarFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let mut p = ctx.origin;
        while self.can_replace.test(ctx.region, p, ctx.generator)?
            && ctx.random.next_float() < self.chance
            && p[1] >= ctx.region.min_y()
            && p[1] < ctx.region.min_y() + ctx.region.height()
        {
            let state = self.block.get_state(ctx, p)?;
            ctx.region.set_block(p, state, 2)?;
            p = relative(p, self.direction);
        }
        if let Some(cap) = &self.cap {
            let origin = ctx.origin;
            ctx.origin = relative(p, self.direction.opposite());
            let result = cap.place(ctx, false);
            ctx.origin = origin;
            result?;
        }
        Ok(true)
    }
}
