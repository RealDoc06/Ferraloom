use super::*;
use ferraloom_util::{
    BlockPos, java, mth,
    noise::{Noise, NoiseStack, SimplexNoise, SmearedPerlinNoise},
    random::{Algorithm, LegacyRandomSource, PositionalRandomFactory, RandomSource},
};
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NodeId(usize);

/// Chunk-local equivalent of SamplerContext. A cache holds one volume and one
/// scalar value independently, including vanilla's packed BlockPos key aliasing.
#[derive(Default)]
pub struct SamplerContext {
    enabled: bool,
    cells: HashMap<NodeId, CacheCell>,
}
#[derive(Default)]
struct CacheCell {
    volume: Option<(DensityVolume, Vec<f32>)>,
    scalar: Option<(i64, f32)>,
}
impl SamplerContext {
    pub fn cached() -> Self {
        Self {
            enabled: true,
            cells: HashMap::new(),
        }
    }
    pub fn uncached() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        self.cells.clear();
    }
}

#[derive(Debug)]
struct Compiled {
    definition: Function,
    children: Vec<NodeId>,
    noise: Option<NoiseStack>,
    island: Option<SimplexNoise>,
    blended: Option<[NoiseStack; 3]>,
    spline: Option<CompiledSpline>,
    axes: u8,
}
#[derive(Debug)]
enum CompiledSpline {
    Constant(f32),
    Multipoint {
        coordinate: NodeId,
        points: Vec<(f32, CompiledSpline, f32)>,
    },
}

