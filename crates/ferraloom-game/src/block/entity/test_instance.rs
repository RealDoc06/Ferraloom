//! Test-instance receiver data and its synchronous update notification.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::level::{Level, LevelError};
use ferraloom_data::Block;
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::{
    chat::Component,
    codec::VarInt,
    play::{Rotation, TestInstanceData, TestInstanceStatus, Vec3i},
};
use ferraloom_util::BlockPos;

/// Port of TestInstanceBlockEntity.ErrorMarker.
#[derive(Clone, Debug, PartialEq)]
pub struct ErrorMarker {
    pub pos: BlockPos,
    pub text: Component,
}

/// Port of TestInstanceBlockEntity. GameTest execution and template operations
/// are separate required services; this state does not start a test or a ticker.
#[derive(Clone, Debug)]
pub struct TestInstanceBlockEntity {
    pub parent: BlockEntityData,
    pub data: TestInstanceData,
    pub error_markers: Vec<ErrorMarker>,
}
impl TestInstanceBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            data: TestInstanceData {
                test: None,
                size: Vec3i {
                    x: VarInt(0),
                    y: VarInt(0),
                    z: VarInt(0),
                },
                rotation: Rotation::None,
                ignore_entities: false,
                status: TestInstanceStatus::Cleared,
                error_message: None,
            },
            error_markers: Vec::new(),
        }
    }
    fn load_errors(&mut self, tag: &CompoundTag) {
        // TagValueInput#read recovers valid list entries and fixed-size partials.
        self.error_markers = tag
            .get_list("errors")
            .into_iter()
            .flat_map(|list| list.iter())
            .filter_map(|tag| {
                let entry = tag.as_compound()?;
                let [x, y, z] = codec_vec3(entry.get("pos")?)?;
                Some(ErrorMarker {
                    pos: BlockPos::new(x, y, z),
                    text: Component::from_nbt(entry.get("text")?.clone()).ok()?,
                })
            })
            .collect();
    }
    fn save(&self) -> ferraloom_protocol::Result<CompoundTag> {
        let mut data = CompoundTag::new();
        if let Some(test) = &self.data.test {
            data.insert("test", Tag::String(test.as_str().into()));
        }
        data.insert(
            "size",
            Tag::IntArray(vec![
                self.data.size.x.0,
                self.data.size.y.0,
                self.data.size.z.0,
            ]),
        );
        data.insert(
            "rotation",
            Tag::String(
                match self.data.rotation {
                    Rotation::None => "none",
                    Rotation::Clockwise90 => "clockwise_90",
                    Rotation::Clockwise180 => "180",
                    Rotation::Counterclockwise90 => "counterclockwise_90",
                }
                .into(),
            ),
        );
        data.insert(
            "ignore_entities",
            Tag::Byte(i8::from(self.data.ignore_entities)),
        );
        data.insert(
            "status",
            Tag::String(
                match self.data.status {
                    TestInstanceStatus::Cleared => "cleared",
                    TestInstanceStatus::Running => "running",
                    TestInstanceStatus::Finished => "finished",
                }
                .into(),
            ),
        );
        if let Some(message) = &self.data.error_message {
            data.insert("error_message", message.to_nbt()?);
        }
        let mut tag = CompoundTag::new();
        tag.insert("data", Tag::Compound(data));
        if !self.error_markers.is_empty() {
            tag.insert(
                "errors",
                Tag::List(ListTag(
                    self.error_markers
                        .iter()
                        .map(|error| {
                            let mut tag = CompoundTag::new();
                            tag.insert(
                                "pos",
                                Tag::IntArray(vec![error.pos.x, error.pos.y, error.pos.z]),
                            );
                            tag.insert("text", error.text.to_nbt()?);
                            Ok(Tag::Compound(tag))
                        })
                        .collect::<ferraloom_protocol::Result<_>>()?,
                )),
            );
        }
        Ok(tag)
    }
}

