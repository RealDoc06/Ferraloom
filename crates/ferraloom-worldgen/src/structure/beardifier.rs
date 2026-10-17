use super::BoundingBox;
use crate::{Error, density::DensityVolume};
use ferraloom_util::mth;
use std::sync::OnceLock;

/// Port of TerrainAdjustment, including 26.3's encapsulate adjustment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerrainAdjustment {
    None,
    Bury,
    BeardThin,
    BeardBox,
    Encapsulate,
}
impl TerrainAdjustment {
    pub fn decode(value: &str) -> Result<Self, Error> {
        match value {
            "none" => Ok(Self::None),
            "bury" => Ok(Self::Bury),
            "beard_thin" => Ok(Self::BeardThin),
            "beard_box" => Ok(Self::BeardBox),
            "encapsulate" => Ok(Self::Encapsulate),
            _ => Err(Error(format!("unknown terrain adjustment {value}"))),
        }
    }
}
/// Port of StructureTemplatePool.Projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Projection {
    Rigid,
    TerrainMatching,
}
/// Port of JigsawJunction. The destination projection is persisted with pieces.
#[derive(Clone, Copy, Debug)]
pub struct JigsawJunction {
    pub source: [i32; 3],
    pub delta_y: i32,
    pub destination_projection: Projection,
}
/// Port of Beardifier.Rigid.
#[derive(Clone, Copy, Debug)]
pub struct Rigid {
    pub bounds: BoundingBox,
    pub adjustment: TerrainAdjustment,
    pub ground_level_delta: i32,
}
/// Borrowed piece data used by Beardifier#forStructuresInChunk. `projection`
/// is None for handwritten pieces, Some for pool pieces (including matching).
#[derive(Clone, Copy, Debug)]
pub struct TerrainPiece<'a> {
    pub bounds: BoundingBox,
    pub adjustment: TerrainAdjustment,
    pub ground_level_delta: i32,
    pub projection: Option<Projection>,
    pub junctions: &'a [JigsawJunction],
}
/// Port of Beardifier. Immutable and Send + Sync; one Arc may be shared across
/// noise sampler contexts, while all density caches remain job-local.
#[derive(Clone, Debug, Default)]
pub struct Beardifier {
    pieces: Vec<Rigid>,
    junctions: Vec<JigsawJunction>,
    affected_box: Option<BoundingBox>,
}
impl Beardifier {
    /// Port of Beardifier's test constructor. `affected_box` is already inflated.
    pub fn new(
        pieces: Vec<Rigid>,
        junctions: Vec<JigsawJunction>,
        affected_box: Option<BoundingBox>,
    ) -> Self {
        Self {
            pieces,
            junctions,
            affected_box,
        }
    }
    /// Iteration order must be StructureManager starts order then piece order;
    /// floating-point accumulation makes that order observable.
    pub fn for_chunk<'a>(
        chunk: [i32; 2],
        pieces: impl IntoIterator<Item = TerrainPiece<'a>>,
    ) -> Self {
        let mut result = Self::default();
        let mut bounds: Option<BoundingBox> = None;
        let min = chunk.map(|v| v.wrapping_mul(16));
        for piece in pieces {
            if piece.adjustment == TerrainAdjustment::None
                || !piece.bounds.is_close_to_chunk(chunk, 12)
            {
                continue;
            }
            if piece.projection.is_none_or(|p| p == Projection::Rigid) {
                result.pieces.push(Rigid {
                    bounds: piece.bounds,
                    adjustment: piece.adjustment,
                    ground_level_delta: if piece.projection.is_none() {
                        0
                    } else {
                        piece.ground_level_delta
                    },
                });
                bounds = Some(bounds.map_or(piece.bounds, |b| b.encapsulating(piece.bounds)));
            }
            if piece.projection.is_some() {
                for &junction in piece.junctions {
                    let [x, _, z] = junction.source;
                    if x > min[0] - 12 && z > min[1] - 12 && x < min[0] + 27 && z < min[1] + 27 {
                        result.junctions.push(junction);
                        let point = BoundingBox::point(junction.source);
                        bounds = Some(bounds.map_or(point, |b| b.encapsulating(point)));
                    }
                }
            }
        }
        result.affected_box = bounds.map(|b| b.inflated(24));
        result
    }
    pub fn sample_value(&self, p: [i32; 3]) -> f32 {
        if self.affected_box.is_some_and(|b| b.contains(p)) {
            self.sample_unchecked(p)
        } else {
            0.0
        }
    }
    /// Volume bounds deliberately use vanilla's floor division at the lower
    /// edge. For steps > 1 this can sample one position below affected_box.
    pub fn sample_volume(&self, volume: DensityVolume) -> Vec<f32> {
        let mut out = vec![0.0; volume.len()];
        let Some(bounds) = self.affected_box else {
            return out;
        };
        if volume.is_empty() {
            return out;
        }
        let max =
            std::array::from_fn(|i| volume.min[i] + volume.size[i] as i32 * volume.step[i] - 1);
        if !bounds.intersects(BoundingBox::new(volume.min, max)) {
            return out;
        }
        let start: [i32; 3] = std::array::from_fn(|i| {
            (bounds.min[i] - volume.min[i])
                .max(0)
                .div_euclid(volume.step[i])
        });
        let end: [i32; 3] = std::array::from_fn(|i| {
            (volume.size[i] as i32 - 1)
                .min((bounds.max[i] - volume.min[i]).div_euclid(volume.step[i]))
        });
        for z in start[2]..=end[2] {
            for x in start[0]..=end[0] {
                for y in start[1]..=end[1] {
                    let index = volume.index(x as usize, y as usize, z as usize);
                    out[index] = self.sample_unchecked(volume.position(index));
                }
            }
        }
        out
    }
    fn sample_unchecked(&self, [x, y, z]: [i32; 3]) -> f32 {
        let mut result = 0.0;
        for rigid in &self.pieces {
            let b = rigid.bounds;
            let dx = 0.max(b.min[0] - x).max(x - b.max[0]);
            let dz = 0.max(b.min[2] - z).max(z - b.max[2]);
            let ground_y = b.min[1] + rigid.ground_level_delta;
            let dy_ground = y - ground_y;
            let dy = match rigid.adjustment {
                TerrainAdjustment::None => 0,
                TerrainAdjustment::Bury | TerrainAdjustment::BeardThin => dy_ground,
                TerrainAdjustment::BeardBox => 0.max(ground_y - y).max(y - b.max[1]),
                TerrainAdjustment::Encapsulate => 0.max(b.min[1] - y).max(y - b.max[1]),
            };
            result += match rigid.adjustment {
                TerrainAdjustment::None => 0.0,
                TerrainAdjustment::Bury => bury(dx as f32, dy as f32 / 2.0, dz as f32),
                TerrainAdjustment::BeardThin | TerrainAdjustment::BeardBox => {
                    beard(dx, dy, dz, dy_ground) * 0.8
                }
                TerrainAdjustment::Encapsulate => {
                    bury(dx as f32 / 2.0, dy as f32 / 2.0, dz as f32 / 2.0) * 0.8
                }
            };
        }
        for junction in &self.junctions {
            let [jx, jy, jz] = junction.source;
            let dy = y - jy;
            result += beard(x - jx, dy, z - jz, dy) * 0.4;
        }
        result
    }
}
fn bury(dx: f32, dy: f32, dz: f32) -> f32 {
    let square = mth::length_squared_f32(dx, dy, dz);
    if square >= 36.0 {
        0.0
    } else {
        1.0 - square.sqrt() / 6.0
    }
}
fn beard(dx: i32, dy: i32, dz: i32, y_to_ground: i32) -> f32 {
    let [xi, yi, zi] = [dx + 12, dy + 12, dz + 12];
    if !(0..24).contains(&xi) || !(0..24).contains(&yi) || !(0..24).contains(&zi) {
        return 0.0;
    }
    static KERNEL: OnceLock<[f32; 13824]> = OnceLock::new();
    let kernel = KERNEL.get_or_init(|| {
        std::array::from_fn(|i| {
            let x = (i / 24 % 24) as f64 - 12.0;
            let y = (i % 24) as f64 - 12.0 + 0.5;
            let z = (i / (24 * 24)) as f64 - 12.0;
            std::f64::consts::E.powf(-mth::length_squared(x, y, z) / 16.0) as f32
        })
    });
    let dy = y_to_ground as f32 + 0.5;
    let square = mth::length_squared_f32(dx as f32, dy, dz as f32);
    let value = -dy * mth::fast_inv_sqrt(f64::from(square / 2.0)) as f32 / 2.0;
    value * kernel[(zi * 24 * 24 + xi * 24 + yi) as usize]
}

#[cfg(test)]
#[path = "beardifier_tests.rs"]
mod tests;
