use super::*;

struct ListenerInfo {
    listener: GameEventListenerRef,
    distance_squared: f64,
}

/// Port of GameEventDispatcher. Every listener call completes inside post.
pub struct GameEventDispatcher;
impl GameEventDispatcher {
    pub fn post(
        level: &mut Level,
        event: GameEvent,
        position: Vec3,
        context: GameEventContext,
    ) -> Result<(), LevelError> {
        let center = BlockPos::containing(position.x, position.y, position.z);
        let radius = event.notification_radius();
        let min = BlockPos::new(
            center.x.wrapping_sub(radius) >> 4,
            center.y.wrapping_sub(radius) >> 4,
            center.z.wrapping_sub(radius) >> 4,
        );
        let max = BlockPos::new(
            center.x.wrapping_add(radius) >> 4,
            center.y.wrapping_add(radius) >> 4,
            center.z.wrapping_add(radius) >> 4,
        );
        let mut queued = Vec::new();
        let result: Result<(), LevelError> = (|| {
            for x in min.x..=max.x {
                for z in min.z..=max.z {
                    let chunk = ChunkPos::new(x, z);
                    if level.get_chunk_now(chunk)?.is_none() {
                        continue;
                    }
                    let owner = level.capture_listener_owner(chunk)?;
                    let result: Result<(), LevelError> = (|| {
                        for section in min.y..=max.y {
                            let registry = level.listener_registry(owner, section);
                            visit(level, registry, event, position, context, &mut queued)?;
                        }
                        Ok(())
                    })();
                    level.release_listener_owner(owner);
                    result?;
                }
            }
            // Java's stable Collections.sort keeps encounter order at equal
            // distances. Squared distances are nonnegative; all NaNs compare
            // equal and after finite values, as Double.compare does.
            queued.sort_by(|a, b| {
                a.distance_squared
                    .partial_cmp(&b.distance_squared)
                    .unwrap_or_else(|| {
                        a.distance_squared
                            .is_nan()
                            .cmp(&b.distance_squared.is_nan())
                    })
            });
            for info in &queued {
                info.listener.deliver(level, event, context, position)?;
            }
            Ok(())
        })();
        // ListenerInfo's entire native list retains all recipients through all
        // deliveries, including those already called before a later callback.
        for info in queued {
            info.listener.release(level);
        }
        result
    }
}

fn visit(
    level: &mut Level,
    registry: ListenerRegistryRef,
    event: GameEvent,
    position: Vec3,
    context: GameEventContext,
    queued: &mut Vec<ListenerInfo>,
) -> Result<(), LevelError> {
    let state = level
        .region_mut()
        .game_event_listeners
        .get_mut()
        .registries
        .get_mut(&registry.0)
        .unwrap();
    state.processing = true;
    state.visits += 1;
    let mut expected_modification = state.modification_count;
    let mut index = 0;
    let result: Result<(), LevelError> = (|| {
        loop {
            let listener = {
                let store = level.region().game_event_listeners.borrow();
                let state = &store.registries[&registry.0];
                // ArrayList.Itr#hasNext precedes its modification check.
                if index == state.listeners.len() {
                    break;
                }
                if state.modification_count != expected_modification {
                    return Err(LevelError::ConcurrentGameEventListenerModification);
                }
                state.listeners[index]
            };
            let state = level
                .region_mut()
                .game_event_listeners
                .get_mut()
                .registries
                .get_mut(&registry.0)
                .unwrap();
            if state.listeners_to_remove.remove(&listener) {
                state.listeners.remove(index);
                state.modification_count = state.modification_count.wrapping_add(1);
                expected_modification = state.modification_count;
                listener.release(level); // pending-removal set
                listener.release(level); // actual list entry
                continue;
            }
            index += 1;
            let behavior = listener.behavior(level)?;
            let Some(destination) = behavior
                .get_listener_source(listener, level)?
                .get_position(level)?
            else {
                continue;
            };
            let listener_radius = behavior.get_listener_radius(listener, level)?;
            let source_block = BlockPos::containing(position.x, position.y, position.z);
            let target_block = BlockPos::containing(destination.x, destination.y, destination.z);
            let dx = f64::from(source_block.x) - f64::from(target_block.x);
            let dy = f64::from(source_block.y) - f64::from(target_block.y);
            let dz = f64::from(source_block.z) - f64::from(target_block.z);
            if dx * dx + dy * dy + dz * dz
                > f64::from(listener_radius.wrapping_mul(listener_radius))
            {
                continue;
            }
            if behavior.get_delivery_mode() == DeliveryMode::ByDistance {
                listener.retain_delivery(level)?;
                queued.push(ListenerInfo {
                    listener,
                    distance_squared: position.distance_to_sqr(destination),
                });
            } else {
                listener.deliver(level, event, context, position)?;
            }
        }
        Ok(())
    })();
    // Native finally resets a boolean, not a nesting counter. A nested visit
    // can flush mutations and invalidate its caller's ArrayList iterator.
    level
        .region_mut()
        .game_event_listeners
        .get_mut()
        .registries
        .get_mut(&registry.0)
        .unwrap()
        .processing = false;
    if result.is_ok() {
        flush(level, registry);
    }
    level
        .region_mut()
        .game_event_listeners
        .get_mut()
        .registries
        .get_mut(&registry.0)
        .unwrap()
        .visits -= 1;
    level.collect_listener_registry(registry);
    result
}
fn flush(level: &mut Level, registry: ListenerRegistryRef) {
    let state = level
        .region_mut()
        .game_event_listeners
        .get_mut()
        .registries
        .get_mut(&registry.0)
        .unwrap();
    if !state.listeners_to_add.is_empty() {
        state.listeners.append(&mut state.listeners_to_add);
        state.modification_count = state.modification_count.wrapping_add(1);
    }
    let pending = std::mem::take(&mut state.listeners_to_remove);
    let mut removed = Vec::new();
    state.listeners.retain(|listener| {
        if pending.contains(listener) {
            removed.push(*listener);
            false
        } else {
            true
        }
    });
    state.modification_count = state.modification_count.wrapping_add(removed.len() as i32);
    for listener in removed.into_iter().chain(pending) {
        listener.release(level);
    }
}
