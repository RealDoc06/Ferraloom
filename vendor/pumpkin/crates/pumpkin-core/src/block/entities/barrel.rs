use pumpkin_data::block_properties::BarrelLikeProperties;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::{Block, FacingExt, item_stack::ItemStack};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::random::xoroshiro128::Xoroshiro;
use pumpkin_util::random::{RandomImpl, get_seed};
use std::sync::RwLock;
use std::sync::{Arc, atomic::AtomicBool};

use crate::block::viewer::{ContainerViewerCount, ViewerCountListener};
use crate::world::{BlockFlags, World};

pub struct BarrelBlockEntity {
    pub position: BlockPos,
    state: RwLock<super::chest_like_block_entity::ChestState<{ Self::INVENTORY_SIZE }>>,
    pub dirty: AtomicBool,
    pub comparator_dirty: AtomicBool,

    // Viewer
    viewers: ContainerViewerCount,
}

crate::impl_block_entity_for_chest!(BarrelBlockEntity);
crate::impl_inventory_for_chest!(BarrelBlockEntity);
crate::impl_clearable_for_chest!(BarrelBlockEntity);
crate::impl_container_helper_methods!(BarrelBlockEntity);

impl ViewerCountListener for BarrelBlockEntity {
    fn on_container_open(&self, world: &Arc<World>, _position: &BlockPos) {
        self.play_sound(world, Sound::BlockBarrelOpen);
        self.set_open(world, true);
    }

    fn on_container_close(&self, world: &Arc<World>, _position: &BlockPos) {
        self.play_sound(world, Sound::BlockBarrelClose);
        self.set_open(world, false);
    }
}

impl BarrelBlockEntity {
    pub const INVENTORY_SIZE: usize = 27;
    pub const ID: &'static str = "minecraft:barrel";

    fn set_open(&self, world: &Arc<World>, open: bool) {
        let state = world.get_block_state(&self.position);
        let mut properties = BarrelLikeProperties::from_state_id(state.id);

        properties.open = open;

        world.set_block_state(
            &self.position,
            properties.to_state_id(&Block::BARREL),
            BlockFlags::NOTIFY_ALL,
        );
    }

    fn play_sound(&self, world: &Arc<World>, sound: Sound) {
        let mut rng = Xoroshiro::from_seed(get_seed());

        let state = world.get_block_state(&self.position);
        let properties = BarrelLikeProperties::from_state_id(state.id);
        let direction = properties.facing.to_block_direction().to_offset();
        let position = Vector3::new(
            self.position.0.x as f64 + 0.5 + direction.x as f64 / 2.0,
            self.position.0.y as f64 + 0.5 + direction.y as f64 / 2.0,
            self.position.0.z as f64 + 0.5 + direction.z as f64 / 2.0,
        );
        world.play_sound_fine(
            sound,
            SoundCategory::Blocks,
            &position,
            0.5,
            rng.next_f32() * 0.1 + 0.9,
        );
    }
}
