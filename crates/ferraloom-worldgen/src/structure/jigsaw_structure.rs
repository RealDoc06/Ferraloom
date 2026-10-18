//! JigsawStructure's datapack settings and start-height/random wiring.
use super::{
    TerrainAdjustment,
    alias::{PoolAliasBinding, PoolAliasLookup},
    jigsaw::{JigsawPlacement, JigsawStart},
    pool::{PoolHolder, StructurePools},
    template::{LiquidSettings, StructureTemplateManager},
};
use crate::{
    Error, Identifier,
    codec::*,
    feature::PlacedFeature,
    placement::providers::{HeightProvider, WorldGenerationContext},
    region::Heightmap,
};
use ferraloom_data::datapack::ResourceManager;
use ferraloom_util::random::{Algorithm, WorldgenRandom};
use serde_json::Value;
use std::sync::Arc;

/// Port of JigsawStructure. Biome acceptance and base-column sampling are
/// supplied by the caller's generation context, never a mutable global world.
#[derive(Clone, Debug)]
pub struct JigsawStructure {
    pub start_pool: PoolHolder,
    pub start_jigsaw_name: Option<Identifier>,
    pub max_depth: i32,
    pub start_height: HeightProvider,
    pub expansion_hack: bool,
    pub project_start: Option<Heightmap>,
    pub max_distance: [i32; 2],
    pub aliases: Vec<PoolAliasBinding>,
    pub padding: Option<[i32; 2]>,
    pub liquid: LiquidSettings,
}
impl JigsawStructure {
    pub fn decode(
        v: &Value,
        resources: &ResourceManager,
        decode_feature: &mut impl FnMut(&Value) -> Result<Arc<PlacedFeature>, Error>,
    ) -> Result<Self, Error> {
        let pool = field(v, "start_pool")?;
        let start_pool = if pool.is_string() {
            PoolHolder::Reference(id(pool)?)
        } else {
            PoolHolder::Direct(Box::new(super::pool::StructureTemplatePool::decode(
                pool,
                resources,
                decode_feature,
            )?))
        };
        let max_distance = field(v, "max_distance_from_center")?;
        let max_distance = if max_distance.is_number() {
            let n = int_range(max_distance, 1, 128)?;
            [n, n]
        } else {
            [
                int_range(field(max_distance, "horizontal")?, 1, 128)?,
                optional(max_distance, "vertical", 4064, |v| int_range(v, 1, 4064))?,
            ]
        };
        let adaptation = optional(v, "terrain_adaptation", TerrainAdjustment::None, |v| {
            TerrainAdjustment::decode(string(v)?)
        })?;
        let edge = if adaptation == TerrainAdjustment::None {
            0
        } else {
            12
        };
        if max_distance[0] + edge > 128 {
            return Err(Error(
                "Horizontal structure size including terrain adaptation must not exceed 128".into(),
            ));
        }
        let padding = v
            .get("dimension_padding")
            .map(|v| {
                if v.is_number() {
                    let n = int_range(v, 0, i32::MAX)?;
                    Ok::<_, Error>([n, n])
                } else {
                    Ok([
                        v.get("bottom")
                            .and_then(|v| int_range(v, 0, i32::MAX).ok())
                            .unwrap_or(0),
                        v.get("top")
                            .and_then(|v| int_range(v, 0, i32::MAX).ok())
                            .unwrap_or(0),
                    ])
                }
            })
            .transpose()?;
        Ok(Self {
            start_pool,
            start_jigsaw_name: v.get("start_jigsaw_name").map(id).transpose()?,
            max_depth: int_range(field(v, "size")?, 0, 20)?,
            start_height: HeightProvider::decode(field(v, "start_height")?)?,
            expansion_hack: boolean(field(v, "use_expansion_hack")?)?,
            project_start: v
                .get("project_start_to_heightmap")
                .map(|v| match string(v)? {
                    "WORLD_SURFACE_WG" => Ok(Heightmap::WorldSurfaceWg),
                    "WORLD_SURFACE" => Ok(Heightmap::WorldSurface),
                    "OCEAN_FLOOR_WG" => Ok(Heightmap::OceanFloorWg),
                    "OCEAN_FLOOR" => Ok(Heightmap::OceanFloor),
                    "MOTION_BLOCKING" => Ok(Heightmap::MotionBlocking),
                    "MOTION_BLOCKING_NO_LEAVES" => Ok(Heightmap::MotionBlockingNoLeaves),
                    s => Err(Error(format!("unknown heightmap {s}"))),
                })
                .transpose()?,
            max_distance,
            aliases: optional(v, "pool_aliases", Vec::new(), |v| {
                list(v)?.iter().map(PoolAliasBinding::decode).collect()
            })?,
            padding,
            liquid: optional(
                v,
                "liquid_settings",
                LiquidSettings::ApplyWaterlogging,
                LiquidSettings::decode,
            )?,
        })
    }
    /// Port of JigsawStructure#findGenerationPoint. The returned stub still
    /// needs Structure.GenerationContext#isValidBiome at its final position.
    #[allow(clippy::too_many_arguments)]
    pub fn find_generation_point(
        &self,
        pools: &StructurePools,
        templates: &StructureTemplateManager,
        seed: i64,
        chunk: [i32; 2],
        context: WorldGenerationContext,
        dimension_min_y: i32,
        dimension_height: i32,
        height: &mut impl FnMut(i32, i32, Heightmap) -> Result<i32, Error>,
        could_exist: &mut impl FnMut(i32, i32, i32, i32) -> Result<bool, Error>,
    ) -> Result<Option<JigsawStart>, Error> {
        let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(0));
        random.set_large_feature_seed(seed, chunk[0], chunk[1]);
        let y = self.start_height.sample(&mut random, context);
        let position = [chunk[0].wrapping_mul(16), y, chunk[1].wrapping_mul(16)];
        let aliases = PoolAliasLookup::create(&self.aliases, position, seed)?;
        let config = JigsawPlacement {
            start_pool: self.start_pool.clone(),
            start_jigsaw: self.start_jigsaw_name.clone(),
            max_depth: self.max_depth,
            position,
            expansion_hack: self.expansion_hack,
            project_start_to_heightmap: self.project_start,
            max_distance: self.max_distance,
            dimension_min_y,
            dimension_max_y: dimension_min_y + dimension_height - 1,
            padding: self.padding,
            liquid: self.liquid,
        };
        config.add_pieces(pools, templates, &mut random, &aliases, height, could_exist)
    }
}
