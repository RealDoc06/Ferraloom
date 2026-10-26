//! StructureBlockEntity data and placement authorship. Structure loading/saving
//! remains in the separately required StructureBlock interaction virtual.
use crate::{
    block::{
        UpdateFlags,
        entity::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState},
    },
    entity::EntityRef,
    level::{Level, LevelError},
};
use ferraloom_data::{Block, properties as p};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::Identifier;
use ferraloom_util::{BlockPos, Mirror, Rotation, Vec3i};

/// Port of StructureBlockEntity's persisted template settings.
#[derive(Clone, Debug)]
pub struct StructureBlockEntity {
    pub parent: BlockEntityData,
    pub structure_name: Option<Identifier>,
    pub author: String,
    pub metadata: String,
    pub structure_pos: BlockPos,
    pub structure_size: Vec3i,
    pub mirror: Mirror,
    pub rotation: Rotation,
    pub mode: p::Mode,
    pub ignore_entities: bool,
    pub strict: bool,
    pub powered: bool,
    pub show_air: bool,
    pub show_bounding_box: bool,
    pub integrity: f32,
    pub seed: i64,
}
impl StructureBlockEntity {
    /// Port of StructureBlockEntity#createRandom.
    pub fn create_random(seed: i64) -> ferraloom_util::random::Random {
        ferraloom_util::random::Random::Legacy(ferraloom_util::random::LegacyRandomSource::new(
            if seed == 0 {
                crate::level::get_millis()
            } else {
                seed
            },
        ))
    }

