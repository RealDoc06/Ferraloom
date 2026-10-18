use super::{ContextParam, EntityTarget, Identifier, LootFacts, LootParams, Result, codec};
use ferraloom_nbt::Tag;
use serde_json::Value;

/// Port of ContextNbtProvider's context argument codec.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NbtSource {
    Entity(EntityTarget),
    BlockEntity,
}
impl NbtSource {
    pub fn decode(v: &Value) -> Result<Self> {
        if codec::string(v)? == "block_entity" {
            Ok(Self::BlockEntity)
        } else {
            Ok(Self::Entity(EntityTarget::decode(v)?))
        }
    }
    pub fn param(self) -> ContextParam {
        match self {
            Self::BlockEntity => ContextParam::BlockEntity,
            Self::Entity(target) => target.param(),
        }
    }
}
/// Port of NbtProviders#CODEC, including shorthand context strings.
#[derive(Clone, Debug, PartialEq)]
pub enum NbtProvider {
    Context(NbtSource),
    Storage(Identifier),
}
impl NbtProvider {
    pub fn decode(v: &Value) -> Result<Self> {
        if v.is_string() {
            return Ok(Self::Context(NbtSource::decode(v)?));
        }
        Ok(match codec::kind(v)?.as_str() {
            "minecraft:context" => Self::Context(NbtSource::decode(codec::field(v, "target")?)?),
            "minecraft:storage" => Self::Storage(codec::id(codec::field(v, "source")?)?),
            _ => return Err(codec::error("unknown NBT provider")),
        })
    }
    pub fn get(&self, params: &LootParams, facts: &dyn LootFacts) -> Result<Option<Tag>> {
        match self {
            Self::Context(NbtSource::BlockEntity) => Ok(params.block_entity.clone()),
            Self::Context(NbtSource::Entity(target)) => params
                .entity(*target)
                .map(|entity| facts.entity_nbt(entity))
                .transpose(),
            Self::Storage(id) => Ok(Some(facts.storage(id)?)),
        }
    }
}
