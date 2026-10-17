//! Port of Blender's height, density and biome blending. Historical chunk IO
//! supplies BlendingData; border ticks belong to later stages.
use crate::{Error, Identifier, density::DensityVolume};
use ferraloom_util::{
    mth,
    noise::{Noise, NoiseStack, NormalNoise},
    random::XoroshiroRandomSource,
};
use std::{collections::BTreeMap, sync::Arc};

/// Port of BlendingData's sixteen perimeter columns. Densities are unscaled
/// column values; Blender multiplies them by 0.1 exactly as vanilla does.
#[derive(Clone, Debug)]
pub struct BlendingData {
    pub min_section: i32,
    pub max_section: i32,
    pub heights: [f32; 16],
    pub densities: [Option<Vec<f32>>; 16],
    pub biomes: [Option<Vec<Option<Identifier>>>; 16],
}
impl BlendingData {
    pub fn validate(&self) -> Result<(), Error> {
        if self.max_section <= self.min_section {
            return Err(Error("empty old-generation range".into()));
        }
        let sections = (self.max_section - self.min_section) as usize;
        if self
            .densities
            .iter()
            .flatten()
            .any(|column| column.len() != sections * 2)
            || self
                .biomes
                .iter()
                .flatten()
                .any(|column| column.len() != sections * 4)
        {
            return Err(Error(
                "blending column length differs from old generation height".into(),
            ));
        }
        Ok(())
    }
    fn index(x: i32, z: i32) -> Option<usize> {
        if !(0..=4).contains(&x) || !(0..=4).contains(&z) {
            return None;
        }
        if x == 4 || z == 4 {
            Some((7 + x + 4 - z) as usize)
        } else if x == 0 || z == 0 {
            Some((3 - x + z) as usize)
        } else {
            None
        }
    }
    fn height(&self, x: i32, z: i32) -> f32 {
        Self::index(x, z).map_or(f32::MAX, |i| self.heights[i])
    }
    fn density(&self, x: i32, y: i32, z: i32) -> f32 {
        let min = self.min_section * 2;
        if y == min {
            return 0.1;
        }
        Self::index(x, z)
            .and_then(|i| self.densities[i].as_ref())
            .and_then(|column| {
                usize::try_from(y - (min + 1))
                    .ok()
                    .and_then(|i| column.get(i))
            })
            .map_or(f32::MAX, |v| *v * 0.1)
    }
}
fn column_position(index: usize) -> [i32; 2] {
    let i = index as i32;
    if i < 7 {
        [(3 - i).max(0), (i - 3).max(0)]
    } else {
        let i = i - 7;
        [4 - (4 - i).max(0), 4 - (i - 4).max(0)]
    }
}
#[derive(Clone, Debug)]
pub struct Blender {
    data: BTreeMap<[i32; 2], Arc<BlendingData>>,
    height_order: Vec<[i32; 2]>,
    density_order: Vec<[i32; 2]>,
    shift: NoiseStack,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendingOutput {
    pub alpha: f32,
    pub offset: f32,
}
impl Blender {
    /// Equivalent to Blender#of after historical chunks have supplied their
    /// sampled columns. Chunk insertion and fastutil iteration order are kept,
    /// because weighted double accumulation can affect the final float bits.
    pub fn new(
        center: [i32; 2],
        chunks: impl IntoIterator<Item = ([i32; 2], BlendingData)>,
    ) -> Result<Self, Error> {
        let mut data = BTreeMap::new();
        for (position, chunk) in chunks {
            chunk.validate()?;
            let dx = position[0] - center[0];
            let dz = position[1] - center[1];
            if dx.abs() <= 7 && dz.abs() <= 7 && dx * dx + dz * dz <= 64 {
                data.insert(position, Arc::new(chunk));
            }
        }
        let insert = data.keys().copied().collect::<Vec<_>>();
        let height_order = fastutil_order(&insert);
        let density_order = fastutil_order(
            &insert
                .iter()
                .filter(|position| {
                    (position[0] - center[0]).abs() <= 1 && (position[1] - center[1]).abs() <= 1
                })
                .copied()
                .collect::<Vec<_>>(),
        );
        let shift = NormalNoise::create_parity(-3, vec![1.0, 1.0, 1.0, 0.0])
            .create(&mut XoroshiroRandomSource::new(42));
        Ok(Self {
            data,
            height_order,
            density_order,
            shift,
        })
    }
    /// Port of Blender#createAroundOldChunksCarvingMaskFilter. The vanilla
    /// filter admits samples within four jittered blocks of old generation.
    pub fn can_carve(&self, center: [i32; 2], x: u8, y: i32, z: u8) -> bool {
        let old = self
            .data
            .iter()
            .filter(|(p, _)| (p[0] - center[0]).abs() <= 1 && (p[1] - center[1]).abs() <= 1)
            .collect::<Vec<_>>();
        if old.is_empty() {
            return true;
        }
        let (x, y, z) = (f64::from(x), f64::from(y), f64::from(z));
        let shifted = [
            x + 0.5 + f64::from(self.shift.get(x, y, z) * 4.0),
            y + 0.5 + f64::from(self.shift.get(y, z, x) * 4.0),
            z + 0.5 + f64::from(self.shift.get(z, x, y) * 4.0),
        ];
        old.iter().any(|(p, data)| {
            let radius_y = f64::from((data.max_section - data.min_section) * 16) / 2.0;
            let center_y = f64::from(data.min_section * 16) + radius_y;
            let dx = ((shifted[0] - 8.0 - f64::from((p[0] - center[0]) * 16)).abs() - 8.0).max(0.0);
            let dy = ((shifted[1] - center_y).abs() - radius_y).max(0.0);
            let dz = ((shifted[2] - 8.0 - f64::from((p[1] - center[1]) * 16)).abs() - 8.0).max(0.0);
            (dx * dx + dy * dy + dz * dz).sqrt() < 4.0
        })
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    fn fixed(&self, x: i32, y: i32, z: i32, density: bool) -> f32 {
        let chunk = [x >> 2, z >> 2];
        let get = |cx, cz| {
            self.data.get(&[cx, cz]).map_or(f32::MAX, |data| {
                if density {
                    data.density(x - cx * 4, y, z - cz * 4)
                } else {
                    data.height(x - cx * 4, z - cz * 4)
                }
            })
        };
        let mut value = get(chunk[0], chunk[1]);
        if value == f32::MAX {
            if x & 3 == 0 && z & 3 == 0 {
                value = get(chunk[0] - 1, chunk[1] - 1);
            }
            if value == f32::MAX {
                if x & 3 == 0 {
                    value = get(chunk[0] - 1, chunk[1]);
                }
                if value == f32::MAX && z & 3 == 0 {
                    value = get(chunk[0], chunk[1] - 1);
                }
            }
        }
        value
    }
    pub fn blend_offset_and_factor(&self, block_x: i32, block_z: i32) -> BlendingOutput {
        let x = block_x >> 2;
        let z = block_z >> 2;
        let fixed = self.fixed(x, 0, z, false);
        if fixed != f32::MAX {
            return BlendingOutput {
                alpha: 0.0,
                offset: height_to_offset(f64::from(fixed)) as f32,
            };
        }
        let mut total = 0.0;
        let mut weighted = 0.0;
        let mut closest = f64::INFINITY;
        for position in &self.height_order {
            let data = &self.data[position];
            for i in 0..16 {
                let height = data.heights[i];
                if height == f32::MAX {
                    continue;
                }
                let p = column_position(i);
                let dx = x - position[0] * 4 - p[0];
                let dz = z - position[1] * 4 - p[1];
                let [dx, dz] = [dx, dz].map(|v| v as f32);
                let distance = f64::from(f64::from(dx * dx + dz * dz).sqrt() as f32);
                if distance > 27.0 {
                    continue;
                }
                closest = closest.min(distance);
                let weight = 1.0 / (distance * distance * distance * distance);
                weighted += f64::from(height) * weight;
                total += weight;
            }
        }
        if closest == f64::INFINITY {
            return BlendingOutput {
                alpha: 1.0,
                offset: 0.0,
            };
        }
        let alpha = (closest / 28.0).clamp(0.0, 1.0);
        let alpha = 3.0 * alpha * alpha - 2.0 * alpha * alpha * alpha;
        BlendingOutput {
            alpha: alpha as f32,
            offset: height_to_offset(weighted / total) as f32,
        }
    }
    pub fn blend_density(&self, p: [i32; 3], noise: f32) -> f32 {
        let [x, y, z] = [p[0] >> 2, p[1] / 8, p[2] >> 2];
        let fixed = self.fixed(x, y, z, true);
        if fixed != f32::MAX {
            return fixed;
        }
        let mut total = 0.0;
        let mut weighted = 0.0;
        let mut closest = f64::INFINITY;
        for position in &self.density_order {
            let data = &self.data[position];
            let min = data.min_section * 2 + 1;
            let from = (y - 1 - min).max(0);
            let to = ((data.max_section - data.min_section) * 2).min(y + 1 - min);
            for (i, column) in data.densities.iter().enumerate() {
                let Some(column) = column else { continue };
                let local = column_position(i);
                let tx = position[0] * 4 + local[0];
                let tz = position[1] * 4 + local[1];
                for index in from..to {
                    let ty = index + min;
                    let [dx, dy, dz] = [x - tx, (y - ty) * 2, z - tz].map(|v| v as f32);
                    let distance = f64::from(f64::from(dx * dx + dy * dy + dz * dz).sqrt() as f32);
                    if distance > 2.0 {
                        continue;
                    }
                    closest = closest.min(distance);
                    let weight = 1.0 / (distance * distance * distance * distance);
                    weighted += f64::from(column[index as usize] * 0.1) * weight;
                    total += weight;
                }
            }
        }
        if closest == f64::INFINITY {
            noise
        } else {
            mth::lerp(
                (closest / 3.0).clamp(0.0, 1.0),
                weighted / total,
                f64::from(noise),
            ) as f32
        }
    }
    pub fn blend_biome(&self, quart: [i32; 3]) -> Option<Identifier> {
        let mut closest = f32::INFINITY;
        let mut selected = None;
        for position in &self.height_order {
            let data = &self.data[position];
            let min = data.min_section * 4;
            if quart[1] < min || quart[1] >= data.max_section * 4 {
                continue;
            }
            for (i, column) in data.biomes.iter().enumerate() {
                let Some(biome) = column
                    .as_ref()
                    .and_then(|c| c[(quart[1] - min) as usize].as_ref())
                else {
                    continue;
                };
                let p = column_position(i);
                let [dx, dz] = [
                    quart[0] - position[0] * 4 - p[0],
                    quart[2] - position[1] * 4 - p[1],
                ]
                .map(|n| n as f32);
                let distance = f64::from(dx * dx + dz * dz).sqrt() as f32;
                if distance <= 27.0 && distance < closest {
                    closest = distance;
                    selected = Some(biome.clone());
                }
            }
        }
        if closest == f32::INFINITY {
            return None;
        }
        let shift = self
            .shift
            .get(f64::from(quart[0]), 0.0, f64::from(quart[2]))
            * 12.0;
        if ((closest + shift) / 28.0).clamp(0.0, 1.0) > 0.5 {
            None
        } else {
            selected
        }
    }
    pub fn output_buffer(self: &Arc<Self>, volume: DensityVolume) -> BlendingOutputBuffer {
        let min = [volume.min[0] >> 2, volume.min[2] >> 2];
        let max = [
            (volume.min[0] + (volume.size[0] as i32 - 1) * volume.step[0]) >> 2,
            (volume.min[2] + (volume.size[2] as i32 - 1) * volume.step[2]) >> 2,
        ];
        let size = [
            (max[0] - min[0] + 2) as usize,
            (max[1] - min[1] + 2) as usize,
        ];
        let mut values = Vec::new();
        for z in 0..size[1] {
            for x in 0..size[0] {
                values.push(
                    self.blend_offset_and_factor((min[0] + x as i32) * 4, (min[1] + z as i32) * 4),
                );
            }
        }
        BlendingOutputBuffer {
            blender: self.clone(),
            min,
            size,
            values,
        }
    }
}
#[derive(Clone, Debug)]
pub struct BlendingOutputBuffer {
    pub blender: Arc<Blender>,
    min: [i32; 2],
    size: [usize; 2],
    values: Vec<BlendingOutput>,
}
impl BlendingOutputBuffer {
    pub fn get(&self, x: i32, z: i32) -> BlendingOutput {
        let local = [(x >> 2) - self.min[0], (z >> 2) - self.min[1]];
        if (0..2).all(|i| local[i] >= 0 && (local[i] as usize) < self.size[i]) {
            self.values[local[1] as usize * self.size[0] + local[0] as usize]
        } else {
            self.blender.blend_offset_and_factor(x, z)
        }
    }
}
fn height_to_offset(height: f64) -> f64 {
    let target = height + 0.5;
    let modulo = mth::positive_modulo(target, 8.0);
    (32.0 * (target - 128.0) - 3.0 * (target - 120.0) * modulo + 3.0 * modulo * modulo)
        / (128.0 * (32.0 - 3.0 * modulo))
}
fn pack(position: [i32; 2]) -> u64 {
    (position[0] as u32 as u64) | ((position[1] as u32 as u64) << 32)
}
fn mix(value: u64) -> usize {
    let value = value.wrapping_mul(0x9E3779B97F4A7C15);
    let value = value ^ (value >> 32);
    (value ^ (value >> 16)) as usize
}
fn fastutil_order(insert: &[[i32; 2]]) -> Vec<[i32; 2]> {
    fn put(table: &mut [Option<[i32; 2]>], position: [i32; 2]) {
        let mut index = mix(pack(position)) & (table.len() - 1);
        while table[index].is_some() {
            index = (index + 1) & (table.len() - 1);
        }
        table[index] = Some(position);
    }
    let mut table = vec![None; 32];
    let mut size = 0;
    let mut zero = false;
    for &position in insert {
        if pack(position) == 0 {
            zero = true;
        } else {
            put(&mut table, position);
        }
        size += 1;
        if size > table.len() * 3 / 4 {
            let mut new = vec![None; table.len() * 2];
            for position in table.into_iter().rev().flatten() {
                put(&mut new, position);
            }
            table = new;
        }
    }
    let mut order = Vec::new();
    if zero {
        order.push([0, 0]);
    }
    order.extend(table.into_iter().rev().flatten());
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    #[test]
    fn nonempty_blending_matches_vanilla_weighting_boundaries_and_biomes() {
        let chunks = [[-1, -1], [-1, 0], [0, -1], [0, 0], [2, 1]].map(|[cx, cz]| {
            let min = if cx < 0 { -4 } else { 0 };
            let heights = std::array::from_fn(|i| {
                if (i as i32 + cx + cz) & 3 == 0 {
                    f32::MAX
                } else {
                    (20 + cx * 9 + cz * 3) as f32 + i as f32 * 1.5
                }
            });
            let densities = std::array::from_fn(|i| {
                if i % 3 == 0 {
                    None
                } else {
                    Some(
                        (0..8)
                            .map(|y| ((i as i32 * 17 + y * 5 + cx - cz) % 29 - 14) as f32 / 4.0)
                            .collect(),
                    )
                }
            });
            let biomes = std::array::from_fn(|i| {
                if i % 2 != 0 {
                    None
                } else {
                    Some(
                        (0..16)
                            .map(|y| {
                                Some(
                                    if (i as i32 + y + cx) & 1 == 0 {
                                        "plains"
                                    } else {
                                        "desert"
                                    }
                                    .parse()
                                    .unwrap(),
                                )
                            })
                            .collect(),
                    )
                }
            });
            (
                [cx, cz],
                BlendingData {
                    min_section: min,
                    max_section: min + 4,
                    heights,
                    densities,
                    biomes,
                },
            )
        });
        let blender = Arc::new(Blender::new([0, 0], chunks).unwrap());
        let buffer = blender.output_buffer(DensityVolume {
            size: [16, 64, 16],
            min: [-16, -64, -16],
            step: [1; 3],
        });
        let mut text = String::new();
        flate2::read::GzDecoder::new(include_bytes!("../tests/golden/blending.json.gz").as_slice())
            .read_to_string(&mut text)
            .unwrap();
        let cases: serde_json::Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let p = std::array::from_fn(|i| case[0][i].as_i64().unwrap() as i32);
            let output = blender.blend_offset_and_factor(p[0], p[2]);
            for (value, index) in [
                (output.alpha, 1),
                (output.offset, 2),
                (blender.blend_density(p, 0.12345), 3),
                (buffer.get(p[0], p[2]).alpha, 5),
                (buffer.get(p[0], p[2]).offset, 6),
            ] {
                assert_eq!(
                    value.to_bits(),
                    case[index].as_u64().unwrap() as u32,
                    "point={p:?} field={index} actual={value}"
                );
            }
            assert_eq!(
                blender
                    .blend_biome(p.map(|v| v >> 2))
                    .map(|id| id.to_string())
                    .as_deref(),
                case[4].as_str(),
                "point={p:?} biome"
            );
        }
    }
}
