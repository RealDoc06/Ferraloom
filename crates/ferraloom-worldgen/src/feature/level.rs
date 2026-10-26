//! Feature placement over either exclusively owned generation chunks or a live level.
use super::FeatureRandomSelector;
use crate::region::{Heightmap, LightLayer, SavedTick};
use crate::{Error, region::WorldGenRegion};
use ferraloom_chunk::BiomeId;
use ferraloom_data::{Block, BlockState, FluidState};
use ferraloom_nbt::CompoundTag;
use ferraloom_util::{Mirror, Rotation};

/// Port of the WorldGenLevel operations reached by the configured features.
/// Region calls are statically dispatched; Live alone uses a trait object.
/// In particular, a live set_block must run Level#setBlock with the supplied
/// flags, including neighbors, block entities, light and tracking. It must never
/// substitute WorldGenRegion writes. Missing game operations fail explicitly.
pub enum WorldGenLevel<'a, 'r> {
    Region(&'a mut WorldGenRegion<'r>),
    Live(&'a mut dyn LiveWorldGenLevel),
}

// Keep the public live contract and both forwarding implementations together.
macro_rules! level_method {
    ($(#[$attr:meta])* read $vis:vis fn $name:ident($this:ident: $receiver:ty $(, $arg:ident: $ty:ty)*) -> $ret:ty [required]) => {
        $(#[$attr])* $vis fn $name(&$this, $($arg: $ty),*) -> $ret;
    };
    ($(#[$attr:meta])* write $vis:vis fn $name:ident($this:ident: $receiver:ty $(, $arg:ident: $ty:ty)*) -> $ret:ty [required]) => {
        $(#[$attr])* $vis fn $name(&mut $this, $($arg: $ty),*) -> $ret;
    };
    ($(#[$attr:meta])* read $vis:vis fn $name:ident($this:ident: $receiver:ty $(, $arg:ident: $ty:ty)*) -> $ret:ty [$body:block]) => {
        $(#[$attr])* $vis fn $name(&$this, $($arg: $ty),*) -> $ret $body
    };
    ($(#[$attr:meta])* write $vis:vis fn $name:ident($this:ident: $receiver:ty $(, $arg:ident: $ty:ty)*) -> $ret:ty [$body:block]) => {
        $(#[$attr])* $vis fn $name(&mut $this, $($arg: $ty),*) -> $ret $body
    };
}
macro_rules! level_region_call {
    ($name:ident, read, $region:ident $(, $arg:ident)*) => { LiveWorldGenLevel::$name(&**$region, $($arg),*) };
    ($name:ident, write, $region:ident $(, $arg:ident)*) => { LiveWorldGenLevel::$name(&mut **$region, $($arg),*) };
}
macro_rules! level_operations {
    ($( $(#[$attr:meta])* $mode:ident fn $name:ident($this:ident: $receiver:ty $(, $arg:ident: $ty:ty)* $(,)?) -> $ret:ty [$default:tt] => $region:block; )*) => {
        #[allow(clippy::too_many_arguments)]
        /// WorldGenLevel operations used by features. Implement over the live
        /// Level; defaults for unavailable game behavior return Unsupported.
        pub trait LiveWorldGenLevel {

            $( level_method! { $(#[$attr])* $mode fn $name($this: $receiver $(, $arg: $ty)*) -> $ret [$default] } )*
        }
        #[allow(clippy::too_many_arguments)]
        impl LiveWorldGenLevel for WorldGenRegion<'_> {
            $( level_method! { $mode fn $name($this: $receiver $(, $arg: $ty)*) -> $ret [$region] } )*
        }
        #[allow(clippy::too_many_arguments)]
        impl WorldGenLevel<'_, '_> {
            $( level_method! { $mode pub fn $name($this: $receiver $(, $arg: $ty)*) -> $ret [{
                match $this {
                    Self::Region(region) => level_region_call!($name, $mode, region $(, $arg)*),
                    Self::Live(level) => level.$name($($arg),*),
                }
            }] } )*
        }
        #[allow(clippy::too_many_arguments)]
        impl LiveWorldGenLevel for WorldGenLevel<'_, '_> {
            $( level_method! { $mode fn $name($this: $receiver $(, $arg: $ty)*) -> $ret [{ WorldGenLevel::$name($this, $($arg),*) }] } )*
        }
    };
}
level_operations! {
    read fn min_y(self: &Self) -> i32 [required] => { WorldGenRegion::min_y(self) };
    read fn height(self: &Self) -> i32 [required] => { WorldGenRegion::height(self) };
    read fn block(self: &Self, p: [i32; 3]) -> Result<BlockState, Error> [{ let _ = p; Err(Error("Unsupported: live feature block".into())) }] => { WorldGenRegion::block(self, p) };
    read fn fluid(self: &Self, p: [i32; 3]) -> Result<FluidState, Error> [{ Ok(self.block(p)?.data().fluid) }] => { WorldGenRegion::fluid(self, p) };
    read fn noise_biome(self: &Self, quart: [i32; 3]) -> Result<BiomeId, Error> [{ let _ = quart; Err(Error("Unsupported: live feature noise_biome".into())) }] => { WorldGenRegion::noise_biome(self, quart) };
    read fn height_at(self: &Self, map: Heightmap, x: i32, z: i32) -> Result<i32, Error> [{ let _ = (map, x, z); Err(Error("Unsupported: live feature height_at".into())) }] => { WorldGenRegion::height_at(self, map, x, z) };
    read fn ensure_can_write(self: &Self, p: [i32; 3]) -> bool [{ let _ = p; true }] => { WorldGenRegion::ensure_can_write(self, p) };
    write fn set_block(self: &mut Self, p: [i32; 3], state: BlockState, flags: u32) -> Result<bool, Error> [{ let _ = (p, state, flags); Err(Error("Unsupported: live feature set_block".into())) }] => { WorldGenRegion::set_block(self, p, state, flags) };
    write fn set_block_in_section(self: &mut Self, p: [i32; 3], state: BlockState) -> Result<(), Error> [{ let _ = (p, state); Err(Error("Unsupported: live feature set_block_in_section".into())) }] => { WorldGenRegion::set_block_in_section(self, p, state) };
    write fn mark_post_processing(self: &mut Self, p: [i32; 3]) -> Result<(), Error> [{ let _ = p; Err(Error("Unsupported: live feature mark_post_processing".into())) }] => { WorldGenRegion::mark_post_processing(self, p) };
    write fn add_entity(self: &mut Self, chunk: [i32; 2], tag: CompoundTag) -> Result<(), Error> [{ let _ = (chunk, tag); Err(Error("Unsupported: live feature add_entity".into())) }] => { WorldGenRegion::add_entity(self, chunk, tag) };
    write fn schedule_block_tick(self: &mut Self, tick: SavedTick<Block>, delay: i32) -> Result<(), Error> [{ let _ = (tick, delay); Err(Error("Unsupported: live feature schedule_block_tick".into())) }] => { let _ = delay; WorldGenRegion::schedule_block_tick(self, tick) };
    write fn schedule_fluid_tick(self: &mut Self, tick: SavedTick<ferraloom_data::registries::fluid::Id>, delay: i32) -> Result<(), Error> [{ let _ = (tick, delay); Err(Error("Unsupported: live feature schedule_fluid_tick".into())) }] => { let _ = delay; WorldGenRegion::schedule_fluid_tick(self, tick) };
    write fn destroy_block(self: &mut Self, p: [i32; 3], _drop: bool) -> Result<bool, Error> [{ let _ = (p, _drop); Err(Error("Unsupported: live feature destroy_block".into())) }] => { WorldGenRegion::destroy_block(self, p, _drop) };
    write fn place_liquid(self: &mut Self, p: [i32; 3], fluid: FluidState) -> Result<bool, Error> [{ let _ = (p, fluid); Err(Error("Unsupported: live feature place_liquid".into())) }] => { WorldGenRegion::place_liquid(self, p, fluid) };
    write fn update_shape(self: &mut Self, p: [i32; 3], direction: ferraloom_util::Direction, neighbor_p: [i32; 3], neighbor_state: BlockState) -> Result<BlockState, Error> [{ let _ = (p, direction, neighbor_p, neighbor_state); Err(Error("Unsupported: live feature update_shape".into())) }] => { WorldGenRegion::update_shape(self, p, direction, neighbor_p, neighbor_state) };
    write fn update_from_neighbor_shapes(self: &mut Self, p: [i32; 3], state: BlockState) -> Result<BlockState, Error> [{ let _ = (p, state); Err(Error("Unsupported: live feature update_from_neighbor_shapes".into())) }] => { WorldGenRegion::update_from_neighbor_shapes(self, p, state) };
    write fn update_neighbors_at(self: &mut Self, _p: [i32; 3], _block: Block) -> Result<(), Error> [{ let _ = (_p, _block); Err(Error("Unsupported: live feature update_neighbors_at".into())) }] => { WorldGenRegion::update_neighbors_at(self, _p, _block) };
    write fn load_block_entity(self: &mut Self, p: [i32; 3], tag: CompoundTag, random: &mut FeatureRandomSelector<'_>) -> Result<(), Error> [{ let _ = (p, tag, random); Err(Error("Unsupported: live feature load_block_entity".into())) }] => { WorldGenRegion::load_block_entity(self, p, tag, random.region_mut()?) };
    write fn finalize_template_entity(self: &mut Self, tag: &mut CompoundTag, mirror: Mirror, rotation: Rotation, finalize: bool) -> Result<bool, Error> [{ let _ = (tag, mirror, rotation, finalize); Err(Error("Unsupported: live feature finalize_template_entity".into())) }] => { WorldGenRegion::finalize_template_entity(self, tag, mirror, rotation, finalize) };
    /// Live placement constructs and transforms one retained receiver. Generation
    /// keeps its existing deferred tag finalization and insertion path.
    write fn place_template_entity(self: &mut Self, chunk: [i32; 2], tag: CompoundTag, mirror: Mirror, rotation: Rotation, finalize: bool) -> Result<(), Error> [{ place_template_entity_deferred(self, chunk, tag, mirror, rotation, finalize) }] => { place_template_entity_deferred(self, chunk, tag, mirror, rotation, finalize) };
    /// StructureTemplate's final placed-block pass calls setChanged only after
    /// all waterlogging and neighbor-shape updates have completed.
    write fn mark_template_block_entity_changed(self: &mut Self, p: [i32; 3]) -> Result<(), Error> [{ let _ = p; Err(Error("Unsupported: live template block entity setChanged".into())) }] => { let _ = p; Ok(()) };
    read fn is_outline_full_block(self: &Self, state: BlockState, p: [i32; 3]) -> Result<bool, Error> [{ let _ = (state, p); Err(Error("Unsupported: live feature is_outline_full_block".into())) }] => { WorldGenRegion::is_outline_full_block(self, state, p) };
    read fn is_collision_full_block(self: &Self, state: BlockState, p: [i32; 3]) -> Result<bool, Error> [{ let _ = (state, p); Err(Error("Unsupported: live feature is_collision_full_block".into())) }] => { WorldGenRegion::is_collision_full_block(self, state, p) };
    read fn is_solid(self: &Self, p: [i32; 3]) -> Result<bool, Error> [{ Ok(self.block(p)?.is_solid()) }] => { WorldGenRegion::is_solid(self, p) };
    read fn has_sturdy_face(self: &Self, p: [i32; 3], face: ferraloom_util::Direction) -> Result<bool, Error> [{ let _ = (p, face); Err(Error("Unsupported: live feature has_sturdy_face".into())) }] => { WorldGenRegion::has_sturdy_face(self, p, face) };
    read fn state_has_sturdy_face(self: &Self, state: BlockState, query_position: [i32; 3], face: ferraloom_util::Direction) -> Result<bool, Error> [{ let _ = (state, query_position, face); Err(Error("Unsupported: live feature state_has_sturdy_face".into())) }] => { WorldGenRegion::state_has_sturdy_face(self, state, query_position, face) };
    read fn can_survive(self: &Self, state: BlockState, p: [i32; 3]) -> Result<bool, Error> [{ let _ = (state, p); Err(Error("Unsupported: live feature can_survive".into())) }] => { WorldGenRegion::can_survive(self, state, p) };
    read fn is_unobstructed(self: &Self, _state: BlockState, _p: [i32; 3]) -> Result<bool, Error> [{ let _ = (_state, _p); Err(Error("Unsupported: live feature is_unobstructed".into())) }] => { WorldGenRegion::is_unobstructed(self, _state, _p) };
    write fn generate_chorus_plant(self: &mut Self, p: [i32; 3], random: &mut FeatureRandomSelector<'_>, radius: i32) -> Result<(), Error> [{ let _ = (p, random, radius); Err(Error("Unsupported: live feature generate_chorus_plant".into())) }] => { WorldGenRegion::generate_chorus_plant(self, p, random.region_mut()?, radius) };
    write fn configure_end_gateway(self: &mut Self, p: [i32; 3], exit: [i32; 3], exact: bool) -> Result<(), Error> [{ let _ = (p, exit, exact); Err(Error("Unsupported: live feature configure_end_gateway".into())) }] => { WorldGenRegion::configure_end_gateway(self, p, exit, exact) };
    read fn is_collision_empty(self: &Self, p: [i32; 3]) -> Result<bool, Error> [{ let _ = p; Err(Error("Unsupported: live feature is_collision_empty".into())) }] => { WorldGenRegion::is_collision_empty(self, p) };
    write fn create_end_spike_crystal(self: &mut Self, p: [i32; 3], beam: Option<[i32; 3]>, invulnerable: bool, random: &mut FeatureRandomSelector<'_>) -> Result<(), Error> [{ let _ = (p, beam, invulnerable, random); Err(Error("Unsupported: live feature create_end_spike_crystal".into())) }] => { WorldGenRegion::create_end_spike_crystal(self, p, beam, invulnerable, random.region_mut()?) };
    read fn brightness(self: &Self, layer: LightLayer, p: [i32; 3]) -> Result<u8, Error> [{ let _ = (layer, p); Err(Error("Unsupported: live feature brightness".into())) }] => { WorldGenRegion::brightness(self, layer, p) };
    read fn vine_acceptable_neighbor(self: &Self, p: [i32; 3], direction: ferraloom_util::Direction) -> Result<bool, Error> [{ let _ = (p, direction); Err(Error("Unsupported: live feature vine_acceptable_neighbor".into())) }] => { WorldGenRegion::vine_acceptable_neighbor(self, p, direction) };
    write fn place_double_plant(self: &mut Self, state: BlockState, p: [i32; 3], flags: u32) -> Result<(), Error> [{ let _ = (state, p, flags); Err(Error("Unsupported: live feature place_double_plant".into())) }] => { WorldGenRegion::place_double_plant(self, state, p, flags) };
    write fn place_mossy_carpet(self: &mut Self, p: [i32; 3], flags: u32) -> Result<(), Error> [{ let _ = (p, flags); Err(Error("Unsupported: live feature place_mossy_carpet".into())) }] => { WorldGenRegion::place_mossy_carpet(self, p, flags) };
    write fn populate_worldgen_beehive(self: &mut Self, p: [i32; 3], random: &mut FeatureRandomSelector<'_>) -> Result<(), Error> [{ let _ = (p, random); Err(Error("Unsupported: live feature populate_worldgen_beehive".into())) }] => { WorldGenRegion::populate_worldgen_beehive(self, p, random.region_mut()?) };
    read fn multiface_state_for_placement(self: &Self, block: Block, old: BlockState, p: [i32; 3], face: ferraloom_util::Direction) -> Result<Option<BlockState>, Error> [{ let _ = (block, old, p, face); Err(Error("Unsupported: live feature multiface_state_for_placement".into())) }] => { WorldGenRegion::multiface_state_for_placement(self, block, old, p, face) };
    write fn multiface_spread(self: &mut Self, state: BlockState, p: [i32; 3], face: ferraloom_util::Direction, random: &mut FeatureRandomSelector<'_>, mark_post_processing: bool) -> Result<bool, Error> [{ let _ = (state, p, face, random, mark_post_processing); Err(Error("Unsupported: live feature multiface_spread".into())) }] => { WorldGenRegion::multiface_spread(self, state, p, face, random.region_mut()?, mark_post_processing) };
    read fn sculk_can_spread_from(self: &Self, p: [i32; 3]) -> Result<bool, Error> [{ let _ = p; Err(Error("Unsupported: live feature sculk_can_spread_from".into())) }] => { WorldGenRegion::sculk_can_spread_from(self, p) };
    write fn sculk_spread(self: &mut Self, origin: [i32; 3], random: &mut FeatureRandomSelector<'_>, charge_count: i32, amount_per_charge: i32, spread_attempts: i32, growth_rounds: i32, spread_rounds: i32) -> Result<(), Error> [{ let _ = (origin, random, charge_count, amount_per_charge, spread_attempts, growth_rounds, spread_rounds); Err(Error("Unsupported: live feature sculk_spread".into())) }] => { WorldGenRegion::sculk_spread(self, origin, random.region_mut()?, charge_count, amount_per_charge, spread_attempts, growth_rounds, spread_rounds) };
    write fn set_loot_table(self: &mut Self, p: [i32; 3], table: &crate::Identifier, random: &mut FeatureRandomSelector<'_>) -> Result<(), Error> [{ let _ = (p, table, random); Err(Error("Unsupported: live feature set_loot_table".into())) }] => { WorldGenRegion::set_loot_table(self, p, table, random.region_mut()?) };
    write fn set_spawner_entity(self: &mut Self, p: [i32; 3], kind: ferraloom_data::EntityType, random: &mut FeatureRandomSelector<'_>) -> Result<(), Error> [{ let _ = (p, kind, random); Err(Error("Unsupported: live feature set_spawner_entity".into())) }] => { WorldGenRegion::set_spawner_entity(self, p, kind, random.region_mut()?) };
    read fn block_tags(self: &Self) -> Result<&ferraloom_data::datapack::TagSet<Block>, Error> [{ Err(Error("Unsupported: live feature block_tags".into())) }] => { WorldGenRegion::block_tags(self) };
    read fn fluid_tags(self: &Self) -> Result<&ferraloom_data::datapack::TagSet<ferraloom_data::registries::fluid::Id>, Error> [{ Err(Error("Unsupported: live feature fluid_tags".into())) }] => { WorldGenRegion::fluid_tags(self) };
    write fn random_mut(self: &mut Self) -> Result<&mut ferraloom_util::random::Random, Error> [{ Err(Error("Unsupported: live feature random_mut".into())) }] => { WorldGenRegion::random_mut(self) };
    read fn seed(self: &Self) -> i64 [required] => { self.seed };
    read fn sea_level(self: &Self) -> i32 [required] => { self.sea_level };
    read fn has_block_entity_at(self: &Self, p: [i32; 3]) -> Result<bool, Error> [{
        let _ = p; Err(Error("Unsupported: live feature block entities".into()))
    }] => { Ok(WorldGenRegion::block_entity(self, p)?.is_some()) };
}

fn place_template_entity_deferred(
    level: &mut (impl LiveWorldGenLevel + ?Sized),
    chunk: [i32; 2],
    mut tag: CompoundTag,
    mirror: Mirror,
    rotation: Rotation,
    finalize: bool,
) -> Result<(), Error> {
    if level.finalize_template_entity(&mut tag, mirror, rotation, finalize)? {
        level.add_entity(chunk, tag)?;
    }
    Ok(())
}
