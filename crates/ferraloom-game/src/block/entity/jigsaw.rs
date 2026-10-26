//! JigsawBlockEntity data codecs. Structure generation remains in JigsawPlacement.
use super::{BlockEntity, BlockEntityData, BlockEntityState};
use ferraloom_data::properties as p;
use ferraloom_nbt::{CompoundTag, NbtString, Tag};
use ferraloom_protocol::Identifier;

/// Port of JigsawBlockEntity.JointType.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JointType {
    Rollable,
    Aligned,
}
impl JointType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rollable => "rollable",
            Self::Aligned => "aligned",
        }
    }
}

/// Port of JigsawBlockEntity. The pool is a ResourceKey, so loading validates
/// its identifier syntax without requiring that the pool is registered.
#[derive(Clone, Debug)]
pub struct JigsawBlockEntity {
    pub parent: BlockEntityData,
    pub name: Identifier,
    pub target: Identifier,
    pub pool: Identifier,
    pub joint: JointType,
    pub final_state: NbtString,
    pub placement_priority: i32,
    pub selection_priority: i32,
}
impl JigsawBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        let empty = Identifier::parse("minecraft:empty").expect("native constant");
        Self {
            parent,
            name: empty.clone(),
            target: empty.clone(),
            pool: empty,
            joint: JointType::Rollable,
            final_state: "minecraft:air".into(),
            placement_priority: 0,
            selection_priority: 0,
        }
    }
    /// Port of StructureTemplate#getDefaultJointType, used only when loading.
    fn default_joint(&self) -> JointType {
        match self
            .parent
            .block_state
            .get(p::ORIENTATION)
            .expect("JigsawBlock orientation")
        {
            p::Orientation::NorthUp
            | p::Orientation::SouthUp
            | p::Orientation::EastUp
            | p::Orientation::WestUp => JointType::Aligned,
            _ => JointType::Rollable,
        }
    }
}
fn identifier(tag: &CompoundTag, key: &str) -> Identifier {
    tag.get_string(key)
        .and_then(|value| value.to_string_checked().ok())
        .and_then(|value| Identifier::parse(&value).ok())
        .unwrap_or_else(|| Identifier::parse("minecraft:empty").expect("native constant"))
}
pub(super) struct JigsawBehavior;
pub(super) static JIGSAW_ENTITY: JigsawBehavior = JigsawBehavior;
impl BlockEntity for JigsawBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::Jigsaw(entity) = state else {
            unreachable!()
        };
        entity.name = identifier(tag, "name");
        entity.target = identifier(tag, "target");
        entity.pool = identifier(tag, "pool");
        entity.final_state = tag
            .get_string("final_state")
            .cloned()
            .unwrap_or_else(|| "minecraft:air".into());
        entity.joint = match tag
            .get_string("joint")
            .map(NbtString::to_string_lossy)
            .as_deref()
        {
            Some("rollable") => JointType::Rollable,
            Some("aligned") => JointType::Aligned,
            _ => entity.default_joint(),
        };
        entity.placement_priority = tag.get_int_or("placement_priority", 0);
        entity.selection_priority = tag.get_int_or("selection_priority", 0);
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Jigsaw(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        tag.insert("name", Tag::String(entity.name.as_str().into()));
        tag.insert("target", Tag::String(entity.target.as_str().into()));
        tag.insert("pool", Tag::String(entity.pool.as_str().into()));
        tag.insert("final_state", Tag::String(entity.final_state.clone()));
        tag.insert("joint", Tag::String(entity.joint.as_str().into()));
        tag.insert("placement_priority", Tag::Int(entity.placement_priority));
        tag.insert("selection_priority", Tag::Int(entity.selection_priority));
        Ok(tag)
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        self.save_additional(state)
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests;
