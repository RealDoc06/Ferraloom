//! NoiseRouter and RandomState's immutable, seeded sampler graph.
use crate::{Error, codec::*, density::*};
use serde_json::Value;

pub const ROUTER_FIELDS: [&str; 8] = [
    "temperature",
    "vegetation",
    "continents",
    "erosion",
    "depth",
    "ridges",
    "chunk_surface_level",
    "final_density",
];

/// Port of NoiseRouter. Its first six fields are Climate.Sampler coordinates.
#[derive(Clone, Debug)]
pub struct NoiseRouter {
    pub functions: [Function; 8],
}
impl NoiseRouter {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let functions = ROUTER_FIELDS
            .iter()
            .map(|name| DensityFunction::decode(field(v, name)?))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            functions: functions.try_into().unwrap(),
        })
    }
    pub fn compile(&self, compiler: &mut DensityFunctionCompiler) -> Result<CompiledRouter, Error> {
        let functions = self
            .functions
            .iter()
            .map(|f| compiler.compile(f))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CompiledRouter {
            functions: functions.try_into().unwrap(),
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CompiledRouter {
    pub functions: [NodeId; 8],
}
impl CompiledRouter {
    pub fn sample_climate(
        &self,
        compiler: &DensityFunctionCompiler,
        context: &mut SamplerContext,
        quart: [i32; 3],
    ) -> [i64; 6] {
        let block = quart.map(|v| v.wrapping_shl(2));
        std::array::from_fn(|i| {
            (compiler.sample_value(self.functions[i], context, block) * 10000.0) as i64
        })
    }
    pub fn sample_climate_volume(
        &self,
        compiler: &DensityFunctionCompiler,
        context: &mut SamplerContext,
        volume: DensityVolume,
    ) -> Vec<[i64; 6]> {
        let values = self.functions[..6]
            .iter()
            .map(|id| compiler.sample_volume(*id, context, volume))
            .collect::<Vec<_>>();
        (0..volume.len())
            .map(|i| std::array::from_fn(|axis| (values[axis][i] * 10000.0) as i64))
            .collect()
    }
}
