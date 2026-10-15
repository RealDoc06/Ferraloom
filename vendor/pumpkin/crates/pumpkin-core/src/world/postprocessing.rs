use super::World;
use crate::block::OnScheduledTickArgs;
use pumpkin_data::{Block, fluid::Fluid};
use pumpkin_util::math::{position::BlockPos, vector2::Vector2};
use pumpkin_world::{level::SyncChunk, world::BlockFlags};
use std::sync::Arc;

impl World {
    pub(super) fn process_chunk_postprocessing(self: &Arc<Self>) {
        self.level.process_chunk_activations(|chunk| {
            chunk.process_post_processing(|pos| self.postprocess_generation_position(&pos))
        });
    }

    /// Direct spawn/teleport sends use the same ticketed activation as batch sends.
    /// Publication and completion wake the waiter, including while normal ticking
    /// is frozen. Neither a blocking fetch nor a polling loop drives generation.
    pub(super) async fn get_ready_chunk(self: &Arc<Self>, pos: Vector2<i32>) -> Option<SyncChunk> {
        loop {
            let changed = self.level.chunk_activation_changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            self.level.request_chunk_activation(pos);
            self.process_chunk_postprocessing();
            if let Some(chunk) = self.level.ready_chunk(pos) {
                return Some(chunk);
            }
            tokio::select! {
                () = self.level.cancel_token.cancelled() => return None,
                () = &mut changed => {},
            }
        }
    }