/// Port of DensityFunctionCompiler and the seed wiring from RandomState.
/// Compilation requires `&mut`; evaluation only borrows the graph with `&`.
#[derive(Debug)]
pub struct DensityFunctionCompiler {
    seed: i64,
    legacy: bool,
    random: PositionalRandomFactory,
    registry: Arc<DensityRegistry>,
    nodes: Vec<Compiled>,
    memo: Vec<(Function, u8, NodeId)>,
    caches: Vec<(Function, NodeId)>,
}
impl DensityFunctionCompiler {
    pub fn new(registry: Arc<DensityRegistry>, seed: i64, legacy: bool) -> Self {
        let random = if legacy {
            Algorithm::Legacy
        } else {
            Algorithm::Xoroshiro
        }
        .new_instance(seed)
        .fork_positional();
        Self {
            seed,
            legacy,
            random,
            registry,
            nodes: Vec::new(),
            memo: Vec::new(),
            caches: Vec::new(),
        }
    }
    pub fn positional_random(&self, name: &Identifier) -> PositionalRandomFactory {
        self.random
            .from_hash_of(&name.to_string())
            .fork_positional()
    }
    pub fn compile(&mut self, function: &Function) -> Result<NodeId, Error> {
        self.axes(function, &mut BTreeSet::new())?;
        self.compile_with_axes(function, 7)
    }
    pub fn compile_reference(&mut self, id: &Identifier) -> Result<NodeId, Error> {
        self.compile(&Arc::new(DensityFunction::Reference(id.clone())))
    }
    fn axes(&self, f: &Function, seen: &mut BTreeSet<Identifier>) -> Result<u8, Error> {
        use DensityFunction as D;
        Ok(match f.as_ref() {
            D::Reference(id) => {
                if !seen.insert(id.clone()) {
                    return Err(Error(format!("cyclic density reference {id}")));
                }
                let value = self
                    .registry
                    .functions
                    .get(id)
                    .ok_or_else(|| Error(format!("unknown density {id}")))?;
                let axes = self.axes(value, seen)?;
                seen.remove(id);
                axes
            }
            D::Constant(_) => 0,
            D::Simple(Simple::Beardifier) => 7,
            D::Simple(_) | D::EndOuterIslands => 5,
            D::Gradient { axis, .. } => 1 << axis.index(),
            D::Noise {
                xz_scale,
                y_scale,
                shift,
                ..
            } => {
                let mut axes = if *xz_scale == 0.0 { 0 } else { 5 };
                if *y_scale != 0.0 {
                    axes |= 2;
                }
                for s in shift {
                    axes |= self.axes(s, seen)?;
                }
                axes
            }
            D::Shift {
                kind: Shift::All, ..
            }
            | D::DistanceToPoint { .. }
            | D::OldBlendedNoise { .. } => 7,
            D::Shift { .. } => 5,
            D::Slice { input, axis, .. } => self.axes(input, seen)? & !(1 << axis.index()),
            D::FindTopSurface { density, upper, .. } => {
                (self.axes(density, seen)? | self.axes(upper, seen)?) & !2
            }
            D::Spline(s) => self.spline_axes(s, seen)?,
            _ => {
                let mut axes = 0;
                for child in children(f) {
                    axes |= self.axes(child, seen)?;
                }
                axes
            }
        })
    }
    fn spline_axes(&self, s: &CubicSpline, seen: &mut BTreeSet<Identifier>) -> Result<u8, Error> {
        match s {
            CubicSpline::Constant(_) => Ok(0),
            CubicSpline::Multipoint { coordinate, points } => {
                let mut axes = self.axes(coordinate, seen)?;
                for p in points {
                    axes |= self.spline_axes(&p.value, seen)?;
                }
                Ok(axes)
            }
        }
    }
    fn push(&mut self, node: Compiled) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        id
    }
    fn compile_with_axes(&mut self, f: &Function, parent: u8) -> Result<NodeId, Error> {
        use DensityFunction as D;
        if let D::Reference(id) = f.as_ref() {
            let value = self
                .registry
                .functions
                .get(id)
                .ok_or_else(|| Error(format!("unknown density {id}")))?
                .clone();
            return self.compile_with_axes(&value, parent);
        }
        if let Some((_, _, id)) = self
            .memo
            .iter()
            .find(|(def, axes, _)| *axes == parent && def == f)
        {
            return Ok(*id);
        }
        let axes = self.axes(f, &mut BTreeSet::new())?;
        let mut id = if let D::Cache(input) = f.as_ref() {
            if let Some((_, id)) = self.caches.iter().find(|(def, _)| def == input) {
                *id
            } else {
                let child = self.compile_with_axes(input, 7)?;
                let id = self.push(Compiled {
                    definition: f.clone(),
                    children: vec![child],
                    noise: None,
                    island: None,
                    blended: None,
                    spline: None,
                    axes,
                });
                self.caches.push((input.clone(), id));
                id
            }
        } else {
            let children = children(f)
                .into_iter()
                .map(|child| self.compile_with_axes(child, axes))
                .collect::<Result<Vec<_>, _>>()?;
            let noise = match f.as_ref() {
                D::Noise { noise, .. } | D::Shift { noise, .. } => Some(self.create_noise(noise)?),
                _ => None,
            };
            let island = if matches!(f.as_ref(), D::EndOuterIslands) {
                let mut r = LegacyRandomSource::new(self.seed);
                r.consume_count(17292);
                Some(SimplexNoise::with_discarded_offset(&mut r, true))
            } else {
                None
            };
            let blended = if let D::OldBlendedNoise {
                y_scale,
                y_factor,
                smear_scale,
                ..
            } = f.as_ref()
            {
                let mut r = if self.legacy {
                    ferraloom_util::random::Random::Legacy(LegacyRandomSource::new(self.seed))
                } else {
                    self.random.from_hash_of("minecraft:terrain")
                };
                let smear = 684.412 * y_scale * smear_scale;
                Some([
                    create_fbm(&mut r, -15, smear, 0.9999847412109375),
                    create_fbm(&mut r, -15, smear, 0.9999847412109375),
                    create_fbm(&mut r, -7, smear / y_factor, 12.75),
                ])
            } else {
                None
            };
            let spline = if let D::Spline(s) = f.as_ref() {
                Some(self.compile_spline(s, axes)?)
            } else {
                None
            };
            self.push(Compiled {
                definition: f.clone(),
                children,
                noise,
                island,
                blended,
                spline,
                axes,
            })
        };
        if !matches!(f.as_ref(), D::Constant(_) | D::Gradient { .. }) {
            for axis in [Axis::X, Axis::Z, Axis::Y] {
                let bit = 1 << axis.index();
                if parent & !axes & bit != 0
                    && !matches!(self.nodes[id.0].definition.as_ref(),D::Slice {axis:a,..} if *a==axis)
                {
                    let def = Arc::new(D::Slice {
                        input: self.nodes[id.0].definition.clone(),
                        axis,
                        coordinate: 0,
                    });
                    id = self.push(Compiled {
                        definition: def,
                        children: vec![id],
                        noise: None,
                        island: None,
                        blended: None,
                        spline: None,
                        axes,
                    });
                }
            }
        }
        self.memo.push((f.clone(), parent, id));
        Ok(id)
    }
    fn compile_spline(&mut self, s: &CubicSpline, axes: u8) -> Result<CompiledSpline, Error> {
        Ok(match s {
            CubicSpline::Constant(v) => CompiledSpline::Constant(*v),
            CubicSpline::Multipoint { coordinate, points } => CompiledSpline::Multipoint {
                coordinate: self.compile_with_axes(coordinate, axes)?,
                points: points
                    .iter()
                    .map(|p| {
                        Ok((
                            p.location,
                            self.compile_spline(&p.value, axes)?,
                            p.derivative,
                        ))
                    })
                    .collect::<Result<_, Error>>()?,
            },
        })
    }
    fn create_noise(&self, holder: &Holder<NoiseSpec>) -> Result<NoiseStack, Error> {
        let Holder::Reference(id) = holder else {
            return Err(Error(
                "RandomState requires a registry key for noise samplers".into(),
            ));
        };
        let spec = self
            .registry
            .noises
            .get(id)
            .ok_or_else(|| Error(format!("unknown noise {id}")))?;
        let noise = spec.noise();
        if id.to_string() == "minecraft:temperature_nether"
            || id.to_string() == "minecraft:vegetation_nether"
        {
            let offset = i64::from(id.path() == "vegetation_nether");
            Ok(
                noise.create_for_legacy_nether_biome(&mut LegacyRandomSource::new(
                    self.seed.wrapping_add(offset),
                )),
            )
        } else {
            Ok(noise.create(&mut self.random.from_hash_of(&id.to_string())))
        }
    }
    fn constant(&self, id: NodeId) -> Option<f32> {
        match self.nodes[id.0].definition.as_ref() {
            DensityFunction::Constant(v) => Some(*v),
            _ => None,
        }
    }
    pub fn domain_axes(&self, id: NodeId) -> u8 {
        self.nodes[id.0].axes
    }

    pub fn sample_value(&self, id: NodeId, context: &mut SamplerContext, p: [i32; 3]) -> f32 {
        let node = &self.nodes[id.0];
        let c = &node.children;
        let sample = |id, ctx: &mut SamplerContext| self.sample_value(id, ctx, p);
        use DensityFunction as D;
        match node.definition.as_ref() {
            D::Constant(v) => *v,
            D::Simple(Simple::BlendAlpha) => 1.0,
            D::Simple(_) => 0.0,
            D::Reference(_) => unreachable!("compiled references are inlined"),
            D::Noise {
                xz_scale, y_scale, ..
            } => {
                let sx = sample(c[0], context);
                let sy = sample(c[1], context);
                let sz = sample(c[2], context);
                node.noise.as_ref().unwrap().get(
                    f64::from(p[0]) * xz_scale + f64::from(sx),
                    f64::from(p[1]) * y_scale + f64::from(sy),
                    f64::from(p[2]) * xz_scale + f64::from(sz),
                )
            }
            D::Shift { kind, .. } => {
                let [x, y, z] = p.map(|n| f64::from(n) * 0.25);
                let n = node.noise.as_ref().unwrap();
                4.0 * match kind {
                    Shift::All => n.get(x, y, z),
                    Shift::A => n.get(x, 0.0, z),
                    Shift::B => n.get(z, x, 0.0),
                }
            }
            D::EndOuterIslands => end_islands(node.island.as_ref().unwrap(), p[0], p[2]),
            D::DistanceToPoint { point, metric } => distance(
                std::array::from_fn(|i| point[i].wrapping_sub(p[i]) as f32),
                *metric,
            ),
            D::Gradient {
                axis,
                tiling,
                from,
                to,
                from_value,
                to_value,
            } => gradient(p[axis.index()], *tiling, *from, *to, *from_value, *to_value),
            D::Unary { kind, .. } => unary(*kind, sample(c[0], context)),
            D::Binary { kind, .. } => {
                let left = sample(c[0], context);
                if matches!(kind, Binary::Mul | Binary::Div)
                    && left == 0.0
                    && self.constant(c[0]).is_none()
                    && self.constant(c[1]).is_none()
                {
                    return 0.0;
                }
                binary(
                    *kind,
                    left,
                    sample(c[1], context),
                    self.constant(c[0]),
                    self.constant(c[1]),
                    false,
                )
            }
            D::Round { kind, .. } => round(*kind, sample(c[0], context), sample(c[1], context)),
            D::Pow { .. } => power(
                sample(c[0], context),
                sample(c[1], context),
                self.constant(c[0]).is_none() && self.constant(c[1]).is_some(),
            ),
            D::Spline(_) => self.spline_value(node.spline.as_ref().unwrap(), &mut |coordinate| {
                sample(coordinate, context)
            }),
            D::Lerp { .. } => {
                let a = sample(c[0], context);
                if a == 0.0 {
                    sample(c[1], context)
                } else if a == 1.0 {
                    sample(c[2], context)
                } else {
                    mth::lerp_f32(a, sample(c[1], context), sample(c[2], context))
                }
            }
            D::Clamp { min, max, .. } => clamp(sample(c[0], context), *min, *max),
            D::RangeChoice { min, max, .. } => {
                let v = sample(c[0], context);
                sample(if v >= *min && v < *max { c[1] } else { c[2] }, context)
            }
            D::IntervalSelect { thresholds, .. } => {
                let v = sample(c[0], context);
                sample(c[1 + thresholds.partition_point(|t| v >= *t)], context)
            }
            D::Cache(_) => {
                if !context.enabled {
                    return sample(c[0], context);
                }
                let key = BlockPos::new(p[0], p[1], p[2]).as_long();
                if let Some(cell) = context.cells.get(&id) {
                    if let Some((k, v)) = cell.scalar {
                        if k == key && !v.is_nan() {
                            return v;
                        }
                    }
                    if let Some((volume, buffer)) = &cell.volume {
                        if let Some(index) = index_of(*volume, p) {
                            return buffer[index];
                        }
                    }
                }
                let v = sample(c[0], context);
                context.cells.entry(id).or_default().scalar = Some((key, v));
                v
            }
            D::BlendDensity(_) => sample(c[0], context),
            D::Interpolated {
                cell_xz, cell_y, ..
            } => {
                let cell = [*cell_xz, *cell_y, *cell_xz];
                let local = std::array::from_fn::<_, 3, _>(|i| p[i].rem_euclid(cell[i]));
                if local == [0; 3] {
                    return sample(c[0], context);
                }
                let volume = DensityVolume {
                    size: [2; 3],
                    min: std::array::from_fn(|i| p[i] - local[i]),
                    step: cell,
                };
                let b = self.sample_volume(c[0], context, volume);
                let [x, y, z] = std::array::from_fn(|i| local[i] as f32 / cell[i] as f32);
                mth::lerp3_f32(x, y, z, b[0], b[2], b[1], b[3], b[4], b[6], b[5], b[7])
            }
            D::Slice {
                axis, coordinate, ..
            } => {
                let mut p = p;
                p[axis.index()] = *coordinate;
                self.sample_value(c[0], context, p)
            }
            D::FindTopSurface {
                lower, cell_height, ..
            } => {
                let upper = self.sample_value(c[1], context, [p[0], 0, p[2]]);
                self.find_top(c[0], context, p[0], p[2], upper, *lower, *cell_height) as f32
            }
            D::OldBlendedNoise {
                xz_scale,
                y_scale,
                xz_factor,
                y_factor,
                ..
            } => {
                let fbm = node.blended.as_ref().unwrap();
                let x = f64::from(p[0]) * (684.412 * xz_scale);
                let y = f64::from(p[1]) * (684.412 * y_scale);
                let z = f64::from(p[2]) * (684.412 * xz_scale);
                let a = clamp(
                    fbm[2].get(x / xz_factor, y / y_factor, z / xz_factor) + 0.5,
                    0.0,
                    1.0,
                );
                if a == 0.0 {
                    fbm[0].get(x, y, z)
                } else if a == 1.0 {
                    fbm[1].get(x, y, z)
                } else {
                    mth::lerp_f32(a, fbm[0].get(x, y, z), fbm[1].get(x, y, z))
                }
            }
        }
    }

    /// Port of DensitySampler#sampleVolume. Volume evaluation is intentionally
    /// separate: Perlin dot products and cell interpolation have different float
    /// evaluation order from sampleValue, and cached scalars can read a volume.
    pub fn sample_volume(
        &self,
        id: NodeId,
        context: &mut SamplerContext,
        volume: DensityVolume,
    ) -> Vec<f32> {
        use DensityFunction as D;
        let node = &self.nodes[id.0];
        let c = &node.children;
        let sample = |id, ctx: &mut SamplerContext| self.sample_volume(id, ctx, volume);
        match node.definition.as_ref() {
            D::Constant(v) => vec![*v; volume.len()],
            D::Noise {
                xz_scale, y_scale, ..
            } => {
                if c.iter().all(|id| self.constant(*id) == Some(0.0)) {
                    let mut out = vec![0.0; volume.len()];
                    node.noise
                        .as_ref()
                        .unwrap()
                        .add_to_volume(&mut out, volume, *xz_scale, *y_scale, 1.0);
                    out
                } else {
                    let sx = sample(c[0], context);
                    let sy = sample(c[1], context);
                    let sz = sample(c[2], context);
                    let n = node.noise.as_ref().unwrap();
                    (0..volume.len())
                        .map(|i| {
                            let [x, y, z] = volume.position(i);
                            n.get(
                                f64::from(x) * xz_scale + f64::from(sx[i]),
                                f64::from(y) * y_scale + f64::from(sy[i]),
                                f64::from(z) * xz_scale + f64::from(sz[i]),
                            )
                        })
                        .collect()
                }
            }
            D::Shift { kind, .. } => {
                let mut out = vec![0.0; volume.len()];
                let n = node.noise.as_ref().unwrap();
                if *kind == Shift::B {
                    let transposed = DensityVolume {
                        size: [volume.size[2], volume.size[0], 1],
                        min: [volume.min[2], volume.min[0], 0],
                        step: [volume.step[2], volume.step[0], 1],
                    };
                    let mut b = vec![0.0; transposed.len()];
                    n.add_to_volume(&mut b, transposed, 0.25, 0.25, 4.0);
                    for (i, v) in out.iter_mut().enumerate() {
                        let z = i / (volume.size[0] * volume.size[1]);
                        let x = i / volume.size[1] % volume.size[0];
                        *v = b[transposed.index(z, x, 0)];
                    }
                } else {
                    n.add_to_volume(
                        &mut out,
                        volume,
                        0.25,
                        if *kind == Shift::A { 0.0 } else { 0.25 },
                        4.0,
                    );
                }
                out
            }
            D::Unary { kind, .. } => sample(c[0], context)
                .into_iter()
                .map(|v| unary(*kind, v))
                .collect(),
            D::Binary { kind, .. } => {
                let left = sample(c[0], context);
                let right = sample(c[1], context);
                left.into_iter()
                    .zip(right)
                    .map(|(a, b)| {
                        binary(*kind, a, b, self.constant(c[0]), self.constant(c[1]), true)
                    })
                    .collect()
            }
            D::Round { kind, .. } => sample(c[0], context)
                .into_iter()
                .zip(sample(c[1], context))
                .map(|(a, b)| round(*kind, a, b))
                .collect(),
            D::Pow { .. } => sample(c[0], context)
                .into_iter()
                .zip(sample(c[1], context))
                .map(|(a, b)| {
                    power(
                        a,
                        b,
                        self.constant(c[0]).is_none() && self.constant(c[1]).is_some(),
                    )
                })
                .collect(),
            D::Spline(_) => {
                let mut coordinates = HashMap::new();
                (0..volume.len())
                    .map(|i| {
                        self.spline_value(node.spline.as_ref().unwrap(), &mut |coordinate| {
                            coordinates
                                .entry(coordinate)
                                .or_insert_with(|| sample(coordinate, context))[i]
                        })
                    })
                    .collect()
            }
            D::Lerp { .. } => {
                let alpha = sample(c[0], context);
                let first = sample(c[1], context);
                let second = sample(c[2], context);
                alpha
                    .into_iter()
                    .zip(first)
                    .zip(second)
                    .map(|((a, f), s)| {
                        if a == 0.0 {
                            f
                        } else if a == 1.0 {
                            s
                        } else {
                            mth::lerp_f32(a, f, s)
                        }
                    })
                    .collect()
            }
            D::Clamp { min, max, .. } => sample(c[0], context)
                .into_iter()
                .map(|v| clamp(v, *min, *max))
                .collect(),
            D::RangeChoice { min, max, .. } => {
                let input = sample(c[0], context);
                let inside = sample(c[1], context);
                let outside = sample(c[2], context);
                input
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| {
                        if v >= *min && v < *max {
                            inside[i]
                        } else {
                            outside[i]
                        }
                    })
                    .collect()
            }
            D::IntervalSelect { thresholds, .. } => {
                let input = sample(c[0], context);
                let functions = c[1..]
                    .iter()
                    .map(|id| sample(*id, context))
                    .collect::<Vec<_>>();
                input
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| functions[thresholds.partition_point(|t| v >= *t)][i])
                    .collect()
            }
            D::Cache(_) => {
                if !context.enabled {
                    return sample(c[0], context);
                }
                if let Some((old, b)) = context.cells.get(&id).and_then(|cell| cell.volume.as_ref())
                {
                    if *old == volume {
                        return b.clone();
                    }
                }
                let result = sample(c[0], context);
                context.cells.entry(id).or_default().volume = Some((volume, result.clone()));
                result
            }
            D::BlendDensity(_) => sample(c[0], context),
            D::Slice {
                axis, coordinate, ..
            } => {
                let axis = axis.index();
                let mut sliced = volume;
                sliced.min[axis] = *coordinate;
                sliced.size[axis] = 1;
                if sliced == volume {
                    return sample(c[0], context);
                }
                let input = self.sample_volume(c[0], context, sliced);
                (0..volume.len())
                    .map(|i| {
                        let p = volume.position(i);
                        let mut local = std::array::from_fn::<_, 3, _>(|j| {
                            ((p[j] - volume.min[j]) / volume.step[j]) as usize
                        });
                        local[axis] = 0;
                        input[sliced.index(local[0], local[1], local[2])]
                    })
                    .collect()
            }
            D::FindTopSurface {
                lower, cell_height, ..
            } => {
                let mut flat = volume;
                flat.min[1] = 0;
                flat.size[1] = 1;
                let upper = self.sample_volume(c[1], context, flat);
                let heights = upper
                    .into_iter()
                    .enumerate()
                    .map(|(i, upper)| {
                        let [x, _, z] = flat.position(i);
                        self.find_top(c[0], context, x, z, upper, *lower, *cell_height) as f32
                    })
                    .collect::<Vec<_>>();
                (0..volume.len())
                    .map(|i| heights[i / volume.size[1]])
                    .collect()
            }
            D::Interpolated {
                cell_xz, cell_y, ..
            } => self.interpolate_volume(c[0], context, volume, [*cell_xz, *cell_y, *cell_xz]),
            D::OldBlendedNoise {
                xz_scale,
                y_scale,
                xz_factor,
                y_factor,
                ..
            } => {
                let fbm = node.blended.as_ref().unwrap();
                let mut a = vec![0.0; volume.len()];
                let mut b = a.clone();
                let mut alpha = a.clone();
                let xs = 684.412 * xz_scale;
                let ys = 684.412 * y_scale;
                fbm[2].add_to_volume(&mut alpha, volume, xs / xz_factor, ys / y_factor, 1.0);
                fbm[0].add_to_volume(&mut a, volume, xs, ys, 1.0);
                fbm[1].add_to_volume(&mut b, volume, xs, ys, 1.0);
                alpha
                    .into_iter()
                    .zip(a)
                    .zip(b)
                    .map(|((alpha, a), b)| {
                        let t = clamp(alpha + 0.5, 0.0, 1.0);
                        if t == 0.0 {
                            a
                        } else if t == 1.0 {
                            b
                        } else {
                            mth::lerp_f32(t, a, b)
                        }
                    })
                    .collect()
            }
            _ => (0..volume.len())
                .map(|i| self.sample_value(id, context, volume.position(i)))
                .collect(),
        }
    }
    fn interpolate_volume(
        &self,
        input: NodeId,
        context: &mut SamplerContext,
        volume: DensityVolume,
        cell: [i32; 3],
    ) -> Vec<f32> {
        if (0..3).all(|i| {
            (volume.step[i] == cell[i] || volume.size[i] == 1)
                && volume.min[i].rem_euclid(cell[i]) == 0
        }) {
            return self.sample_volume(input, context, volume);
        }
        if volume.step != [1; 3] {
            let blocks = DensityVolume {
                size: std::array::from_fn(|i| volume.size[i] * volume.step[i] as usize),
                min: volume.min,
                step: [1; 3],
            };
            let buffer = self.interpolate_volume(input, context, blocks, cell);
            return (0..volume.len())
                .map(|i| {
                    let p = volume.position(i);
                    buffer[blocks.index(
                        (p[0] - blocks.min[0]) as usize,
                        (p[1] - blocks.min[1]) as usize,
                        (p[2] - blocks.min[2]) as usize,
                    )]
                })
                .collect();
        }
        let min_cell = std::array::from_fn::<_, 3, _>(|i| volume.min[i].div_euclid(cell[i]));
        let max = std::array::from_fn::<_, 3, _>(|i| volume.min[i] + volume.size[i] as i32 - 1);
        let size = std::array::from_fn(|i| {
            (max[i].div_euclid(cell[i]) - min_cell[i]
                + 1
                + i32::from(max[i].rem_euclid(cell[i]) != 0)) as usize
        });
        let grid = DensityVolume {
            size,
            min: std::array::from_fn(|i| min_cell[i] * cell[i]),
            step: cell,
        };
        let buffer = self.sample_volume(input, context, grid);
        let mut out = vec![0.0; volume.len()];
        let inv = cell.map(|n| 1.0 / n as f32);
        for z in 0..volume.size[2] {
            let gz = (volume.min[2] + z as i32 - grid.min[2]) / cell[2];
            let tz = (volume.min[2] + z as i32 - grid.min[2]) % cell[2];
            for x in 0..volume.size[0] {
                let gx = (volume.min[0] + x as i32 - grid.min[0]) / cell[0];
                let tx = (volume.min[0] + x as i32 - grid.min[0]) % cell[0];
                let mut y = 0;
                while y < volume.size[1] {
                    let gy = (volume.min[1] + y as i32 - grid.min[1]) / cell[1];
                    let ty = (volume.min[1] + y as i32 - grid.min[1]) % cell[1];
                    let v = |dx: usize, dy: usize, dz: usize| {
                        buffer[grid.index(
                            (gx as usize + dx).min(size[0] - 1),
                            (gy as usize + dy).min(size[1] - 1),
                            (gz as usize + dz).min(size[2] - 1),
                        )]
                    };
                    let a = mth::lerp_f32(
                        tx as f32 * inv[0],
                        mth::lerp_f32(tz as f32 * inv[2], v(0, 0, 0), v(0, 0, 1)),
                        mth::lerp_f32(tz as f32 * inv[2], v(1, 0, 0), v(1, 0, 1)),
                    );
                    let b = mth::lerp_f32(
                        tx as f32 * inv[0],
                        mth::lerp_f32(tz as f32 * inv[2], v(0, 1, 0), v(0, 1, 1)),
                        mth::lerp_f32(tz as f32 * inv[2], v(1, 1, 0), v(1, 1, 1)),
                    );
                    let step = (b - a) * inv[1];
                    let mut value = a + step * ty as f32;
                    let count = (cell[1] - ty) as usize;
                    let end = (y + count).min(volume.size[1]);
                    while y < end {
                        out[volume.index(x, y, z)] = value;
                        value += step;
                        y += 1;
                    }
                }
            }
        }
        out
    }
    fn find_top(
        &self,
        density: NodeId,
        context: &mut SamplerContext,
        x: i32,
        z: i32,
        upper: f32,
        lower: i32,
        step: i32,
    ) -> i32 {
        let mut y = mth::floor(f64::from(upper / step as f32)).wrapping_mul(step);
        if y <= lower {
            return lower;
        }
        while y >= lower {
            if self.sample_value(density, context, [x, y, z]) > 0.0 {
                return y;
            }
            let next = y.wrapping_sub(step);
            if next > y {
                break;
            }
            y = next;
        }
        lower
    }
    fn spline_value(&self, spline: &CompiledSpline, sample: &mut impl FnMut(NodeId) -> f32) -> f32 {
        // `self` is intentionally not involved: all nested coordinates come from
        // the supplied scalar or lazily populated volume input.
        let _ = self;
        match spline {
            CompiledSpline::Constant(v) => *v,
            CompiledSpline::Multipoint { coordinate, points } => {
                let x = sample(*coordinate);
                let upper = points.partition_point(|p| x >= p.0);
                if upper == 0 || upper == points.len() {
                    let (location, value, derivative) = if upper == 0 {
                        &points[0]
                    } else {
                        points.last().unwrap()
                    };
                    let value = self.spline_value(value, sample);
                    if *derivative == 0.0 {
                        value
                    } else {
                        value + derivative * (x - location)
                    }
                } else {
                    let (x1, v1, d1) = &points[upper - 1];
                    let (x2, v2, d2) = &points[upper];
                    let t = (x - x1) / (x2 - x1);
                    let y1 = self.spline_value(v1, sample);
                    let y2 = self.spline_value(v2, sample);
                    let a = d1 * (x2 - x1) - (y2 - y1);
                    let b = -d2 * (x2 - x1) + (y2 - y1);
                    mth::lerp_f32(t, y1, y2) + t * (1.0 - t) * mth::lerp_f32(t, a, b)
                }
            }
        }
    }
}

