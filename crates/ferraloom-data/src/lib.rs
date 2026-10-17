//! Pinned vanilla 26.3 registry facts, generated at build time without Java.
//!
//! ```
//! use ferraloom_data::{Block, properties};
//! let stairs = Block::OAK_STAIRS.default_state();
//! let wet = stairs.with(properties::WATERLOGGED, true).unwrap();
//! assert_eq!(wet.get(properties::WATERLOGGED), Some(true));
//! assert_eq!(wet.block(), Block::OAK_STAIRS);
//! ```

pub mod datapack;

use std::marker::PhantomData;

/// Vanilla global block-state ID, in `Block.BLOCK_STATE_REGISTRY` order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlockState(u16);

impl BlockState {
    pub const fn from_id(id: u16) -> Option<Self> {
        if (id as usize) < BLOCK_STATES.len() {
            Some(Self(id))
        } else {
            None
        }
    }

    pub const fn id(self) -> u16 {
        self.0
    }

    pub const fn data(self) -> &'static BlockStateData {
        &BLOCK_STATES[self.0 as usize]
    }

    pub const fn block(self) -> Block {
        self.data().block
    }

    /// Port of StateHolder#getValues, in StateDefinition's sorted property order.
    pub fn properties(self) -> impl Iterator<Item = (&'static str, &'static str)> {
        let block = self.block().data();
        block.properties.iter().rev().map(move |layout| {
            let index = (self.0 - block.first_state) / layout.stride % layout.names.len() as u16;
            (layout.name, layout.names[index as usize])
        })
    }

    /// Port of StateDefinition#appendPropertyCodec and NbtUtils#readBlockState.
    /// Missing/invalid values keep the default, and unknown properties are ignored.
    pub fn from_properties<'a>(
        block: Block,
        properties: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Option<Self> {
        let mut state = block.default_state();
        let data = block.data();
        for (name, value) in properties {
            let Some(layout) = data.properties.iter().find(|p| p.name == name) else {
                continue;
            };
            let Some(new) = layout.names.iter().position(|&v| v == value) else {
                continue;
            };
            let old = (state.0 - data.first_state) / layout.stride % layout.names.len() as u16;
            state = Self(state.0 - old * layout.stride + new as u16 * layout.stride);
        }
        Some(state)
    }

    /// Port of StateDefinition's sorted properties and cartesian-product order.
    pub fn get<T: PropertyValue>(self, property: Property<T>) -> Option<T> {
        let block = self.block().data();
        let layout = block.properties.iter().find(|p| p.id == property.id)?;
        let index = (self.0 - block.first_state) / layout.stride % layout.values.len() as u16;
        T::from_index(layout.values[index as usize])
    }

    /// Port of StateHolder#setValue. Absent properties and invalid values fail.
    pub fn with<T: PropertyValue>(self, property: Property<T>, value: T) -> Option<Self> {
        let block = self.block().data();
        let layout = block.properties.iter().find(|p| p.id == property.id)?;
        let new = layout.values.iter().position(|&v| v == value.index())? as u16;
        let old = (self.0 - block.first_state) / layout.stride % layout.values.len() as u16;
        Some(Self(self.0 - old * layout.stride + new * layout.stride))
    }
}

impl BlockState {
    /// Port of BlockStateBase#rotate. Tables preserve every block's override,
    /// including stairs, rails, multi-face connections and jigsaw orientation.
    pub const fn rotate(self, rotation: ferraloom_util::Rotation) -> Self {
        use ferraloom_util::Rotation;
        match rotation {
            Rotation::None => self,
            Rotation::Clockwise90 => Self(STATE_TRANSFORM_CLOCKWISE_90[self.0 as usize]),
            Rotation::Clockwise180 => Self(STATE_TRANSFORM_CLOCKWISE_180[self.0 as usize]),
            Rotation::CounterClockwise90 => {
                Self(STATE_TRANSFORM_COUNTERCLOCKWISE_90[self.0 as usize])
            }
        }
    }
    /// Port of BlockStateBase#mirror. Apply before rotate for templates.
    pub const fn mirror(self, mirror: ferraloom_util::Mirror) -> Self {
        use ferraloom_util::Mirror;
        match mirror {
            Mirror::None => self,
            Mirror::LeftRight => Self(STATE_TRANSFORM_LEFT_RIGHT[self.0 as usize]),
            Mirror::FrontBack => Self(STATE_TRANSFORM_FRONT_BACK[self.0 as usize]),
        }
    }
}

