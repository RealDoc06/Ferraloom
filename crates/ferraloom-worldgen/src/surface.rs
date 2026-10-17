//! Port of MaterialSystem, material-rule evaluation, and surface extensions.
use crate::{
    Error, Identifier,
    biome_manager::BiomeTemperatures,
    density::*,
    generator::{GeneratedChunk, NoiseChunk, is_air},
    material::*,
};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{
    mth,
    noise::{Noise, NoiseStack},
    random::{PositionalRandomFactory, RandomSource},
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug)]
enum CompiledRule {
    Block(BlockState),
    Sequence(Vec<CompiledRule>),
    Condition {
        condition: CompiledCondition,
        then: Box<CompiledRule>,
    },
    Bandlands,
    Ore {
        ore: BlockState,
        raw: BlockState,
        filler: BlockState,
        chance: f32,
        density: NodeId,
        richness: NodeId,
        gap: NodeId,
        random: PositionalRandomFactory,
    },
}
#[derive(Debug)]
enum CompiledCondition {
    Constant(bool),
    Biome(BTreeSet<Identifier>),
    Noise {
        noise: NoiseStack,
        min: f64,
        max: f64,
        is_3d: bool,
    },
    Not(Box<CompiledCondition>),
    Y {
        anchor: VerticalAnchor,
        multiplier: i32,
        stone: bool,
    },
    Water {
        offset: i32,
        multiplier: i32,
        stone: bool,
    },
    Stone {
        offset: i32,
        surface: bool,
        secondary: i32,
        ceiling: bool,
    },
    Gradient {
        random: PositionalRandomFactory,
        below: VerticalAnchor,
        above: VerticalAnchor,
    },
    Temperature,
    Steep,
    Hole,
    AbovePreliminarySurface,
}
#[derive(Debug)]
pub struct SurfaceSystem {
    rule: CompiledRule,
    default_block: BlockState,
    sea: i32,
    preliminary: NodeId,
    random: PositionalRandomFactory,
    bands: [BlockState; 192],
    clay_offset: NoiseStack,
    surface: NoiseStack,
    secondary: NoiseStack,
    badlands: [NoiseStack; 3],
    iceberg: [NoiseStack; 3],
}
impl SurfaceSystem {
    pub fn new(
        compiler: &mut DensityFunctionCompiler,
        registry: &MaterialRegistry,
        settings: &crate::settings::NoiseGeneratorSettings,
        tags: &BTreeMap<Identifier, Vec<Identifier>>,
        possible: &[Identifier],
        preliminary: NodeId,
    ) -> Result<Self, Error> {
        let rule = compile_rule(
            &settings.material_rule,
            compiler,
            registry,
            tags,
            possible,
            &mut BTreeSet::new(),
        )?;
        let random = compiler.noise_random();
        let noise = |name: &str| compiler.create_noise(&Holder::Reference(name.parse().unwrap()));
        Ok(Self {
            rule,
            default_block: settings.default_block,
            sea: settings.sea_level,
            preliminary,
            random,
            bands: generate_bands(&mut random.from_hash_of("minecraft:clay_bands")),
            clay_offset: noise("clay_bands_offset")?,
            surface: noise("surface")?,
            secondary: noise("surface_secondary")?,
            badlands: [
                noise("badlands_pillar")?,
                noise("badlands_pillar_roof")?,
                noise("badlands_surface")?,
            ],
            iceberg: [
                noise("iceberg_pillar")?,
                noise("iceberg_pillar_roof")?,
                noise("iceberg_surface")?,
            ],
        })
    }
    pub fn build(
        &self,
        chunk: &mut GeneratedChunk,
        noise: &mut NoiseChunk,
        compiler: &DensityFunctionCompiler,
        temperatures: &BiomeTemperatures,
        biome: &mut dyn FnMut([i32; 3]) -> Identifier,
    ) {
        let highest = chunk.sections.iter().rposition(|s| !s.has_only_air());
        let top = highest.map_or(chunk.min_y - 1, |i| chunk.min_y + (i as i32 + 1) * 16 - 1);
        let volume = DensityVolume {
            size: [16, (top - chunk.min_y + 1).max(1) as usize, 16],
            min: noise.volume.min,
            step: [1; 3],
        };
        let mut context = SurfaceContext {
            system: self,
            compiler,
            samplers: &mut noise.samplers,
            temperatures,
            biome_getter: biome,
            volume,
            prefilled: HashMap::new(),
            preliminary: None,
            p: [0; 3],
            biome: None,
            depth: 0,
            above: 0,
            below: 0,
            water: i32::MIN,
            gradient: [0; 2],
            secondary: None,
            min_surface: None,
            min_y: chunk.min_y,
            height: chunk.height,
        };
        self.rule.prefill(&mut context);
        for x in 0..16u8 {
            for z in 0..16u8 {
                let block_x = chunk.position[0] * 16 + i32::from(x);
                let block_z = chunk.position[1] * 16 + i32::from(z);
                let starting = chunk.world_surface[z as usize * 16 + x as usize];
                let surface_biome = (context.biome_getter)([block_x, starting, block_z]);
                if surface_biome.to_string() == "minecraft:eroded_badlands" {
                    self.badlands_extension(chunk, x, z, block_x, block_z, starting);
                }
                let height = chunk.world_surface[z as usize * 16 + x as usize];
                context.p = [block_x, 0, block_z];
                context.secondary = None;
                context.min_surface = None;
                context.gradient = [
                    chunk.world_surface[z as usize * 16 + (x + 1).min(15) as usize]
                        - chunk.world_surface[z as usize * 16 + x.saturating_sub(1) as usize],
                    chunk.world_surface[(z + 1).min(15) as usize * 16 + x as usize]
                        - chunk.world_surface[z.saturating_sub(1) as usize * 16 + x as usize],
                ];
                context.depth = (f64::from(self.surface.get(
                    f64::from(block_x),
                    0.0,
                    f64::from(block_z),
                )) * 2.75
                    + 3.0
                    + self.random.at(block_x, 0, block_z).next_double() * 0.25)
                    as i32;
                let mut above = 0;
                let mut water = i32::MIN;
                let mut next_ceiling = i32::MAX;
                for y in (chunk.min_y..=height).rev() {
                    let old = chunk.block(x, y, z);
                    if is_air(old) {
                        above = 0;
                        water = i32::MIN;
                    } else if !old.data().fluid.empty {
                        if water == i32::MIN {
                            water = y + 1;
                        }
                    } else {
                        if next_ceiling >= y {
                            next_ceiling = -32512;
                            for look in (chunk.min_y - 1..y).rev() {
                                let state = chunk.block(x, look, z);
                                if is_air(state) || !state.data().fluid.empty {
                                    next_ceiling = look + 1;
                                    break;
                                }
                            }
                        }
                        above += 1;
                        context.p[1] = y;
                        context.biome = None;
                        context.above = above;
                        context.below = y - next_ceiling + 1;
                        context.water = water;
                        if let Some(state) = self.rule.apply(&mut context) {
                            chunk.set_block(x, y, z, state);
                            if !state.data().fluid.empty {
                                chunk.post_processing.insert([block_x, y, block_z]);
                            }
                        }
                    }
                }
                if matches!(
                    surface_biome.to_string().as_str(),
                    "minecraft:frozen_ocean" | "minecraft:deep_frozen_ocean"
                ) {
                    let min = context.min_surface();
                    self.iceberg_extension(
                        chunk,
                        x,
                        z,
                        [block_x, block_z],
                        starting,
                        min,
                        temperatures,
                        &surface_biome,
                    );
                }
            }
        }
    }
    fn badlands_extension(
        &self,
        chunk: &mut GeneratedChunk,
        x: u8,
        z: u8,
        wx: i32,
        wz: i32,
        height: i32,
    ) {
        let [pillar, roof, surface] = &self.badlands;
        let value = (f64::from(surface.get(f64::from(wx), 0.0, f64::from(wz))) * 8.25)
            .abs()
            .min(f64::from(
                pillar.get(f64::from(wx) * 0.2, 0.0, f64::from(wz) * 0.2) * 15.0,
            ));
        if value <= 0.0 {
            return;
        }
        let floor =
            (f64::from(roof.get(f64::from(wx) * 0.75, 0.0, f64::from(wz) * 0.75)) * 1.5).abs();
        let top = mth::floor(64.0 + (value * value * 2.5).min((floor * 50.0).ceil() + 24.0));
        if height > top {
            return;
        }
        for y in (chunk.min_y..=top).rev() {
            let state = chunk.block(x, y, z);
            if state.block() == self.default_block.block() {
                break;
            }
            if state.block() == Block::WATER {
                return;
            }
        }
        for y in (chunk.min_y..=top).rev() {
            if !is_air(chunk.block(x, y, z)) {
                break;
            }
            chunk.set_block(x, y, z, self.default_block);
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn iceberg_extension(
        &self,
        chunk: &mut GeneratedChunk,
        x: u8,
        z: u8,
        world: [i32; 2],
        height: i32,
        min: i32,
        temperatures: &BiomeTemperatures,
        biome: &Identifier,
    ) {
        let [wx, wz] = world;
        let [pillar, roof, surface] = &self.iceberg;
        let iceberg = (f64::from(surface.get(f64::from(wx), 0.0, f64::from(wz))) * 8.25)
            .abs()
            .min(f64::from(
                pillar.get(f64::from(wx) * 1.28, 0.0, f64::from(wz) * 1.28) * 15.0,
            ));
        if iceberg <= 1.8 {
            return;
        }
        let roof =
            (f64::from(roof.get(f64::from(wx) * 1.17, 0.0, f64::from(wz) * 1.17)) * 1.5).abs();
        let mut top = (iceberg * iceberg * 1.2).min((roof * 40.0).ceil() + 14.0);
        if temperatures.temperature(biome, [wx, self.sea, wz], self.sea) > 0.1 {
            top -= 2.0;
        }
        if top <= 2.0 {
            return;
        }
        let bottom = f64::from(self.sea) - top - 7.0;
        top += f64::from(self.sea);
        let mut random = self.random.at(wx, 0, wz);
        let max_snow = 2 + random.next_int_bound(4);
        let min_snow = self.sea + 18 + random.next_int_bound(10);
        let mut snow = 0;
        for y in (min..=height.max(top as i32 + 1)).rev() {
            let state = chunk.block(x, y, z);
            if (is_air(state) && y < (top as i32) && random.next_double() > 0.01)
                || (state.block() == Block::WATER
                    && y > bottom as i32
                    && y < self.sea
                    && random.next_double() > 0.15)
            {
                let state = if snow <= max_snow && y > min_snow {
                    snow += 1;
                    Block::SNOW_BLOCK.default_state()
                } else {
                    Block::PACKED_ICE.default_state()
                };
                chunk.set_block(x, y, z, state);
            }
        }
    }
}
struct SurfaceContext<'a> {
    system: &'a SurfaceSystem,
    compiler: &'a DensityFunctionCompiler,
    samplers: &'a mut SamplerContext,
    temperatures: &'a BiomeTemperatures,
    biome_getter: &'a mut dyn FnMut([i32; 3]) -> Identifier,
    volume: DensityVolume,
    prefilled: HashMap<NodeId, Vec<f32>>,
    preliminary: Option<Vec<f32>>,
    p: [i32; 3],
    biome: Option<Identifier>,
    depth: i32,
    above: i32,
    below: i32,
    water: i32,
    gradient: [i32; 2],
    secondary: Option<f64>,
    min_surface: Option<i32>,
    min_y: i32,
    height: i32,
}
impl SurfaceContext<'_> {
    fn biome(&mut self) -> Identifier {
        self.biome
            .get_or_insert_with(|| (self.biome_getter)(self.p))
            .clone()
    }
    fn secondary(&mut self) -> f64 {
        *self.secondary.get_or_insert_with(|| {
            f64::from(
                self.system
                    .secondary
                    .get(f64::from(self.p[0]), 0.0, f64::from(self.p[2])),
            )
        })
    }
    fn min_surface(&mut self) -> i32 {
        if let Some(value) = self.min_surface {
            return value;
        }
        let volume = DensityVolume {
            size: [self.volume.size[0], 1, self.volume.size[2]],
            min: [self.volume.min[0], 0, self.volume.min[2]],
            step: [1; 3],
        };
        if self.preliminary.is_none() {
            self.preliminary = Some(self.compiler.sample_volume(
                self.system.preliminary,
                self.samplers,
                volume,
            ));
        }
        let index = volume.index(
            (self.p[0] - volume.min[0]) as usize,
            0,
            (self.p[2] - volume.min[2]) as usize,
        );
        let level =
            mth::floor(f64::from(self.preliminary.as_ref().unwrap()[index])) + self.depth - 8;
        self.min_surface = Some(level);
        level
    }
    fn prefilled(&mut self, id: NodeId) -> f32 {
        let local = std::array::from_fn::<_, 3, _>(|i| self.p[i] - self.volume.min[i]);
        if (0..3).all(|i| local[i] >= 0 && (local[i] as usize) < self.volume.size[i]) {
            self.prefilled[&id][self.volume.index(
                local[0] as usize,
                local[1] as usize,
                local[2] as usize,
            )]
        } else {
            self.compiler.sample_value(id, self.samplers, self.p)
        }
    }
    fn resolve(&self, anchor: VerticalAnchor) -> i32 {
        anchor.resolve(self.min_y, self.height, self.system.sea)
    }
}
impl CompiledRule {
    fn prefill(&self, context: &mut SurfaceContext<'_>) {
        match self {
            Self::Sequence(rules) => {
                for rule in rules {
                    rule.prefill(context)
                }
            }
            Self::Condition { then, .. } => then.prefill(context),
            Self::Ore {
                density, richness, ..
            } => {
                for id in [density, richness] {
                    let buffer =
                        context
                            .compiler
                            .sample_volume(*id, context.samplers, context.volume);
                    context.prefilled.insert(*id, buffer);
                }
            }
            _ => {}
        }
    }
    fn apply(&self, context: &mut SurfaceContext<'_>) -> Option<BlockState> {
        match self {
            Self::Block(state) => Some(*state),
            Self::Sequence(rules) => rules.iter().find_map(|rule| rule.apply(context)),
            Self::Condition { condition, then } => {
                if condition.test(context) {
                    then.apply(context)
                } else {
                    None
                }
            }
            Self::Bandlands => {
                let offset = java_round(
                    context.system.clay_offset.get(
                        f64::from(context.p[0]),
                        0.0,
                        f64::from(context.p[2]),
                    ) * 4.0,
                );
                Some(context.system.bands[((context.p[1] + offset + 192) % 192) as usize])
            }
            Self::Ore {
                ore,
                raw,
                filler,
                chance,
                density,
                richness,
                gap,
                random,
            } => {
                let density = context.prefilled(*density);
                if density <= 0.0 {
                    return None;
                }
                let mut r = random.at(context.p[0], context.p[1], context.p[2]);
                if r.next_float() > density {
                    return None;
                }
                let richness = context.prefilled(*richness);
                if r.next_float() < richness
                    && context
                        .compiler
                        .sample_value(*gap, context.samplers, context.p)
                        < 0.0
                {
                    Some(if r.next_float() < *chance { *raw } else { *ore })
                } else {
                    Some(*filler)
                }
            }
        }
    }
}
impl CompiledCondition {
    fn test(&self, c: &mut SurfaceContext<'_>) -> bool {
        match self {
            Self::Constant(value) => *value,
            Self::Biome(biomes) => biomes.contains(&c.biome()),
            Self::Noise {
                noise,
                min,
                max,
                is_3d,
            } => {
                let value = f64::from(noise.get(
                    f64::from(c.p[0]),
                    if *is_3d { f64::from(c.p[1]) } else { 0.0 },
                    f64::from(c.p[2]),
                ));
                value >= *min && value <= *max
            }
            Self::Not(condition) => !condition.test(c),
            Self::Y {
                anchor,
                multiplier,
                stone,
            } => {
                c.p[1] + if *stone { c.above } else { 0 }
                    >= c.resolve(*anchor) + c.depth * multiplier
            }
            Self::Water {
                offset,
                multiplier,
                stone,
            } => {
                c.water == i32::MIN
                    || c.p[1] + if *stone { c.above } else { 0 }
                        >= c.water + offset + c.depth * multiplier
            }
            Self::Stone {
                offset,
                surface,
                secondary,
                ceiling,
            } => {
                let depth = if *ceiling { c.below } else { c.above };
                let surface = if *surface { c.depth } else { 0 };
                let secondary = if *secondary == 0 {
                    0
                } else {
                    mth::map(c.secondary(), -1.0, 1.0, 0.0, f64::from(*secondary)) as i32
                };
                depth <= 1 + offset + surface + secondary
            }
            Self::Gradient {
                random,
                below,
                above,
            } => {
                let below = c.resolve(*below);
                let above = c.resolve(*above);
                if c.p[1] <= below {
                    true
                } else if c.p[1] >= above {
                    false
                } else {
                    f64::from(random.at(c.p[0], c.p[1], c.p[2]).next_float())
                        < mth::map(
                            f64::from(c.p[1]),
                            f64::from(below),
                            f64::from(above),
                            1.0,
                            0.0,
                        )
                }
            }
            Self::Temperature => {
                let biome = c.biome();
                c.temperatures.temperature(&biome, c.p, c.system.sea) < 0.15
            }
            Self::Steep => c.gradient[0] <= -4 || c.gradient[1] >= 4,
            Self::Hole => c.depth <= 0,
            Self::AbovePreliminarySurface => c.p[1] >= c.min_surface(),
        }
    }
}
fn compile_rule(
    rule: &Rule,
    compiler: &mut DensityFunctionCompiler,
    registry: &MaterialRegistry,
    tags: &BTreeMap<Identifier, Vec<Identifier>>,
    possible: &[Identifier],
    stack: &mut BTreeSet<Identifier>,
) -> Result<CompiledRule, Error> {
    Ok(match rule.as_ref() {
        MaterialRule::Reference(id) => {
            if !stack.insert(id.clone()) {
                return Err(Error(format!("cyclic material rule {id}")));
            }
            let rule = registry
                .rules
                .get(id)
                .ok_or_else(|| Error(format!("missing material rule {id}")))?;
            let result = compile_rule(rule, compiler, registry, tags, possible, stack)?;
            stack.remove(id);
            result
        }
        MaterialRule::Block(state) => CompiledRule::Block(*state),
        MaterialRule::Sequence(rules) => CompiledRule::Sequence(
            rules
                .iter()
                .map(|rule| compile_rule(rule, compiler, registry, tags, possible, stack))
                .collect::<Result<_, _>>()?,
        ),
        MaterialRule::Condition { condition, then } => CompiledRule::Condition {
            condition: compile_condition(
                condition,
                compiler,
                registry,
                tags,
                possible,
                &mut BTreeSet::new(),
            )?,
            then: Box::new(compile_rule(
                then, compiler, registry, tags, possible, stack,
            )?),
        },
        MaterialRule::Bandlands => CompiledRule::Bandlands,
        MaterialRule::OreVein {
            ore,
            raw,
            filler,
            raw_chance,
            density,
            richness,
            filler_gap,
        } => CompiledRule::Ore {
            ore: *ore,
            raw: *raw,
            filler: *filler,
            chance: *raw_chance,
            density: compiler.compile(density)?,
            richness: compiler.compile(richness)?,
            gap: compiler.compile(filler_gap)?,
            random: compiler.positional_random(&"ore".parse()?),
        },
    })
}
fn compile_condition(
    condition: &Condition,
    compiler: &DensityFunctionCompiler,
    registry: &MaterialRegistry,
    tags: &BTreeMap<Identifier, Vec<Identifier>>,
    possible: &[Identifier],
    stack: &mut BTreeSet<Identifier>,
) -> Result<CompiledCondition, Error> {
    use MaterialCondition as C;
    Ok(match condition.as_ref() {
        C::Reference(id) => {
            if !stack.insert(id.clone()) {
                return Err(Error(format!("cyclic material condition {id}")));
            }
            let c = registry
                .conditions
                .get(id)
                .ok_or_else(|| Error(format!("missing material condition {id}")))?;
            let result = compile_condition(c, compiler, registry, tags, possible, stack)?;
            stack.remove(id);
            result
        }
        C::Biome(set) => {
            let values = match set {
                BiomeSet::Direct(values) => values,
                BiomeSet::Tag(id) => tags
                    .get(id)
                    .ok_or_else(|| Error(format!("missing biome tag {id}")))?,
            };
            let biomes = values.iter().cloned().collect::<BTreeSet<_>>();
            if !possible.iter().any(|id| biomes.contains(id)) {
                CompiledCondition::Constant(false)
            } else if possible.iter().all(|id| biomes.contains(id)) {
                CompiledCondition::Constant(true)
            } else {
                CompiledCondition::Biome(biomes)
            }
        }
        C::NoiseThreshold {
            noise,
            min,
            max,
            is_3d,
        } => CompiledCondition::Noise {
            noise: compiler.create_noise(&Holder::Reference(noise.clone()))?,
            min: *min,
            max: *max,
            is_3d: *is_3d,
        },
        C::Not(c) => CompiledCondition::Not(Box::new(compile_condition(
            c, compiler, registry, tags, possible, stack,
        )?)),
        C::YAbove {
            anchor,
            surface_multiplier,
            add_stone,
        } => CompiledCondition::Y {
            anchor: *anchor,
            multiplier: *surface_multiplier,
            stone: *add_stone,
        },
        C::Water {
            offset,
            surface_multiplier,
            add_stone,
        } => CompiledCondition::Water {
            offset: *offset,
            multiplier: *surface_multiplier,
            stone: *add_stone,
        },
        C::StoneDepth {
            offset,
            add_surface,
            secondary_range,
            ceiling,
        } => CompiledCondition::Stone {
            offset: *offset,
            surface: *add_surface,
            secondary: *secondary_range,
            ceiling: *ceiling,
        },
        C::VerticalGradient {
            random_name,
            below,
            above,
        } => CompiledCondition::Gradient {
            random: compiler.positional_random(random_name),
            below: *below,
            above: *above,
        },
        C::Temperature => CompiledCondition::Temperature,
        C::Steep => CompiledCondition::Steep,
        C::Hole => CompiledCondition::Hole,
        C::AbovePreliminarySurface => CompiledCondition::AbovePreliminarySurface,
    })
}
fn generate_bands(random: &mut impl RandomSource) -> [BlockState; 192] {
    let mut bands = [Block::TERRACOTTA.default_state(); 192];
    let mut i = 0;
    while i < 192 {
        i += (random.next_int_bound(5) + 1) as usize;
        if i < 192 {
            bands[i] = Block::ORANGE_TERRACOTTA.default_state();
        }
        i += 1;
    }
    for (width, block) in [
        (1, Block::YELLOW_TERRACOTTA),
        (2, Block::BROWN_TERRACOTTA),
        (1, Block::RED_TERRACOTTA),
    ] {
        let count = random.next_int_bound(10) + 6;
        for _ in 0..count {
            let width = (width + random.next_int_bound(3)) as usize;
            let start = random.next_int_bound(192) as usize;
            for band in bands.iter_mut().skip(start).take(width) {
                *band = block.default_state();
            }
        }
    }
    let count = random.next_int_bound(7) + 9;
    let mut start = 0;
    for _ in 0..count {
        if start >= 192 {
            break;
        }
        bands[start] = Block::WHITE_TERRACOTTA.default_state();
        if start > 1 && random.next_boolean() {
            bands[start - 1] = Block::LIGHT_GRAY_TERRACOTTA.default_state();
        }
        if start + 1 < 192 && random.next_boolean() {
            bands[start + 1] = Block::LIGHT_GRAY_TERRACOTTA.default_state();
        }
        start += (random.next_int_bound(16) + 4) as usize;
    }
    bands
}
fn java_round(value: f32) -> i32 {
    let floor = value.floor();
    (if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }) as i32
}
