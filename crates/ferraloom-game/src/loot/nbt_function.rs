use super::{EvalContext, LootError, NbtProvider, Result, codec};
use ferraloom_nbt::{CompoundTag, ListTag, NbtPath, Tag};
use ferraloom_protocol::component::*;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NbtMergeStrategy {
    Replace,
    Append,
    Merge,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CopyOperation {
    pub source: String,
    pub target: String,
    pub op: NbtMergeStrategy,
}
/// Port of CopyCustomDataFunction. Mutable multi-target merge awaits the NBT
/// path mutation API; such requests return Unsupported rather than overwriting
/// distinct target compounds with a single merged value.
#[derive(Clone, Debug, PartialEq)]
pub struct CopyCustomData {
    pub source: NbtProvider,
    pub operations: Vec<CopyOperation>,
}
impl CopyCustomData {
    pub(crate) fn decode(v: &Value) -> Result<Self> {
        Ok(Self {
            source: NbtProvider::decode(codec::field(v, "source")?)?,
            operations: codec::list(codec::field(v, "ops")?, |v| {
                let source = codec::string(codec::field(v, "source")?)?.to_owned();
                let target = codec::string(codec::field(v, "target")?)?.to_owned();
                NbtPath::parse(&source).map_err(|e| codec::error(e.to_string()))?;
                NbtPath::parse(&target).map_err(|e| codec::error(e.to_string()))?;
                let op = match codec::string(codec::field(v, "op")?)? {
                    "replace" => NbtMergeStrategy::Replace,
                    "append" => NbtMergeStrategy::Append,
                    "merge" => NbtMergeStrategy::Merge,
                    _ => return Err(codec::error("unknown NBT merge strategy")),
                };
                Ok(CopyOperation { source, target, op })
            })?,
        })
    }
    pub(crate) fn apply(&self, mut stack: ItemStack, c: &EvalContext<'_>) -> Result<ItemStack> {
        let Some(source) = self.source.get(c.params, c.facts)? else {
            return Ok(stack);
        };
        let mut result = None;
        for operation in &self.operations {
            let sources = NbtPath::parse(&operation.source)
                .expect("validated path")
                .get(&source)
                .unwrap_or_default();
            if sources.is_empty() {
                continue;
            }
            let target = result.get_or_insert_with(|| {
                match stack
                    .components()
                    .expect("validated components")
                    .get(ComponentTypeId::CUSTOM_DATA)
                {
                    Some(DataComponent::CustomData(v)) => v.0.clone(),
                    _ => Tag::Compound(CompoundTag::new()),
                }
            });
            let path = NbtPath::parse(&operation.target).expect("validated path");
            match operation.op {
                NbtMergeStrategy::Replace => {
                    let _ = path.set(target, sources.last().expect("nonempty"));
                }
                NbtMergeStrategy::Append => {
                    if let Ok(targets) = path.get_or_create(target, &Tag::List(ListTag::default()))
                    {
                        let lists = targets.iter().filter(|v| matches!(v, Tag::List(_))).count();
                        if lists != 0 && lists != targets.len() {
                            return Err(LootError::Unsupported(
                                "nbt_path.mutable_mixed_targets".into(),
                            ));
                        }
                        if lists != 0 {
                            let _ = path.insert(-1, target, &sources);
                        }
                    }
                }
                NbtMergeStrategy::Merge => {
                    if let Ok(targets) =
                        path.get_or_create(target, &Tag::Compound(CompoundTag::new()))
                    {
                        let mut compounds = targets.iter().filter_map(Tag::as_compound);
                        if let Some(first) = compounds.next() {
                            if targets.iter().any(|v| v.as_compound().is_none())
                                || compounds.any(|v| v != first)
                            {
                                return Err(LootError::Unsupported(
                                    "nbt_path.mutable_distinct_targets".into(),
                                ));
                            }
                            let mut merged = first.clone();
                            for source in &sources {
                                if let Some(source) = source.as_compound() {
                                    merged.merge(source);
                                }
                            }
                            let _ = path.set(target, &Tag::Compound(merged));
                        }
                    }
                }
            }
        }
        if let Some(Tag::Compound(result)) = result {
            if let Some(s) = &mut stack.0 {
                if result.is_empty() {
                    s.components.remove(ComponentTypeId::CUSTOM_DATA);
                } else {
                    s.components
                        .set(DataComponent::CustomData(CustomData(Tag::Compound(result))));
                }
            }
        }
        Ok(stack)
    }
}