fn children(f: &DensityFunction) -> Vec<&Function> {
    use DensityFunction as D;
    match f {
        D::Noise { shift, .. } => shift.iter().collect(),
        D::Unary { input, .. }
        | D::Round {
            input, multiple: _, ..
        }
        | D::Clamp { input, .. }
        | D::Slice { input, .. }
        | D::Interpolated { input, .. }
        | D::RangeChoice { input, .. }
        | D::IntervalSelect { input, .. } => {
            let mut out = vec![input];
            match f {
                D::Round { multiple, .. } => out.push(multiple),
                D::RangeChoice {
                    inside, outside, ..
                } => {
                    out.push(inside);
                    out.push(outside);
                }
                D::IntervalSelect { functions, .. } => out.extend(functions),
                _ => {}
            }
            out
        }
        D::Cache(f) | D::BlendDensity(f) => vec![f],
        D::Binary { left, right, .. } => vec![left, right],
        D::Pow { base, exponent } => vec![base, exponent],
        D::Lerp {
            alpha,
            first,
            second,
        } => vec![alpha, first, second],
        D::FindTopSurface { density, upper, .. } => vec![density, upper],
        _ => vec![],
    }
}
fn index_of(volume: DensityVolume, p: [i32; 3]) -> Option<usize> {
    let mut local = [0; 3];
    for i in 0..3 {
        let diff = p[i].wrapping_sub(volume.min[i]);
        if diff < 0 || diff % volume.step[i] != 0 {
            return None;
        }
        local[i] = (diff / volume.step[i]) as usize;
        if local[i] >= volume.size[i] {
            return None;
        }
    }
    Some(volume.index(local[0], local[1], local[2]))
}
fn create_fbm(
    random: &mut impl RandomSource,
    first: i32,
    smear: f64,
    mut amplitude: f64,
) -> NoiseStack {
    let mut out = NoiseStack::new();
    let mut frequency = 1.0;
    amplitude /= 2.0_f64.powi(-first + 1) - 1.0;
    for _ in first..=0 {
        out.add(
            SmearedPerlinNoise::new(random, smear * frequency),
            frequency,
            amplitude as f32,
        );
        frequency /= 2.0;
        amplitude *= 2.0;
    }
    out
}
fn clamp(v: f32, min: f32, max: f32) -> f32 {
    java::min_f32(java::max_f32(v, min), max)
}
fn unary(kind: Unary, v: f32) -> f32 {
    match kind {
        Unary::Abs => v.abs(),
        Unary::Square => v * v,
        Unary::Cube => v * v * v,
        Unary::Sqrt => (f64::from(v).sqrt()) as f32,
        Unary::HalfNegative => {
            if v > 0.0 {
                v
            } else {
                v * 0.5
            }
        }
        Unary::QuarterNegative => {
            if v > 0.0 {
                v
            } else {
                v * 0.25
            }
        }
        Unary::Reciprocal => 1.0 / v,
        Unary::Negate => -v,
        Unary::Squeeze => {
            let v = clamp(v, -1.0, 1.0);
            v / 2.0 - v * v * v / 24.0
        }
        Unary::Log => f64::from(v).ln() as f32,
        Unary::Sign => {
            if v == 0.0 || v.is_nan() {
                v
            } else if v > 0.0 {
                1.0
            } else {
                -1.0
            }
        }
    }
}
fn binary(kind: Binary, a: f32, b: f32, ca: Option<f32>, cb: Option<f32>, volume: bool) -> f32 {
    match kind {
        Binary::Add => {
            if ca.is_some() {
                b + a
            } else {
                a + b
            }
        }
        Binary::Sub => {
            if cb.is_some() && ca.is_none() {
                a + (-b)
            } else if volume && ca.is_none() {
                a + (-b)
            } else {
                a - b
            }
        }
        Binary::Mul => {
            if ca.is_some() {
                b * a
            } else {
                a * b
            }
        }
        Binary::Div => {
            if cb.is_some() && ca.is_none() {
                a * (1.0 / b)
            } else {
                a / b
            }
        }
        Binary::Min => {
            if volume && ca.is_none() && cb.is_none() {
                if b < a { b } else { a }
            } else {
                java::min_f32(a, b)
            }
        }
        Binary::Max => {
            if volume && ca.is_none() && cb.is_none() {
                if b > a { b } else { a }
            } else {
                java::max_f32(a, b)
            }
        }
    }
}
fn round(kind: Round, input: f32, multiple: f32) -> f32 {
    if multiple == 0.0 {
        return input;
    }
    let value = input / multiple;
    let rounded = match kind {
        Round::Floor => value.floor(),
        Round::Round => {
            let floor = value.floor();
            (if value - floor >= 0.5 {
                floor + 1.0
            } else {
                floor
            }) as i32 as f32
        }
        Round::Ceil => value.ceil(),
        Round::Truncate => value.trunc(),
    };
    rounded * multiple
}
fn power(base: f32, exponent: f32, special: bool) -> f32 {
    if special {
        let v = match exponent.abs() {
            0.5 => unary(Unary::Sqrt, base),
            1.0 => base,
            2.0 => base * base,
            3.0 => base * base * base,
            _ => return f64::from(base).powf(f64::from(exponent)) as f32,
        };
        if exponent >= 0.0 { v } else { 1.0 / v }
    } else {
        f64::from(base).powf(f64::from(exponent)) as f32
    }
}
fn gradient(coordinate: i32, tiling: TilingMode, from: i32, to: i32, a: f32, b: f32) -> f32 {
    let range = to.wrapping_sub(from);
    let factor = (b - a) / range as f32;
    let relative = match tiling {
        TilingMode::ClampToEdge => coordinate
            .clamp(from.min(to), from.max(to))
            .wrapping_sub(from),
        TilingMode::Repeat => {
            let rel = coordinate.wrapping_sub(from);
            rel.wrapping_sub(java_floor_div(rel, range).wrapping_mul(range))
        }
        TilingMode::MirroredRepeat => {
            let rel = coordinate.wrapping_sub(from);
            let tile = java_floor_div(rel, range);
            let local = rel.wrapping_sub(tile.wrapping_mul(range));
            if tile & 1 == 0 {
                local
            } else {
                range.wrapping_sub(local)
            }
        }
    };
    a + relative as f32 * factor
}
fn java_floor_div(a: i32, b: i32) -> i32 {
    if a == i32::MIN && b == -1 {
        i32::MIN
    } else {
        let q = a / b;
        let r = a % b;
        if r != 0 && (a ^ b) < 0 { q - 1 } else { q }
    }
}
fn distance(p: [f32; 3], metric: DistanceMetric) -> f32 {
    match metric {
        DistanceMetric::Euclidean => {
            (f64::from(p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt()) as f32
        }
        DistanceMetric::EuclideanSquared => p[0] * p[0] + p[1] * p[1] + p[2] * p[2],
        DistanceMetric::Manhattan => p[0].abs() + p[1].abs() + p[2].abs(),
        DistanceMetric::Chebyshev => {
            java::max_f32(java::max_f32(p[0].abs(), p[1].abs()), p[2].abs())
        }
    }
}
fn end_islands(noise: &SimplexNoise, x: i32, z: i32) -> f32 {
    let sx = x / 8;
    let sz = z / 8;
    let cx = sx / 2;
    let cz = sz / 2;
    let subx = sx % 2;
    let subz = sz % 2;
    let mut offset = -100.0_f32;
    for dx in -12..=12 {
        for dz in -12..=12 {
            let x = i64::from(cx.wrapping_add(dx));
            let z = i64::from(cz.wrapping_add(dz));
            if x * x + z * z > 4096 && noise.get_2d(x as f64, z as f64) < -0.9 {
                let size = ((x as f32).abs() * 3439.0 + (z as f32).abs() * 147.0) % 13.0 + 9.0;
                let xx = (subx - dx * 2) as f32;
                let zz = (subz - dz * 2) as f32;
                let value = 100.0 - (f64::from(xx * xx + zz * zz).sqrt() as f32) * size;
                offset = java::max_f32(offset, clamp(value, -100.0, 80.0));
            }
        }
    }
    (offset - 8.0) / 128.0
}
