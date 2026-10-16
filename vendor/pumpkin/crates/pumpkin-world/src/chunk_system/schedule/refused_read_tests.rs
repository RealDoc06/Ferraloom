use super::*;

/// Build the scheduler's read-dispatch graph without starting IO or generation workers.
fn failed_read_graph() -> (DAG, HashMap<ChunkPos, ChunkHolder>, NodeKey) {
    let mut graph = DAG::default();
    let pos = ChunkPos::new(0, 0);
    let read = graph.nodes.insert(Node::new(pos, StagedChunkEnum::Empty));
    graph.nodes[read].in_flight = true;
    let fence = graph.nodes.insert(Node::new(
        ChunkPos::new(i32::MAX, i32::MAX),
        StagedChunkEnum::None,
    ));
    graph.add_edge(fence, read);
    let mut holder = ChunkHolder {
        target_stage: StagedChunkEnum::Empty,
        occupied: fence,
        ..ChunkHolder::default()
    };
    holder.tasks[StagedChunkEnum::Empty as usize] = read;
    GenerationSchedule::finish_failed_read(&mut graph, &mut holder);
    (graph, HashMap::from([(pos, holder)]), read)
}

#[test]
fn failed_read_blocks_pending_chain_but_unrelated_idle_nodes_are_rejected() {
    let (mut graph, mut chunk_map, read) = failed_read_graph();
    let pos = ChunkPos::new(0, 0);
    let dependent_pos = ChunkPos::new(1, 0);
    let dependent = graph
        .nodes
        .insert(Node::new(dependent_pos, StagedChunkEnum::Surface));
    let mut dependent_holder = ChunkHolder {
        target_stage: StagedChunkEnum::Surface,
        current_stage: StagedChunkEnum::Noise,
        ..ChunkHolder::default()
    };
    dependent_holder.tasks[StagedChunkEnum::Surface as usize] = dependent;
    let mut queue = BinaryHeap::new();
    GenerationSchedule::ensure_dependency_chain(
        &mut graph,
        &mut queue,
        &ChunkLevel::default(),
        &[],
        dependent,
        pos,
        chunk_map.get_mut(&pos).unwrap(),
        StagedChunkEnum::Surface,
    );
    chunk_map.insert(dependent_pos, dependent_holder);
    let blocked = GenerationSchedule::blocked_read_nodes(&graph, &chunk_map);
    assert!(blocked.contains(&read));
    assert!(blocked.contains(&dependent));
    assert!(queue.is_empty());
    assert!(chunk_map[&pos].chunk.is_none());
    assert_eq!(chunk_map[&pos].current_stage, StagedChunkEnum::None);
    assert!(!chunk_map[&pos].public);

    let stray = graph
        .nodes
        .insert(Node::new(ChunkPos::new(100, 100), StagedChunkEnum::Biomes));
    assert!(
        std::panic::catch_unwind(|| { GenerationSchedule::blocked_read_nodes(&graph, &chunk_map) })
            .is_err()
    );
    graph.nodes.remove(stray);
    assert_eq!(
        GenerationSchedule::blocked_read_nodes(&graph, &chunk_map),
        blocked,
    );

    graph.nodes[dependent].in_degree += 1;
    assert!(
        std::panic::catch_unwind(|| { GenerationSchedule::blocked_read_nodes(&graph, &chunk_map) })
            .is_err()
    );
    graph.nodes[dependent].in_degree -= 1;
    graph.add_edge(dependent, read);
    assert!(
        std::panic::catch_unwind(|| { GenerationSchedule::blocked_read_nodes(&graph, &chunk_map) })
            .is_err()
    );
}

