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
            let new = if layout.integer {
                integer_property_value(value)
                    .and_then(|value| layout.values.iter().position(|&v| v == value))
            } else {
                layout.names.iter().position(|&v| v == value)
            };
            let Some(new) = new else { continue };
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

/// Port of IntegerProperty#getValue through Integer.parseInt/Character.digit.
fn integer_property_value(value: &str) -> Option<u8> {
    // Java's char overload accepts BMP decimal digits, including fullwidth digits.
    const ZEROES: &[u16] = &[
        0x0030, 0x0660, 0x06f0, 0x07c0, 0x0966, 0x09e6, 0x0a66, 0x0ae6, 0x0b66, 0x0be6, 0x0c66,
        0x0ce6, 0x0d66, 0x0de6, 0x0e50, 0x0ed0, 0x0f20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946,
        0x19d0, 0x1a80, 0x1a90, 0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0,
        0xa9f0, 0xaa50, 0xabf0, 0xff10,
    ];
    let (negative, digits) = value.strip_prefix('-').map_or_else(
        || (false, value.strip_prefix('+').unwrap_or(value)),
        |digits| (true, digits),
    );
    if digits.is_empty() {
        return None;
    }
    let mut result = 0i32;
    for unit in digits.encode_utf16() {
        let digit = ZEROES
            .iter()
            .find_map(|&zero| unit.checked_sub(zero).filter(|&digit| digit < 10))?;
        result = result.checked_mul(10)?.checked_sub(i32::from(digit))?;
    }
    u8::try_from(if negative {
        result
    } else {
        result.checked_neg()?
    })
    .ok()
}

#[derive(Clone, Copy, Debug)]
struct PropertyLayout {
    id: u8,
    name: &'static str,
    names: &'static [&'static str],
    integer: bool,
    stride: u16,
    values: &'static [u8],
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