/// A typed property key. Use the generated constants in [`properties`].
#[derive(Clone, Copy, Debug)]
pub struct Property<T> {
    id: u8,
    marker: PhantomData<T>,
}

/// A value represented by a generated property enum, `bool`, or `u8`.
pub trait PropertyValue: Copy {
    fn index(self) -> u8;
    fn from_index(index: u8) -> Option<Self>;
}

impl PropertyValue for bool {
    fn index(self) -> u8 {
        u8::from(self)
    }
    fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
}

impl PropertyValue for u8 {
    fn index(self) -> u8 {
        self
    }
    fn from_index(index: u8) -> Option<Self> {
        Some(index)
    }
}

#[derive(Clone, Copy, Debug)]
struct PropertyLayout {
    id: u8,
    name: &'static str,
    names: &'static [&'static str],
    stride: u16,
    values: &'static [u8],
    integer_values: Option<&'static [i32]>,
}

#[derive(Clone, Copy, Debug)]
pub struct RegistryInfo {
    pub name: &'static str,
    pub protocol_id: u16,
    pub count: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct BlockData {
    pub default_state: BlockState,
    pub first_state: u16,
    pub state_count: u16,
    pub explosion_resistance: f32,
    pub friction: f32,
    pub speed_factor: f32,
    pub jump_factor: f32,
    properties: &'static [PropertyLayout],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ShapeId(u16);

impl ShapeId {
    pub const fn id(self) -> u16 {
        self.0
    }
    pub const fn boxes(self) -> &'static [Aabb] {
        SHAPES[self.0 as usize]
    }
}

/// Port of vanilla 26.3 PushReaction (which renamed the older variants).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PushReaction {
    PushPull,
    Push,
    Popped,
    Immoveable,
    IgnoreEntity,
}

#[derive(Clone, Copy, Debug)]
pub struct SoundType {
    pub name: &'static str,
    pub volume: f32,
    pub pitch: f32,
    pub break_sound: registries::sound_event::Id,
    pub step_sound: registries::sound_event::Id,
    pub place_sound: registries::sound_event::Id,
    pub hit_sound: registries::sound_event::Id,
    pub fall_sound: registries::sound_event::Id,
}

#[derive(Clone, Copy, Debug)]
pub struct FluidState {
    pub fluid_type: registries::fluid::Id,
    pub amount: u8,
    pub source: bool,
    pub empty: bool,
    pub falling: bool,
}

/// Port of BlockBehaviour.BlockStateBase's cached state facts.
#[derive(Clone, Copy, Debug)]
pub struct BlockStateData {
    pub block: Block,
    pub destroy_time: f32,
    pub light_emission: u8,
    pub collision: ShapeId,
    pub outline: ShapeId,
    pub occlusion: ShapeId,
    /// EmptyBlockGetter at ZERO is a baseline; these states need live queries.
    pub context_dependent_shape: bool,
    pub solid_render: bool,
    /// Vanilla 26.3 calls this `getLightDampening`.
    pub opacity: u8,
    pub propagates_skylight: bool,
    pub use_shape_for_light_occlusion: bool,
    pub map_color: u8,
    pub requires_correct_tool: bool,
    pub random_ticking: bool,
    pub replaceable: bool,
    pub push_reaction: PushReaction,
    pub sound: &'static SoundType,
    pub fluid: FluidState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
}