#[test]
fn parked_read_only_neighbor_gets_one_fence_while_healthy_work_stays_ready() {
    let (mut graph, mut chunk_map, read) = failed_read_graph();
    let failed_pos = ChunkPos::new(0, 0);
    let retained_fence = chunk_map[&failed_pos].occupied;
    let waiting_pos = ChunkPos::new(1, 0);
    let waiting_task = graph
        .nodes
        .insert(Node::new(waiting_pos, StagedChunkEnum::Surface));
    let mut waiting_holder = ChunkHolder {
        target_stage: StagedChunkEnum::Surface,
        current_stage: StagedChunkEnum::Noise,
        ..ChunkHolder::default()
    };
    waiting_holder.tasks[StagedChunkEnum::Surface as usize] = waiting_task;
    chunk_map.insert(waiting_pos, waiting_holder);
    let waiting = HashSetType::from_iter([waiting_task]);
    assert_eq!(graph.nodes[waiting_task].in_degree, 0);
    GenerationSchedule::hold_failed_read_waiters(&mut graph, &chunk_map, &waiting);
    let edge_count = graph.edges.len();
    assert_eq!(graph.nodes[waiting_task].in_degree, 1);
    GenerationSchedule::hold_failed_read_waiters(&mut graph, &chunk_map, &waiting);
    assert_eq!(graph.edges.len(), edge_count);
    assert_eq!(graph.nodes[waiting_task].in_degree, 1);
    let blocked = GenerationSchedule::blocked_read_nodes(&graph, &chunk_map);
    assert!(blocked.contains(&read) && blocked.contains(&waiting_task));

    let healthy_pos = ChunkPos::new(32, 0);
    let healthy_task = graph
        .nodes
        .insert(Node::new(healthy_pos, StagedChunkEnum::Surface));
    let mut healthy_holder = ChunkHolder {
        target_stage: StagedChunkEnum::Surface,
        current_stage: StagedChunkEnum::Noise,
        ..ChunkHolder::default()
    };
    healthy_holder.tasks[StagedChunkEnum::Surface as usize] = healthy_task;
    chunk_map.insert(healthy_pos, healthy_holder);
    let mut queue = BinaryHeap::new();
    assert_eq!(
        GenerationSchedule::restore_ready_tasks(
            &mut graph,
            &mut queue,
            &chunk_map,
            &ChunkLevel::default(),
            &[],
            &waiting,
        ),
        1,
    );
    assert_eq!(queue.pop().unwrap().node_key(), healthy_task);
    assert!(!graph.nodes[waiting_task].in_queue);
    assert!(!graph.nodes[read].in_flight);
    assert_eq!(chunk_map[&failed_pos].occupied, retained_fence);
    assert!(graph.nodes.contains_key(retained_fence));
    assert!(chunk_map[&failed_pos].chunk.is_none());
}

#[test]
fn healthy_idle_graph_still_rejects_any_leftover_task() {
    let mut graph = DAG::default();
    let chunk_map = HashMap::new();
    assert!(GenerationSchedule::blocked_read_nodes(&graph, &chunk_map).is_empty());
    graph
        .nodes
        .insert(Node::new(ChunkPos::new(0, 0), StagedChunkEnum::Empty));
    assert!(
        std::panic::catch_unwind(|| { GenerationSchedule::blocked_read_nodes(&graph, &chunk_map) })
            .is_err()
    );
}

#[tokio::test]
async fn failed_dependency_notifies_full_waiters_after_read_but_keeps_disk_full_possible() {
    let (mut graph, mut chunk_map, read) = failed_read_graph();
    let failed = ChunkPos::new(0, 0);
    let dependent = ChunkPos::new(1, 0);
    let healthy = ChunkPos::new(2, 0);
    let full = graph
        .nodes
        .insert(Node::new(dependent, StagedChunkEnum::Full));
    graph.add_edge(read, full);
    let mut holder = ChunkHolder::default();
    holder.tasks[StagedChunkEnum::Full as usize] = full;
    chunk_map.insert(dependent, holder);
    let healthy_full = graph
        .nodes
        .insert(Node::new(healthy, StagedChunkEnum::Full));
    graph.add_edge(read, healthy_full);
    let mut healthy_holder = ChunkHolder::default();
    healthy_holder.tasks[StagedChunkEnum::Full as usize] = healthy_full;
    chunk_map.insert(healthy, healthy_holder);
    let listener = ChunkListener::new();
    let healthy_waiter = listener.add_single_chunk_listener(healthy);
    let error = ChunkUnavailable::ReadFailed {
        position: failed,
        reason: "refused dependency".into(),
    };
    listener.fail_chunk(failed, error.clone());
    let early = listener.add_single_chunk_listener(dependent);
    GenerationSchedule::notify_failed_read_dependents(&graph, &chunk_map, &listener);
    // A Full disk read could still bypass the speculative generation chain.
    assert!(listener.failure(dependent).is_none());
    assert_eq!(listener.pending_count(), 2);
    // This neighbor really arrives as Full, so its speculative dependency cannot
    // turn a successful disk load into an unavailable-chunk notification.
    chunk_map.get_mut(&healthy).unwrap().current_stage = StagedChunkEnum::Full;
    let healthy_chunk = crate::chunk::ChunkData::empty_sync(healthy.x, healthy.y);
    listener.process_new_chunk(healthy, &healthy_chunk);
    assert!(Arc::ptr_eq(
        &tokio::time::timeout(Duration::from_secs(1), healthy_waiter)
            .await
            .unwrap()
            .unwrap(),
        &healthy_chunk
    ));
    chunk_map.get_mut(&dependent).unwrap().current_stage = StagedChunkEnum::Empty;
    GenerationSchedule::notify_failed_read_dependents(&graph, &chunk_map, &listener);
    let late = listener.add_single_chunk_listener(dependent);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), early)
            .await
            .unwrap()
            .err(),
        Some(error.clone())
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), late)
            .await
            .unwrap()
            .err(),
        Some(error)
    );
    assert!(listener.failure(healthy).is_none());
    assert!(graph.nodes.contains_key(chunk_map[&failed].occupied));
    assert_eq!(graph.nodes[full].in_degree, 1);
}
