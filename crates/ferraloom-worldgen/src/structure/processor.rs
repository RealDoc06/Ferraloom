//! StructureProcessor decoding and the ordered per-block/finalization passes.
use super::{
    rule::{BlockTags, PosRuleTest, RuleTest},
    template::{StructureBlockInfo, StructurePlaceSettings},
};
use crate::{Error, Identifier, codec::*, placement::providers::IntProvider, region::Heightmap};
use ferraloom_data::{Block, BlockState, datapack::decode_block_state, properties};
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_util::{
    mth,
    random::{LegacyRandomSource, RandomSource, SingleThreadedRandomSource},
};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlockSet {
    Tag(Identifier),
    Direct(Vec<Block>),
}
impl BlockSet {
    pub(crate) fn decode(v: &Value) -> Result<Self, Error> {
        if let Some(tag) = v.as_str().and_then(|s| s.strip_prefix('#')) {
            return Ok(Self::Tag(tag.parse()?));
        }
        let decode = |v: &Value| {
            let id = id(v)?;
            Block::from_name(&id.to_string()).ok_or_else(|| Error(format!("unknown block {id}")))
        };
        Ok(Self::Direct(if v.is_string() {
            vec![decode(v)?]
        } else {
            list(v)?.iter().map(decode).collect::<Result<_, _>>()?
        }))
    }
    fn contains(&self, block: Block, tags: &BlockTags) -> bool {
        match self {
            Self::Tag(id) => tags.contains(id, block),
            Self::Direct(blocks) => blocks.contains(&block),
        }
    }
}
/// Port of the four RuleBlockEntityModifier types.
#[derive(Clone, Debug, Default)]
pub enum RuleBlockEntityModifier {
    #[default]
    Passthrough,
    Clear,
    AppendLoot(Identifier),
    AppendStatic(CompoundTag),
}
impl RuleBlockEntityModifier {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        match kind(v)?.to_string().as_str() {
            "minecraft:passthrough" => Ok(Self::Passthrough),
            "minecraft:clear" => Ok(Self::Clear),
            "minecraft:append_loot" => Ok(Self::AppendLoot(id(field(v, "loot_table")?)?)),
            "minecraft:append_static" => match json_nbt(field(v, "data")?)? {
                Tag::Compound(tag) => Ok(Self::AppendStatic(tag)),
                _ => Err(Error("expected static compound".into())),
            },
            s => Err(Error(format!("unknown block entity modifier {s}"))),
        }
    }
    pub fn apply(
        &self,
        random: &mut impl RandomSource,
        existing: Option<CompoundTag>,
    ) -> Option<CompoundTag> {
        match self {
            Self::Passthrough => existing,
            Self::Clear => Some(CompoundTag::new()),
            Self::AppendLoot(table) => {
                let mut tag = existing.unwrap_or_default();
                tag.insert("LootTable", Tag::String(table.to_string().into()));
                tag.insert("LootTableSeed", Tag::Long(random.next_long()));
                Some(tag)
            }
            Self::AppendStatic(extra) => {
                let mut tag = existing.unwrap_or_default();
                tag.merge(extra);
                Some(tag)
            }
        }
    }
}
// JsonOps#convertTo selects the narrowest numeric tag. In 26.3,
// NbtOps#createList always produces a ListTag, including numeric streams.
pub(super) fn json_nbt(v: &Value) -> Result<Tag, Error> {
    Ok(match v {
        Value::Null => return Err(Error("null is not a structure compound value".into())),
        Value::Bool(v) => Tag::Byte(i8::from(*v)),
        Value::String(v) => Tag::String(v.as_str().into()),
        Value::Number(v) => {
            let d = v
                .to_string()
                .parse::<f64>()
                .map_err(|_| Error("invalid NBT number".into()))?;
            if let Some(n) = v.as_i64().or_else(|| {
                (d.fract() == 0.0 && d >= i64::MIN as f64 && d < i64::MAX as f64)
                    .then_some(d as i64)
            }) {
                if let Ok(n) = i8::try_from(n) {
                    Tag::Byte(n)
                } else if let Ok(n) = i16::try_from(n) {
                    Tag::Short(n)
                } else if let Ok(n) = i32::try_from(n) {
                    Tag::Int(n)
                } else {
                    Tag::Long(n)
                }
            } else if f64::from(d as f32) == d {
                Tag::Float(d as f32)
            } else {
                Tag::Double(d)
            }
        }
        Value::Array(values) => {
            let values = values.iter().map(json_nbt).collect::<Result<Vec<_>, _>>()?;
            Tag::List(ListTag(values))
        }
        Value::Object(values) => {
            let mut tag = CompoundTag::new();
            for (k, v) in values {
                tag.insert(k.clone(), json_nbt(v)?);
            }
            Tag::Compound(tag)
        }
    })
}
/// Port of ProcessorRule, including its two lenient optional fields.
#[derive(Clone, Debug)]
pub struct ProcessorRule {
    pub input: RuleTest,
    pub location: RuleTest,
    pub position: PosRuleTest,
    pub output: BlockState,
    pub block_entity_modifier: RuleBlockEntityModifier,
}
impl ProcessorRule {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        Ok(Self {
            input: RuleTest::decode(field(v, "input_predicate")?)?,
            location: RuleTest::decode(field(v, "location_predicate")?)?,
            position: v
                .get("position_predicate")
                .and_then(|v| PosRuleTest::decode(v).ok())
                .unwrap_or(PosRuleTest::AlwaysTrue),
            output: decode_block_state(field(v, "output_state")?)?,
            block_entity_modifier: v
                .get("block_entity_modifier")
                .and_then(|v| RuleBlockEntityModifier::decode(v).ok())
                .unwrap_or_default(),
        })
    }
}
/// Port of the eleven StructureProcessorTypes registered by 26.3.
#[derive(Clone, Debug)]
pub enum StructureProcessor {
    Nop,
    BlockIgnore(Vec<Block>),
    BlockRot {
        integrity: f32,
        rottable: Option<BlockSet>,
    },
    BlockAge {
        mossiness: f32,
    },
    BlackstoneReplace,
    Gravity {
        heightmap: Heightmap,
        offset: i32,
    },
    JigsawReplacement,
    LavaSubmergedBlock,
    ProtectedBlocks(BlockSet),
    Rule(Vec<ProcessorRule>),
    Capped {
        delegate: Box<Self>,
        limit: IntProvider,
    },
}
impl StructureProcessor {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        match id(field(v, "processor_type")?)?.to_string().as_str() {
            "minecraft:nop" => Ok(Self::Nop),
            "minecraft:block_ignore" => Ok(Self::BlockIgnore(
                list(field(v, "blocks")?)?
                    .iter()
                    .map(|v| Ok(decode_block_state(v)?.block()))
                    .collect::<Result<_, Error>>()?,
            )),
            "minecraft:block_rot" => {
                let integrity = float(field(v, "integrity")?)?;
                if !(0.0..=1.0).contains(&integrity) {
                    return Err(Error("integrity outside [0,1]".into()));
                }
                Ok(Self::BlockRot {
                    integrity,
                    rottable: v.get("rottable_blocks").map(BlockSet::decode).transpose()?,
                })
            }
            "minecraft:block_age" => Ok(Self::BlockAge {
                mossiness: float(field(v, "mossiness")?)?,
            }),
            "minecraft:blackstone_replace" => Ok(Self::BlackstoneReplace),
            "minecraft:gravity" => {
                let heightmap =
                    optional(
                        v,
                        "heightmap",
                        Heightmap::WorldSurfaceWg,
                        |v| match string(v)? {
                            "WORLD_SURFACE_WG" => Ok(Heightmap::WorldSurfaceWg),
                            "WORLD_SURFACE" => Ok(Heightmap::WorldSurface),
                            "OCEAN_FLOOR_WG" => Ok(Heightmap::OceanFloorWg),
                            "OCEAN_FLOOR" => Ok(Heightmap::OceanFloor),
                            "MOTION_BLOCKING" => Ok(Heightmap::MotionBlocking),
                            "MOTION_BLOCKING_NO_LEAVES" => Ok(Heightmap::MotionBlockingNoLeaves),
                            s => Err(Error(format!("unknown heightmap {s}"))),
                        },
                    )?;
                Ok(Self::Gravity {
                    heightmap,
                    offset: optional(v, "offset", 0, integer)?,
                })
            }
            "minecraft:jigsaw_replacement" => Ok(Self::JigsawReplacement),
            "minecraft:lava_submerged_block" => Ok(Self::LavaSubmergedBlock),
            "minecraft:protected_blocks" => {
                Ok(Self::ProtectedBlocks(BlockSet::decode(field(v, "value")?)?))
            }
            "minecraft:rule" => Ok(Self::Rule(
                list(field(v, "rules")?)?
                    .iter()
                    .map(ProcessorRule::decode)
                    .collect::<Result<_, _>>()?,
            )),
            "minecraft:capped" => {
                let limit = IntProvider::decode(field(v, "limit")?)?;
                if limit.range().0 < 1 {
                    return Err(Error("capped limit must be positive".into()));
                }
                Ok(Self::Capped {
                    delegate: Box::new(Self::decode(field(v, "delegate")?)?),
                    limit,
                })
            }
            s => Err(Error(format!("unknown structure processor {s}"))),
        }
    }
    pub fn decode_list(v: &Value) -> Result<Vec<Self>, Error> {
        let v = v.get("processors").unwrap_or(v);
        list(v)?.iter().map(Self::decode).collect()
    }
    pub fn evaluates_entire_piece_state(&self) -> bool {
        matches!(self, Self::Capped { .. })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn process_block(
        &self,
        region: &impl crate::feature::LiveWorldGenLevel,
        target: [i32; 3],
        reference: [i32; 3],
        relative: [i32; 3],
        info: StructureBlockInfo,
        settings: &mut StructurePlaceSettings,
        tags: &BlockTags,
        shared: &mut impl crate::feature::FeatureRandomSource,
    ) -> Result<Option<StructureBlockInfo>, Error> {
        let mut access =
            crate::feature::FeaturePlacementAccess::read(region, shared.feature_random());
        self.process_block_with_access(
            &mut access,
            target,
            reference,
            relative,
            info,
            settings,
            tags,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn process_block_with_access(
        &self,
        access: &mut crate::feature::FeaturePlacementAccess<
            '_,
            impl crate::feature::LiveWorldGenLevel,
        >,
        target: [i32; 3],
        reference: [i32; 3],
        relative: [i32; 3],
        info: StructureBlockInfo,
        settings: &mut StructurePlaceSettings,
        tags: &BlockTags,
    ) -> Result<Option<StructureBlockInfo>, Error> {
        self.process_block_with_nbt_mutation_access(
            access, target, reference, relative, info, settings, tags,
        )
        .map(|(info, _)| info)
    }
    #[allow(clippy::too_many_arguments)]
    fn process_block_with_nbt_mutation_access(
        &self,
        access: &mut crate::feature::FeaturePlacementAccess<
            '_,
            impl crate::feature::LiveWorldGenLevel,
        >,
        target: [i32; 3],
        reference: [i32; 3],
        relative: [i32; 3],
        mut info: StructureBlockInfo,
        settings: &mut StructurePlaceSettings,
        tags: &BlockTags,
    ) -> Result<(Option<StructureBlockInfo>, bool), Error> {
        let mut mutated_input_nbt = false;
        match self {
            Self::Nop | Self::Capped { .. } => {}
            Self::BlockIgnore(blocks) => {
                if blocks.contains(&info.state.block()) {
                    return Ok((None, false));
                }
            }
            Self::BlockRot {
                integrity,
                rottable,
            } => {
                if rottable
                    .as_ref()
                    .is_none_or(|b| b.contains(info.state.block(), tags))
                    && !settings.with_random(info.position, &mut access.random()?, |r| {
                        r.next_float() <= *integrity
                    })
                {
                    return Ok((None, false));
                }
            }
            Self::BlockAge { mossiness } => {
                info.state = settings.with_random(info.position, &mut access.random()?, |r| {
                    age_block(info.state, *mossiness, tags, r)
                })
            }
            Self::BlackstoneReplace => info.state = blackstone_replace(info.state),
            Self::Gravity { heightmap, offset } => {
                info.position[1] = access
                    .level()
                    .height_at(*heightmap, info.position[0], info.position[2])?
                    .wrapping_add(*offset)
                    .wrapping_add(relative[1])
            }
            Self::JigsawReplacement => {
                if info.state.block() == Block::JIGSAW
                    && let Some(nbt) = &info.nbt
                {
                    let state = nbt
                        .get_string("final_state")
                        .map(|s| s.to_string_lossy())
                        .unwrap_or_else(|| "minecraft:air".into());
                    let Some(state) = parse_jigsaw_replacement(&state) else {
                        return Ok((None, false));
                    };
                    if state.block() == Block::STRUCTURE_VOID {
                        return Ok((None, false));
                    }
                    info.state = state;
                    info.nbt = None;
                }
            }
            Self::LavaSubmergedBlock => {
                if access.level().block(info.position)?.block() == Block::LAVA
                    && !access
                        .level()
                        .is_outline_full_block(info.state, info.position)?
                {
                    info.state = Block::LAVA.default_state();
                }
            }
            Self::ProtectedBlocks(blocks) => {
                if blocks.contains(access.level().block(info.position)?.block(), tags) {
                    return Ok((None, false));
                }
            }
            Self::Rule(rules) => {
                let mut random = LegacyRandomSource::new(mth::get_seed(
                    info.position[0],
                    info.position[1],
                    info.position[2],
                ));
                for rule in rules {
                    if rule
                        .input
                        .test(info.state, info.position, tags, &mut random)
                        && rule.location.test(
                            access.level().block(info.position)?,
                            info.position,
                            tags,
                            &mut random,
                        )
                        && rule.position.test(info.position, reference, &mut random)
                    {
                        // AppendStatic mutates the existing CompoundTag in Java.
                        // Capped compares its old record after that mutation, so
                        // these NBT changes alone do not count toward the cap.
                        mutated_input_nbt = info.nbt.is_some()
                            && matches!(
                                rule.block_entity_modifier,
                                RuleBlockEntityModifier::AppendStatic(_)
                            );
                        info.state = rule.output;
                        info.nbt = rule.block_entity_modifier.apply(&mut random, info.nbt);
                        break;
                    }
                }
            }
        }
        let _ = target; // Kept in the vanilla call signature for future processors.
        Ok((Some(info), mutated_input_nbt))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn finalize_processing(
        &self,
        region: &impl crate::feature::LiveWorldGenLevel,
        target: [i32; 3],
        reference: [i32; 3],
        original: &[&StructureBlockInfo],
        processed: &mut [StructureBlockInfo],
        settings: &mut StructurePlaceSettings,
        tags: &BlockTags,
        shared: &mut impl crate::feature::FeatureRandomSource,
    ) -> Result<(), Error> {
        let mut access =
            crate::feature::FeaturePlacementAccess::read(region, shared.feature_random());
        self.finalize_processing_with_access(
            &mut access,
            target,
            reference,
            original,
            processed,
            settings,
            tags,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finalize_processing_with_access(
        &self,
        access: &mut crate::feature::FeaturePlacementAccess<
            '_,
            impl crate::feature::LiveWorldGenLevel,
        >,
        target: [i32; 3],
        reference: [i32; 3],
        original: &[&StructureBlockInfo],
        processed: &mut [StructureBlockInfo],
        settings: &mut StructurePlaceSettings,
        tags: &BlockTags,
    ) -> Result<(), Error> {
        let Self::Capped { delegate, limit } = self else {
            return Ok(());
        };
        if limit.range().1 == 0 || processed.is_empty() || original.len() != processed.len() {
            return Ok(());
        }
        let mut random = SingleThreadedRandomSource::new(access.level().seed())
            .fork_positional()
            .at(target[0], target[1], target[2]);
        let max = limit.sample(&mut random).min(processed.len() as i32);
        if max < 1 {
            return Ok(());
        }
        let mut indices = (0..processed.len()).collect::<Vec<_>>();
        shuffle(&mut indices, &mut random);
        let mut replaced = 0;
        for index in indices {
            if replaced >= max {
                break;
            }
            let (altered, mutated_input_nbt) = delegate.process_block_with_nbt_mutation_access(
                access,
                target,
                reference,
                original[index].position,
                processed[index].clone(),
                settings,
                tags,
            )?;
            if let Some(altered) = altered {
                if mutated_input_nbt {
                    processed[index].nbt = altered.nbt.clone();
                }
                if altered != processed[index] {
                    replaced += 1;
                    processed[index] = altered;
                }
            }
        }
        Ok(())
    }
}
pub(crate) fn shuffle<T>(values: &mut [T], random: &mut impl RandomSource) {
    for i in (2..=values.len()).rev() {
        values.swap(i - 1, random.next_int_bound(i as i32) as usize);
    }
}
/// BlockStateParser#parseForBlock accepts a prefix; trailing command text is ignored.
pub(super) fn parse_jigsaw_replacement(text: &str) -> Option<BlockState> {
    let mut reader = ferraloom_brigadier::StringReader::new(text);
    while reader.can_read() && matches!(reader.peek(), 97..=122 | 48..=57 | 95 | 45 | 46 | 58 | 47)
    {
        reader.skip();
    }
    let id: Identifier = reader.read_so_far().parse().ok()?;
    let block = Block::from_name(&id.to_string())?;
    let mut values: Vec<(String, String)> = Vec::new();
    if reader.can_read() && reader.peek() == u16::from(b'[') {
        reader.skip();
        reader.skip_whitespace();
        while reader.can_read() && reader.peek() != u16::from(b']') {
            reader.skip_whitespace();
            let key = reader.read_string().ok()?;
            if values.iter().any(|(k, _)| *k == key)
                || !block.default_state().properties().any(|(k, _)| k == key)
            {
                return None;
            }
            reader.skip_whitespace();
            if !reader.can_read() || reader.read() != u16::from(b'=') {
                return None;
            }
            reader.skip_whitespace();
            values.push((key, reader.read_string().ok()?));
            reader.skip_whitespace();
            if reader.can_read() {
                if reader.peek() != u16::from(b',') {
                    if reader.peek() != u16::from(b']') {
                        return None;
                    }
                    break;
                }
                reader.skip();
            }
        }
        if !reader.can_read() {
            return None;
        }
        reader.skip();
    }
    if reader.can_read()
        && reader.peek() == u16::from(b'{')
        && !matches!(
            ferraloom_nbt::parse_prefix(&reader.remaining()).ok()?.0,
            Tag::Compound(_)
        )
    {
        return None;
    }
    let state =
        BlockState::from_properties(block, values.iter().map(|(k, v)| (k.as_str(), v.as_str())))?;
    values
        .iter()
        .all(|(key, value)| state.properties().any(|(k, v)| k == key && v == value))
        .then_some(state)
}
fn blackstone_replace(old: BlockState) -> BlockState {
    let block = match old.block() {
        Block::COBBLESTONE | Block::MOSSY_COBBLESTONE => Block::BLACKSTONE,
        Block::STONE => Block::POLISHED_BLACKSTONE,
        Block::STONE_BRICKS | Block::MOSSY_STONE_BRICKS => Block::POLISHED_BLACKSTONE_BRICKS,
        Block::COBBLESTONE_STAIRS | Block::MOSSY_COBBLESTONE_STAIRS => Block::BLACKSTONE_STAIRS,
        Block::STONE_STAIRS => Block::POLISHED_BLACKSTONE_STAIRS,
        Block::STONE_BRICK_STAIRS | Block::MOSSY_STONE_BRICK_STAIRS => {
            Block::POLISHED_BLACKSTONE_BRICK_STAIRS
        }
        Block::COBBLESTONE_SLAB | Block::MOSSY_COBBLESTONE_SLAB => Block::BLACKSTONE_SLAB,
        Block::SMOOTH_STONE_SLAB | Block::STONE_SLAB => Block::POLISHED_BLACKSTONE_SLAB,
        Block::STONE_BRICK_SLAB | Block::MOSSY_STONE_BRICK_SLAB => {
            Block::POLISHED_BLACKSTONE_BRICK_SLAB
        }
        Block::STONE_BRICK_WALL | Block::MOSSY_STONE_BRICK_WALL => {
            Block::POLISHED_BLACKSTONE_BRICK_WALL
        }
        Block::COBBLESTONE_WALL | Block::MOSSY_COBBLESTONE_WALL => Block::BLACKSTONE_WALL,
        Block::CHISELED_STONE_BRICKS => Block::CHISELED_POLISHED_BLACKSTONE,
        Block::CRACKED_STONE_BRICKS => Block::CRACKED_POLISHED_BLACKSTONE_BRICKS,
        Block::IRON_BARS => Block::IRON_CHAIN,
        _ => return old,
    };
    let mut state = block.default_state();
    if let Some(v) = old.get(properties::FACING) {
        state = state.with(properties::FACING, v).unwrap();
    }
    if let Some(v) = old.get(properties::HALF) {
        state = state.with(properties::HALF, v).unwrap();
    }
    if let Some(v) = old.get(properties::TYPE_) {
        state = state.with(properties::TYPE_, v).unwrap();
    }
    state
}
fn random_stairs(block: Block, random: &mut (impl RandomSource + ?Sized)) -> BlockState {
    let facing = match ferraloom_util::Plane::Horizontal.faces()[random.next_int_bound(4) as usize]
    {
        ferraloom_util::Direction::North => properties::Facing::North,
        ferraloom_util::Direction::East => properties::Facing::East,
        ferraloom_util::Direction::South => properties::Facing::South,
        ferraloom_util::Direction::West => properties::Facing::West,
        _ => unreachable!(),
    };
    block
        .default_state()
        .with(properties::FACING, facing)
        .unwrap()
        .with(
            properties::HALF,
            if random.next_int_bound(2) == 0 {
                properties::Half::Top
            } else {
                properties::Half::Bottom
            },
        )
        .unwrap()
}
fn choose(
    normal: [BlockState; 2],
    mossy: [BlockState; 2],
    mossiness: f32,
    random: &mut (impl RandomSource + ?Sized),
) -> BlockState {
    let blocks = if random.next_float() < mossiness {
        mossy
    } else {
        normal
    };
    blocks[random.next_int_bound(2) as usize]
}

fn age_block(
    state: BlockState,
    mossiness: f32,
    tags: &BlockTags,
    random: &mut (impl RandomSource + ?Sized),
) -> BlockState {
    match state.block() {
        Block::STONE_BRICKS | Block::STONE | Block::CHISELED_STONE_BRICKS
            if random.next_float() < 0.5 =>
        {
            let normal = [
                Block::CRACKED_STONE_BRICKS.default_state(),
                random_stairs(Block::STONE_BRICK_STAIRS, random),
            ];
            let mossy = [
                Block::MOSSY_STONE_BRICKS.default_state(),
                random_stairs(Block::MOSSY_STONE_BRICK_STAIRS, random),
            ];
            choose(normal, mossy, mossiness, random)
        }
        Block::STONE_BRICKS | Block::STONE | Block::CHISELED_STONE_BRICKS => state,
        b if tags.contains(&"stairs".parse().unwrap(), b) => {
            if random.next_float() < 0.5 {
                choose(
                    [
                        Block::STONE_SLAB.default_state(),
                        Block::STONE_BRICK_SLAB.default_state(),
                    ],
                    [
                        Block::MOSSY_STONE_BRICK_STAIRS
                            .default_state()
                            .with_properties_of(state),
                        Block::MOSSY_STONE_BRICK_SLAB.default_state(),
                    ],
                    mossiness,
                    random,
                )
            } else {
                state
            }
        }
        b if tags.contains(&"slabs".parse().unwrap(), b) => {
            if random.next_float() < mossiness {
                Block::MOSSY_STONE_BRICK_SLAB
                    .default_state()
                    .with_properties_of(state)
            } else {
                state
            }
        }
        b if tags.contains(&"walls".parse().unwrap(), b) => {
            if random.next_float() < mossiness {
                Block::MOSSY_STONE_BRICK_WALL
                    .default_state()
                    .with_properties_of(state)
            } else {
                state
            }
        }
        Block::OBSIDIAN if random.next_float() < 0.15 => Block::CRYING_OBSIDIAN.default_state(),
        _ => state,
    }
}

#[cfg(test)]
#[path = "processor_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "processor_capped_tests.rs"]
mod capped_tests;
