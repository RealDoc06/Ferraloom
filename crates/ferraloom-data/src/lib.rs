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

#[derive(Clone, Copy, Debug)]
struct PropertyLayout {
    id: u8,
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