    pub fn new(parent: BlockEntityData) -> Self {
        let mode = parent.block_state.get(p::MODE).expect("structure mode");
        Self {
            parent,
            structure_name: None,
            author: String::new(),
            metadata: String::new(),
            structure_pos: BlockPos::new(0, 1, 0),
            structure_size: Vec3i::ZERO,
            mirror: Mirror::None,
            rotation: Rotation::None,
            mode,
            ignore_entities: true,
            strict: false,
            powered: false,
            show_air: false,
            show_bounding_box: true,
            integrity: 1.0,
            seed: 0,
        }
    }
}
/// Port of StructureBlock#setPlacedBy / StructureBlockEntity#createdBy. Creator
/// plain text is a required canonical entity read, never a guessed type label.
pub(super) fn set_placed_by(
    pos: BlockPos,
    by: Option<EntityRef>,
    level: &mut Level,
) -> Result<(), LevelError> {
    let Some(by) = by else {
        return Ok(());
    };
    let Some(handle) = level.block_entity_ref(pos)? else {
        return Ok(());
    };
    if !matches!(
        level.block_entity_state(handle)?,
        BlockEntityState::Structure(_)
    ) {
        return Ok(());
    }
    level.with_block_entity_callback(handle, |level| {
        let author = level.entity_plain_text_name(by)?;
        level
            .block_entity_mut::<StructureBlockEntity>(handle)?
            .author = author;
        Ok(())
    })
}
pub struct StructureBehavior;
pub static STRUCTURE_ENTITY: StructureBehavior = StructureBehavior;
impl BlockEntity for StructureBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::Structure(entity) = state else {
            unreachable!()
        };
        entity.structure_name = tag
            .get_string("name")
            .map(|value| value.to_string_lossy())
            .filter(|value| !value.is_empty())
            .and_then(|value| Identifier::parse(&value).ok());
        entity.author = tag
            .get_string("author")
            .map(|v| v.to_string_lossy())
            .unwrap_or_default();
        entity.metadata = tag
            .get_string("metadata")
            .map(|v| v.to_string_lossy())
            .unwrap_or_default();
        entity.structure_pos = BlockPos::new(
            tag.get_int_or("posX", 0).clamp(-48, 48),
            tag.get_int_or("posY", 1).clamp(-48, 48),
            tag.get_int_or("posZ", 0).clamp(-48, 48),
        );
        entity.structure_size = Vec3i::new(
            tag.get_int_or("sizeX", 0).clamp(0, 48),
            tag.get_int_or("sizeY", 0).clamp(0, 48),
            tag.get_int_or("sizeZ", 0).clamp(0, 48),
        );
        entity.rotation = match tag
            .get_string("rotation")
            .map(|v| v.to_string_lossy())
            .as_deref()
        {
            Some("CLOCKWISE_90") => Rotation::Clockwise90,
            Some("CLOCKWISE_180") => Rotation::Clockwise180,
            Some("COUNTERCLOCKWISE_90") => Rotation::CounterClockwise90,
            _ => Rotation::None,
        };
        entity.mirror = match tag
            .get_string("mirror")
            .map(|v| v.to_string_lossy())
            .as_deref()
        {
            Some("LEFT_RIGHT") => Mirror::LeftRight,
            Some("FRONT_BACK") => Mirror::FrontBack,
            _ => Mirror::None,
        };
        entity.mode = match tag
            .get_string("mode")
            .map(|v| v.to_string_lossy())
            .as_deref()
        {
            Some("SAVE") => p::Mode::Save,
            Some("LOAD") => p::Mode::Load,
            Some("CORNER") => p::Mode::Corner,
            _ => p::Mode::Data,
        };
        entity.ignore_entities = tag.get_boolean_or("ignoreEntities", true);
        entity.strict = tag.get_boolean_or("strict", false);
        entity.powered = tag.get_boolean_or("powered", false);
        entity.show_air = tag.get_boolean_or("showair", false);
        entity.show_bounding_box = tag.get_boolean_or("showboundingbox", true);
        entity.integrity = tag.get_float_or("integrity", 1.0);
        entity.seed = tag.get_long_or("seed", 0);
    }
    fn load_additional_live(
        &self,
        handle: BlockEntityRef,
        tag: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let context = level.context().clone();
        self.load_additional(
            level.block_entity_state_mut(handle)?,
            tag,
            context.registry_context()?,
        );
        let mode = level.block_entity::<StructureBlockEntity>(handle)?.mode;
        if level.block_entity::<BlockEntityData>(handle)?.has_level {
            let state = level.get_block_state(handle.pos)?;
            if state.block() == Block::STRUCTURE_BLOCK {
                level.set_block(
                    handle.pos,
                    state.with(p::MODE, mode).unwrap(),
                    UpdateFlags::CLIENTS,
                    crate::block::UPDATE_LIMIT,
                )?;
            }
        }
        Ok(())
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Structure(e) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        for (key, value) in [
            ("name", e.structure_name.as_ref().map_or("", |v| v.as_str())),
            ("author", e.author.as_str()),
            ("metadata", e.metadata.as_str()),
        ] {
            tag.insert(key, Tag::String(value.into()));
        }
        for (key, value) in [
            ("posX", e.structure_pos.x),
            ("posY", e.structure_pos.y),
            ("posZ", e.structure_pos.z),
            ("sizeX", e.structure_size.x),
            ("sizeY", e.structure_size.y),
            ("sizeZ", e.structure_size.z),
        ] {
            tag.insert(key, Tag::Int(value));
        }
        let rotation = match e.rotation {
            Rotation::None => "NONE",
            Rotation::Clockwise90 => "CLOCKWISE_90",
            Rotation::Clockwise180 => "CLOCKWISE_180",
            Rotation::CounterClockwise90 => "COUNTERCLOCKWISE_90",
        };
        let mirror = match e.mirror {
            Mirror::None => "NONE",
            Mirror::LeftRight => "LEFT_RIGHT",
            Mirror::FrontBack => "FRONT_BACK",
        };
        let mode = match e.mode {
            p::Mode::Save => "SAVE",
            p::Mode::Load => "LOAD",
            p::Mode::Corner => "CORNER",
            p::Mode::Data => "DATA",
            _ => unreachable!("structure mode"),
        };
        for (key, value) in [("rotation", rotation), ("mirror", mirror), ("mode", mode)] {
            tag.insert(key, Tag::String(value.into()));
        }
        for (key, value) in [
            ("ignoreEntities", e.ignore_entities),
            ("strict", e.strict),
            ("powered", e.powered),
            ("showair", e.show_air),
            ("showboundingbox", e.show_bounding_box),
        ] {
            tag.insert(key, Tag::Byte(i8::from(value)));
        }
        tag.insert("integrity", Tag::Float(e.integrity));
        tag.insert("seed", Tag::Long(e.seed));
        Ok(tag)
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        self.save_additional(state)
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
}
