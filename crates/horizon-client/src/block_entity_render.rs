//! What the server keeps of the block-entity renderers (`BlockEntityRenderers`): the baked
//! `LayerDefinitions` model parts (`entity-models.json`), the dye colours and the End gateway's
//! faces, whose count sets how many portal particles a gateway shows.
use crate::world::World;
use horizon_core::geometry::Direction;
use horizon_world::{
    block_render::FaceVisibility,
    registry::{BlockRegistry, BlockStateId},
};
use serde_json::Value;
use std::{path::Path, sync::Mutex};

/// One baked `ModelPart`: initial pose, textured polygons (pixel units,
/// normalized UVs) and named children.
#[derive(Clone, Debug, Default)]
pub struct ModelPart {
    /// x, y, z, xRot, yRot, zRot, xScale, yScale, zScale.
    pub pose: [f32; 9],
    pub polygons: Vec<[[f32; 5]; 4]>,
    pub children: Vec<(String, ModelPart)>,
}
impl ModelPart {
    pub fn parse(v: &Value) -> Self {
        let mut pose = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        if let Some(a) = v["pose"].as_array() {
            for (p, x) in pose.iter_mut().zip(a) {
                *p = x.as_f64().unwrap_or(0.0) as f32;
            }
        }
        let polygons = v["polygons"]
            .as_array()
            .map(|polys| {
                polys
                    .iter()
                    .filter_map(|poly| {
                        let verts = poly.as_array()?;
                        if verts.len() != 4 {
                            return None;
                        }
                        let mut out = [[0f32; 5]; 4];
                        for (o, vert) in out.iter_mut().zip(verts) {
                            for (c, x) in o.iter_mut().zip(vert.as_array()?) {
                                *c = x.as_f64().unwrap_or(0.0) as f32;
                            }
                        }
                        Some(out)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let children = v["children"]
            .as_object()
            .map(|m| m.iter().map(|(k, c)| (k.clone(), Self::parse(c))).collect())
            .unwrap_or_default();
        Self {
            pose,
            polygons,
            children,
        }
    }
    pub fn child(&self, name: &str) -> Option<&ModelPart> {
        self.children
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c)
    }
}

/// `DyeColor` names and texture diffuse colours, by id.
pub const DYES: [(&str, u32); 16] = [
    ("white", 0xF9FFFE),
    ("orange", 0xF9801D),
    ("magenta", 0xC74EBD),
    ("light_blue", 0x3AB3DA),
    ("yellow", 0xFED83D),
    ("lime", 0x80C71F),
    ("pink", 0xF38BAA),
    ("gray", 0x474F52),
    ("light_gray", 0x9D9D97),
    ("cyan", 0x169C9C),
    ("purple", 0x8932B8),
    ("blue", 0x3C44AA),
    ("brown", 0x835432),
    ("green", 0x5E7C16),
    ("red", 0xB02E26),
    ("black", 0x1D1D21),
];
pub struct BlockEntityRenderer {
    /// `Block.shouldRenderFace` for End gateway faces (its occlusion pair cache needs a lock).
    visibility: Mutex<FaceVisibility>,
}

impl BlockEntityRenderer {
    pub fn load(root: &Path, registry: &BlockRegistry) -> crate::Result<Self> {
        Ok(Self {
            visibility: Mutex::new(crate::models::load_visibility(root, registry)?),
        })
    }
    /// `TheEndGatewayBlockEntity.shouldRenderFace` for each face of the gateway
    /// at `pos`: the faces its portal is drawn on.
    pub fn gateway_faces(
        &self,
        world: &World,
        state: BlockStateId,
        pos: [i32; 3],
    ) -> Vec<Direction> {
        Direction::ALL
            .into_iter()
            .filter(|&d| {
                let [dx, dy, dz] = d.normal();
                let neighbor = world.block(pos[0] + dx, pos[1] + dy, pos[2] + dz);
                self.visibility
                    .lock()
                    .map(|mut v| {
                        v.should_render_face(state, neighbor, d, true)
                            .unwrap_or(true)
                    })
                    .unwrap_or(true)
            })
            .collect()
    }
}