    fn postprocess_generation_position(self: &Arc<Self>, pos: &BlockPos) -> bool {
        let Some(state) = self.get_block_state_id_if_loaded(pos) else {
            return false;
        };
        let block = state.to_block();
        let fluid = Self::get_fluid_from_state_id(state);
        // Vanilla invokes FluidState.tick synchronously before the liquid-block
        // tick or shape fold. Reuse the scheduled-tick handler; subsequent ticks
        // it requests retain the existing scheduler's ordering and delays.
        if fluid != &Fluid::EMPTY {
            let Some(behavior) = self.block_registry.get_pumpkin_fluid(fluid.id) else {
                return false;
            };
            behavior.on_scheduled_tick(self, fluid, pos);
        }
        if block == &Block::WATER || block == &Block::LAVA {
            if let Some(behavior) = self.block_registry.get_pumpkin_block(block.id) {
                behavior.on_scheduled_tick(OnScheduledTickArgs {
                    world: self,
                    block,
                    position: pos,
                });
            }
            return true;
        }
        let updated = self.update_from_neighbor_shapes(state, pos);
        if updated == state {
            return true;
        }
        // Java 276 is INVISIBLE | KNOWN_SHAPE | SKIP_BLOCK_ENTITY_SIDEEFFECTS.
        // Pumpkin's bit assignments differ. Its ordinary setter also updates light.
        self.set_block_state_if(
            pos,
            updated,
            BlockFlags::UPDATE_KNOWN_SHAPE | BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK,
            |_| true,
        )
        .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use pumpkin_config::world::LevelConfig;
    use pumpkin_data::{block_properties::GlowLichenLikeProperties, dimension::Dimension};
    use pumpkin_util::world_seed::Seed;
    use pumpkin_world::{
        chunk::{
            ChunkData, ChunkLight,
            format::{LightContainer, anvil::SingleChunkDataSerializer},
        },
        chunk_system::ChunkLoading,
        level::Level,
        world_info::LevelData,
    };
    use std::sync::{Weak, atomic::Ordering};

    async fn test_world() -> (tempfile::TempDir, Arc<World>) {
        let folder = tempfile::tempdir().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            folder.path().into(),
            0,
            Dimension::OVERWORLD,
        );
        // Use controlled FULL chunks, with no asynchronous terrain generation.
        level.shutdown().await.unwrap();
        let world = Arc::new(World::load(
            level,
            Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0)))),
            Dimension::OVERWORLD,
            crate::block::registry::default_registry(),
            Weak::new(),
        ));
        for x in -2..=0 {
            for z in -1..=1 {
                let chunk = ChunkData::empty_sync(x, z);
                *chunk.light_engine.lock().unwrap() = ChunkLight {
                    sky_light: vec![LightContainer::new_empty(0); chunk.section.count]
                        .into_boxed_slice(),
                    block_light: vec![LightContainer::new_empty(0); chunk.section.count]
                        .into_boxed_slice(),
                };
                world.level.loaded_chunks.insert(Vector2::new(x, z), chunk);
            }
        }
        (folder, world)
    }

    #[tokio::test]
    #[expect(clippy::too_many_lines)]
    async fn deferred_cross_boundary_shapes_preserve_valid_faces_and_remove_emitted_light() {
        let (_folder, world) = test_world().await;
        let center = Vector2::new(-1, 0);
        let west = Vector2::new(-2, 0);
        let invalid = BlockPos::new(-16, -30, 14);
        let valid = BlockPos::new(-16, -30, 1);
        let flags =
            BlockFlags::UPDATE_KNOWN_SHAPE | BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK;
        for pos in [invalid, valid] {
            world.set_block_state(
                &pos.offset(pumpkin_data::BlockDirection::West.to_offset()),
                Block::STONE.default_state.id,
                flags,
            );
        }
        world.set_block_state(
            &valid.offset(pumpkin_data::BlockDirection::North.to_offset()),
            Block::STONE.default_state.id,
            flags,
        );
        let mut faces = GlowLichenLikeProperties::default(&Block::GLOW_LICHEN);
        faces.west = true;
        let west_only = faces.to_state_id(&Block::GLOW_LICHEN);
        world.set_block_state(&invalid, west_only, flags);
        faces.north = true;
        world.set_block_state(&valid, faces.to_state_id(&Block::GLOW_LICHEN), flags);
        assert!(
            world
                .level
                .light_engine
                .get_block_light_level_sync(&world.level, &invalid)
                .unwrap()
                > 0
        );
        let center_chunk = world
            .level
            .loaded_chunks
            .get(&center)
            .unwrap()
            .value()
            .clone();
        for pos in [invalid, valid] {
            center_chunk.mark_pos_for_postprocessing(&pos);
        }
        // A persisted FULL chunk follows the same activation path after reload.
        let saved = center_chunk.to_bytes().unwrap();
        let center_chunk = Arc::new(ChunkData::from_bytes(&saved, center).unwrap());
        world
            .level
            .loaded_chunks
            .insert(center, center_chunk.clone());

        let (_, west_chunk) = world.level.loaded_chunks.remove(&west).unwrap();
        world.forced_chunks.lock().unwrap().insert(center);
        world.update_active_chunks();
        assert!(!world.level.is_chunk_ready(&center));
        assert!(world.active_chunks.read().unwrap().is_empty());
        assert_eq!(world.get_block_state_id(&invalid), west_only);
        assert_eq!(
            center_chunk
                .post_processing
                .lock()
                .unwrap()
                .iter()
                .map(Vec::len)
                .sum::<usize>(),
            2
        );
        world.schedule_block_tick(
            &Block::GLOW_LICHEN,
            invalid,
            0,
            pumpkin_world::tick::TickPriority::Normal,
        );
        let requested = rustc_hash::FxHashSet::from_iter([center]);
        assert!(
            world
                .level
                .get_tick_data(&requested, 0)
                .block_ticks
                .is_empty()
        );
        assert!(world.is_block_tick_scheduled(&invalid, &Block::GLOW_LICHEN));
        {
            let loading = world.level.chunk_loading.lock().unwrap();
            assert_eq!(
                loading.ticket.get(&center),
                Some(&vec![ChunkLoading::FULL_CHUNK_LEVEL - 1])
            );
            assert_eq!(
                loading.pos_level.get(&west),
                Some(&ChunkLoading::FULL_CHUNK_LEVEL)
            );
            assert_eq!(
                loading.ticket.len(),
                1,
                "halo-only neighbors must not activate"
            );
        }

        // A later neighboring generation stage removes both west supports, without
        // triggering live neighbor updates. The north face of `valid` still has support.
        for pos in [invalid, valid] {
            west_chunk.set_block_absolute_y(
                15,
                pos.0.y,
                pos.0.z as usize,
                Block::AIR.default_state.id,
            );
        }
        world.level.loaded_chunks.insert(west, west_chunk);
        center_chunk.dirty.store(false, Ordering::Relaxed);
        world.update_active_chunks();
        assert!(world.level.is_chunk_ready(&center));
        assert!(world.active_chunks.read().unwrap().contains(&center));
        assert_eq!(
            world.get_block_state_id(&invalid),
            Block::AIR.default_state.id
        );
        let remaining = GlowLichenLikeProperties::from_state_id(world.get_block_state_id(&valid));
        assert!(remaining.north);
        assert!(!remaining.west);
        assert_eq!(
            world
                .level
                .light_engine
                .get_block_light_level_sync(&world.level, &invalid),
            Some(0)
        );
        assert!(
            center_chunk
                .post_processing
                .lock()
                .unwrap()
                .iter()
                .all(Vec::is_empty)
        );
        assert!(center_chunk.dirty.load(Ordering::Relaxed));
        assert!(world.level.chunk_loading.lock().unwrap().ticket.is_empty());
        assert!(
            !world.level.is_chunk_ready(&west),
            "FULL halo was not activated"
        );

        // A later ordinary palette change is not replayed as a generation mark.
        center_chunk.set_block_absolute_y(0, invalid.0.y, invalid.0.z as usize, west_only);
        world.update_active_chunks();
        assert_eq!(world.get_block_state_id(&invalid), west_only);
    }

    #[tokio::test]
    async fn marked_fluid_runs_before_liquid_block_tick_and_future_ticks_stay_scheduled() {
        use crate::block::{BlockBehaviour, BlockMetadata, FluidMetadata, fluid::FluidBehaviour};
        use pumpkin_data::{BlockId, BlockStateId};
        use pumpkin_world::tick::TickPriority;
        use std::sync::Mutex;

        struct ObservedFluid(Arc<Mutex<Vec<(&'static str, BlockStateId)>>>);
        impl FluidMetadata for ObservedFluid {
            fn ids() -> Box<[u16]> {
                [Fluid::FLOWING_WATER.id].into()
            }
        }
        impl FluidBehaviour for ObservedFluid {
            fn on_scheduled_tick(&self, world: &Arc<World>, fluid: &Fluid, pos: &BlockPos) {
                self.0
                    .lock()
                    .unwrap()
                    .push(("fluid", world.get_block_state_id(pos)));
                world.set_block_state(
                    pos,
                    Block::STONE.default_state.id,
                    BlockFlags::UPDATE_KNOWN_SHAPE,
                );
                world.schedule_fluid_tick(fluid, *pos, 5, TickPriority::Normal);
            }
        }
        struct ObservedLiquid(Arc<Mutex<Vec<(&'static str, BlockStateId)>>>);
        impl BlockMetadata for ObservedLiquid {
            fn ids() -> Box<[BlockId]> {
                [BlockId::WATER].into()
            }
        }
        impl BlockBehaviour for ObservedLiquid {
            fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
                self.0
                    .lock()
                    .unwrap()
                    .push(("block", args.world.get_block_state_id(args.position)));
            }
        }

        let (_folder, mut world) = test_world().await;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let registry = Arc::get_mut(&mut Arc::get_mut(&mut world).unwrap().block_registry).unwrap();
        registry.register_fluid(ObservedFluid(calls.clone()));
        registry.register(ObservedLiquid(calls.clone()));
        let pos = BlockPos::new(-8, -32, 8);
        let center = Vector2::new(-1, 0);
        let chunk = world
            .level
            .loaded_chunks
            .get(&center)
            .unwrap()
            .value()
            .clone();
        chunk.set_block_absolute_y(8, pos.0.y, 8, Block::WATER.default_state.id);
        chunk.mark_pos_for_postprocessing(&pos);
        world.level.request_chunk_activation(center);
        world.process_chunk_postprocessing();
        assert_eq!(
            *calls.lock().unwrap(),
            vec![
                ("fluid", Block::WATER.default_state.id),
                ("block", Block::STONE.default_state.id),
            ]
        );
        assert!(world.is_fluid_tick_scheduled(&pos, &Fluid::FLOWING_WATER));
        world.process_chunk_postprocessing();
        assert_eq!(calls.lock().unwrap().len(), 2);
    }
}
