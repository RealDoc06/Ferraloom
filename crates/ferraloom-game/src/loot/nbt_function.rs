use super::{EvalContext, NbtProvider, Result, codec};
use crate::loot::codec::DecodeInput;
use ferraloom_nbt::{CompoundTag, ListTag, NbtPath, Tag};
use ferraloom_protocol::component::*;

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
/// Port of CopyCustomDataFunction, including independent live target mutation.
#[derive(Clone, Debug, PartialEq)]
pub struct CopyCustomData {
    pub source: NbtProvider,
    pub operations: Vec<CopyOperation>,
}
impl CopyCustomData {
    pub(crate) fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
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
        let Some(source) = self.source.get_with_runtime(c.params, c.runtime)? else {
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
            if result.is_none() {
                result = Some(
                    match stack.components()?.get(ComponentTypeId::CUSTOM_DATA) {
                        Some(DataComponent::CustomData(value)) => value.0.copy(),
                        _ => Tag::Compound(CompoundTag::new()),
                    },
                );
            }
            let target = result.as_mut().expect("initialized target");
            let path = NbtPath::parse(&operation.target).expect("validated path");
            match operation.op {
                NbtMergeStrategy::Replace => {
                    let _ = path.set(target, sources.last().expect("nonempty"));
                }
                NbtMergeStrategy::Append => {
                    let _ = path.get_or_create_and_update(
                        target,
                        &Tag::List(ListTag::default()),
                        |target| {
                            if let Tag::List(list) = target {
                                list.extend(sources.iter().map(Tag::copy));
                            }
                        },
                    );
                }
                NbtMergeStrategy::Merge => {
                    let _ = path.get_or_create_and_update(
                        target,
                        &Tag::Compound(CompoundTag::new()),
                        |target| {
                            if let Tag::Compound(compound) = target {
                                for source in &sources {
                                    if let Some(source) = source.as_compound() {
                                        compound.merge(source);
                                    }
                                }
                            }
                        },
                    );
                }
            }
        }
        if let Some(Tag::Compound(result)) = result
            && let Some(s) = &mut stack.0
        {
            if result.is_empty() {
                s.components.remove(ComponentTypeId::CUSTOM_DATA);
            } else {
                s.components
                    .set(DataComponent::CustomData(CustomData(Tag::Compound(result))));
            }
        }
        Ok(stack)
    }
}
