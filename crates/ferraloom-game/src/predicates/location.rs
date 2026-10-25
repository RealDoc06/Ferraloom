use super::{
    BlockPredicate, DoubleBounds, FluidPredicate, IntBounds, PredicateContext, PredicateRegistry,
};
use crate::loot::codec::{DecodeInput, PersistentInput};
use crate::loot::{Identifier, Result, codec};
use ferraloom_protocol::codec::HolderSet;
use ferraloom_util::{BlockPos, Vec3, mth};

/// Port of LightPredicate. Even unconstrained light requires a loaded position.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightPredicate {
    pub light: IntBounds,
}
impl LightPredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        Ok(Self {
            light: codec::optional(v, "light", IntBounds::decode)?.unwrap_or_default(),
        })
    }
    pub fn matches(&self, pos: BlockPos, facts: &dyn PredicateContext) -> Result<bool> {
        Ok(facts.loaded(pos)? && self.light.matches(facts.light(pos)?))
    }
}
/// Port of LocationPredicate#matches in vanilla's short-circuit query order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocationPredicate {
    pub position: Option<[DoubleBounds; 3]>,
    pub biomes: Option<HolderSet>,
    pub structures: Option<HolderSet>,
    pub dimension: Option<Identifier>,
    pub smokey: Option<bool>,
    pub light: Option<LightPredicate>,
    pub block: Option<BlockPredicate>,
    pub fluid: Option<FluidPredicate>,
    pub can_see_sky: Option<bool>,
}
impl LocationPredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        let set = |v: &I| -> Result<HolderSet> { Ok(HolderSet::from_input(v)?) };
        Ok(Self {
            position: codec::optional(v, "position", |v| {
                Ok([
                    codec::optional(v, "x", DoubleBounds::decode)?.unwrap_or_default(),
                    codec::optional(v, "y", DoubleBounds::decode)?.unwrap_or_default(),
                    codec::optional(v, "z", DoubleBounds::decode)?.unwrap_or_default(),
                ])
            })?,
            biomes: codec::optional(v, "biomes", set)?,
            structures: codec::optional(v, "structures", set)?,
            dimension: codec::optional(v, "dimension", codec::id)?,
            smokey: codec::optional(v, "smokey", codec::boolean)?,
            light: codec::optional(v, "light", LightPredicate::decode)?,
            block: codec::optional(v, "block", BlockPredicate::decode)?,
            fluid: codec::optional(v, "fluid", FluidPredicate::decode)?,
            can_see_sky: codec::optional(v, "can_see_sky", codec::boolean)?,
        })
    }
    pub fn matches(&self, point: Vec3, facts: &dyn PredicateContext) -> Result<bool> {
        if self.position.is_some_and(|[x, y, z]| {
            !x.matches(point.x) || !y.matches(point.y) || !z.matches(point.z)
        }) {
            return Ok(false);
        }
        if let Some(dimension) = &self.dimension
            && *dimension != facts.dimension()?
        {
            return Ok(false);
        }
        let pos = BlockPos::new(
            mth::floor(point.x),
            mth::floor(point.y),
            mth::floor(point.z),
        );
        let loaded = facts.loaded(pos)?;
        if let Some(biomes) = &self.biomes
            && (!loaded
                || !super::item::matches_set(
                    biomes,
                    &facts.biome(pos)?,
                    PredicateRegistry::Biome,
                    facts,
                )?)
        {
            return Ok(false);
        }
        if let Some(structures) = &self.structures
            && (!loaded || !facts.in_structure(pos, structures)?)
        {
            return Ok(false);
        }
        if let Some(smokey) = self.smokey
            && (!loaded || smokey != facts.smokey(pos)?)
        {
            return Ok(false);
        }
        if let Some(light) = &self.light
            && !light.matches(pos, facts)?
        {
            return Ok(false);
        }
        if let Some(block) = &self.block
            && !block.matches(pos, facts)?
        {
            return Ok(false);
        }
        if let Some(fluid) = &self.fluid
            && !fluid.matches(pos, facts)?
        {
            return Ok(false);
        }
        if let Some(sky) = self.can_see_sky
            && sky != facts.can_see_sky(pos)?
        {
            return Ok(false);
        }
        Ok(true)
    }
}
