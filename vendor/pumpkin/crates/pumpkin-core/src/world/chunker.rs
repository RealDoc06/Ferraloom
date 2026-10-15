use pumpkin_util::math::vector2::Vector2;
use std::{num::NonZero, sync::Arc};

use pumpkin_protocol::{
    bedrock::client::network_chunk_publisher_update::CNetworkChunkPublisherUpdate,
    java::client::play::CCenterChunk,
};
use pumpkin_world::cylindrical_chunk_iterator::Cylindrical;

use crate::{
    entity::{
        EntityBase,
        player::{Player, chunk_watches::HeldChunkTickets},
    },
    net::ClientPlatform,
};

pub fn get_view_distance(player: &Player) -> NonZero<u8> {
    let fallback = NonZero::new(2).unwrap_or(NonZero::<u8>::MIN);
    let Some(server) = player.world().server.upgrade() else {
        return fallback;
    };
    let max_view_distance = match player.client.as_ref() {
        ClientPlatform::Java(_) => server.advanced_config.networking.java.view_distance,
        ClientPlatform::Bedrock(_) => server.advanced_config.networking.bedrock.view_distance,
    };
    player
        .config
        .load()
        .view_distance
        .clamp(fallback, max_view_distance)
}

// Checks if the target chunk is within Chebyshev distance (L_infinity) of the center chunk.
#[must_use]
#[inline]
pub fn is_within_chebyshev_distance(
    center: Vector2<i32>,
    target: Vector2<i32>,
    distance: i32,
) -> bool {
    (target.x - center.x).abs().max((target.y - center.y).abs()) <= distance
}

#[allow(clippy::too_many_lines)]
pub fn update_position(player: &Arc<Player>) {
    // Vanilla `ChunkMap.move` -> re-pair on every move, not only on a view change.
    let world = player.world();
    let mut requested = player.chunk_watches.request();
    if player.client.closed() || player.world().uuid != world.uuid || !requested.accepts(world.uuid)
    {
        return;
    }
    let new_chunk_center = player.get_entity().chunk_pos.load();
    let old_cylindrical = requested.view(world.uuid);
    world.entity_tracker.update_player_position(player, &world);

    let view_distance = get_view_distance(player);
    let new_cylindrical = Cylindrical::new(new_chunk_center, view_distance);

    if old_cylindrical == Some(new_cylindrical) {
        return;
    }

    match player.client.as_ref() {
        ClientPlatform::Java(java_client) => {
            java_client.try_send_packet(&CCenterChunk {
                chunk_x: new_chunk_center.x.into(),
                chunk_z: new_chunk_center.y.into(),
            });
        }
        ClientPlatform::Bedrock(bedrock_client) => {
            if let Ok(data) = bedrock_client.serialize_packet(&CNetworkChunkPublisherUpdate::new(
                player.get_entity().block_pos.load(),
                u32::from(view_distance.get()) * 16,
            )) {
                bedrock_client.try_enqueue_packet(data);
            }
        }
    }
    let (loading_chunks, unloading_chunks): (Vec<_>, Vec<_>) = if let Some(old) = old_cylindrical {
        let (loading, unloading) = Cylindrical::changed_chunks(old, new_cylindrical);
        (loading.collect(), unloading.collect())
    } else {
        (new_cylindrical.all_chunks_within().collect(), Vec::new())
    };

    let level = &world.level;
    let mut held_tickets = player
        .held_chunk_tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let is_spectator = player.is_spectator();
    let spectators_generate_chunks = world
        .level_info
        .load()
        .game_rules
        .spectators_generate_chunks;

    let new_view_level = (!is_spectator || spectators_generate_chunks).then(|| {
        pumpkin_world::chunk_system::ChunkLoading::get_level_from_view_distance(
            u8::from(view_distance) + 1,
        )
    });

    let new_sim_level = (!is_spectator || spectators_generate_chunks).then(|| {
        let sim_dist = world.server.upgrade().map_or(10, |s| {
            s.advanced_config.networking.java.simulation_distance.get()
        });
        pumpkin_world::chunk_system::ChunkLoading::get_level_from_simulation_distance(sim_dist)
    });

    {
        let mut lock = level
            .chunk_loading
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(view) = new_view_level {
            lock.add_ticket(new_chunk_center, view);
        }
        if let Some(sim) = new_sim_level {
            lock.add_ticket(new_chunk_center, sim);
        }

        if let Some(old) = held_tickets.replace(HeldChunkTickets::new(
            level,
            new_chunk_center,
            new_view_level,
            new_sim_level,
        )) {
            requested.retire(old);
        }
        lock.send_change();
    };
    drop(held_tickets);

    {
        let mut sender = player
            .chunk_sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        sender.unload_chunks(&player.client, &unloading_chunks);
        for pos in &loading_chunks {
            sender.enqueue_chunk(*pos);
        }
    }
    player.watched_section.store(new_cylindrical);
    let revision = requested.publish(&world, new_cylindrical);
    drop(requested);

    // Async work reconciles actual owned references, never speculative view deltas.
    if !loading_chunks.is_empty() || !unloading_chunks.is_empty() {
        let player_clone = player.clone();
        if let Some(server) = world.server.upgrade() {
            server.spawn_task(async move {
                if let Some((world, added)) = player_clone.chunk_watches.apply(revision).await {
                    if !added.is_empty()
                        && player_clone.chunk_watches.requests_world(world.uuid)
                        && !player_clone.client.closed()
                    {
                        world.spawn_world_entity_chunks(player_clone, added);
                    }
                }
            });
        }
    }
}