#[derive(Clone, Copy, Debug)]
pub struct ItemData {
    pub max_stack_size: u32,
    pub max_damage: u32,
    pub rarity: Rarity,
    pub block: Option<Block>,
    /// Encoded by vanilla's DataComponentMap codec with registry-aware JsonOps.
    pub default_components_json: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MobCategory {
    Monster,
    Creature,
    Ambient,
    Axolotls,
    UndergroundWaterCreature,
    WaterCreature,
    WaterAmbient,
    Misc,
}

#[derive(Clone, Copy, Debug)]
pub struct EntityTypeData {
    pub width: f32,
    pub height: f32,
    pub eye_height: f32,
    pub fixed_dimensions: bool,
    pub category: MobCategory,
    pub tracking_range: u32,
    pub update_interval: u32,
    pub fire_immune: bool,
    pub summonable: bool,
    pub attributes: &'static [(registries::attribute::Id, f64)],
}

include!(concat!(env!("OUT_DIR"), "/data.rs"));

impl Block {
    pub const fn data(self) -> &'static BlockData {
        &BLOCKS[self.protocol_id() as usize]
    }
    pub const fn default_state(self) -> BlockState {
        self.data().default_state
    }
}

impl Item {
    pub const fn data(self) -> &'static ItemData {
        &ITEMS[self.protocol_id() as usize]
    }
}

impl EntityType {
    pub const fn data(self) -> &'static EntityTypeData {
        &ENTITY_TYPES[self.protocol_id() as usize]
    }
}

#[cfg(test)]
mod tests;

/// Dynamic property failures preserve the distinction needed by state providers:
/// absent/non-integer properties skip sampling, invalid sampled values fail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockStatePropertyError {
    MissingOrNonInteger,
    InvalidValue(i32),
}
impl BlockState {
    /// Port of BlockState#withPropertiesOf. Property identity includes its
    /// allowed values; equally named properties with different ranges differ.
    pub fn with_properties_of(mut self, other: Self) -> Self {
        let target = self.block().data();
        let source = other.block().data();
        for from in source.properties {
            if let Some(to) = target
                .properties
                .iter()
                .find(|to| to.id == from.id && to.names == from.names)
            {
                let source_index =
                    (other.0 - source.first_state) / from.stride % from.values.len() as u16;
                let target_index =
                    (self.0 - target.first_state) / to.stride % to.values.len() as u16;
                self = Self(self.0 - target_index * to.stride + source_index * to.stride);
            }
        }
        self
    }
    pub fn has_integer_property(self, name: &str) -> bool {
        self.block()
            .data()
            .properties
            .iter()
            .any(|p| p.name == name && p.integer_values.is_some())
    }
    /// Port of StateHolder#setValue for an IntegerProperty selected by name.
    /// Numeric values are generated from the layout; no runtime value parsing.
    pub fn with_integer_property(
        self,
        name: &str,
        value: i32,
    ) -> Result<Self, BlockStatePropertyError> {
        let data = self.block().data();
        let property = data
            .properties
            .iter()
            .find(|p| p.name == name)
            .ok_or(BlockStatePropertyError::MissingOrNonInteger)?;
        let values = property
            .integer_values
            .ok_or(BlockStatePropertyError::MissingOrNonInteger)?;
        let new = values
            .iter()
            .position(|&v| v == value)
            .ok_or(BlockStatePropertyError::InvalidValue(value))? as u16;
        let old = (self.0 - data.first_state) / property.stride % values.len() as u16;
        Ok(Self(self.0 - old * property.stride + new * property.stride))
    }
}

impl Block {
    /// Concrete vanilla Java class, extracted from the registered block object.
    pub const fn class(self) -> BlockClass {
        BLOCK_CLASSES[self.protocol_id() as usize]
    }

    /// Java instanceof, including transitive superclasses and interfaces.
    pub const fn is_instance_of(self, class: BlockClass) -> bool {
        let index = class as usize;
        BLOCK_CLASS_INSTANCES[self.class() as usize][index / 64] & (1u64 << (index % 64)) != 0
    }
}