// Vec3i.CODEC uses Number.intValue, then Util.fixedSize's first-three partial;
// the NBT numeric getters instead floor floating values and are not this codec.
fn codec_vec3(tag: &Tag) -> Option<[i32; 3]> {
    let values: Vec<i32> = match tag {
        Tag::ByteArray(values) => values.iter().map(|&value| i32::from(value)).collect(),
        Tag::IntArray(values) => values.clone(),
        Tag::LongArray(values) => values.iter().map(|&value| value as i32).collect(),
        Tag::List(values) => values
            .iter()
            .map(|value| match value {
                Tag::Byte(value) => Some(i32::from(*value)),
                Tag::Short(value) => Some(i32::from(*value)),
                Tag::Int(value) => Some(*value),
                Tag::Long(value) => Some(*value as i32),
                Tag::Float(value) => Some(*value as i32),
                Tag::Double(value) => Some(*value as i32),
                _ => None,
            })
            .collect::<Option<_>>()?,
        _ => return None,
    };
    values.get(..3)?.try_into().ok()
}
fn load_data(tag: &CompoundTag) -> Option<TestInstanceData> {
    let data = tag.get_compound("data")?;
    let [x, y, z] = codec_vec3(data.get("size")?)?;
    let rotation = match data
        .get_string("rotation")?
        .to_string_checked()
        .ok()?
        .as_str()
    {
        "none" => Rotation::None,
        "clockwise_90" => Rotation::Clockwise90,
        "180" => Rotation::Clockwise180,
        "counterclockwise_90" => Rotation::Counterclockwise90,
        _ => return None,
    };
    let status = match data
        .get_string("status")?
        .to_string_checked()
        .ok()?
        .as_str()
    {
        "cleared" => TestInstanceStatus::Cleared,
        "running" => TestInstanceStatus::Running,
        "finished" => TestInstanceStatus::Finished,
        _ => return None,
    };
    Some(TestInstanceData {
        // ResourceKey.CODEC validates syntax, without requiring a bound holder.
        test: data
            .get_string("test")
            .and_then(|value| value.to_string_checked().ok())
            .and_then(|value| value.parse().ok()),
        size: Vec3i {
            x: VarInt(x),
            y: VarInt(y),
            z: VarInt(z),
        },
        rotation,
        ignore_entities: data.get("ignore_entities")?.as_double()? != 0.0,
        status,
        error_message: data
            .get("error_message")
            .and_then(|value| Component::from_nbt(value.clone()).ok()),
    })
}

pub(super) struct TestInstanceBehavior;
pub(super) static TEST_INSTANCE_ENTITY: TestInstanceBehavior = TestInstanceBehavior;
impl BlockEntity for TestInstanceBehavior {
    fn set_changed_live(
        &self,
        handle: BlockEntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        handle.set_changed_base(level)?;
        let data = level.block_entity::<BlockEntityData>(handle)?;
        if data.has_level {
            level.send_block_updated(handle.pos, Block::AIR.default_state(), data.block_state);
        }
        Ok(())
    }
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::TestInstance(entity) = state else {
            unreachable!()
        };
        if let Some(data) = load_data(tag) {
            entity.data = data;
        }
        entity.load_errors(tag);
    }
    fn load_additional_live(
        &self,
        handle: BlockEntityRef,
        tag: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if let Some(data) = load_data(tag) {
            level
                .block_entity_mut::<TestInstanceBlockEntity>(handle)?
                .data = data;
            handle.set_changed(level)?;
        }
        // set(Data)'s callbacks see the old error list. Resolve this generation
        // again after the complete callback, then replace errors like native.
        level
            .block_entity_mut::<TestInstanceBlockEntity>(handle)?
            .load_errors(tag);
        Ok(())
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::TestInstance(entity) = state else {
            unreachable!()
        };
        entity.save()
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        state.save_custom_only()
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests;
