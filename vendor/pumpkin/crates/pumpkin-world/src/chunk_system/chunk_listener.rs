use super::{ChunkPos, HashMapType};
use crate::level::SyncChunk;
use crossbeam::channel::{Receiver, Sender};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::{Mutex, Weak};
use std::task::{Context, Poll};
use tokio::sync::oneshot;

/// A terminal load failure. Clones retain the same diagnostic for every waiter.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ChunkUnavailable {
    #[error("Chunk {position:?} could not be read: {reason}")]
    ReadFailed {
        position: ChunkPos,
        reason: Arc<str>,
    },
    #[error("Chunk loading stopped while waiting for {position:?}")]
    Closed { position: ChunkPos },
}

type ChunkResult = Result<SyncChunk, ChunkUnavailable>;

#[derive(Default)]
struct SingleListeners {
    next_id: u64,
    pending: HashMapType<u64, (ChunkPos, oneshot::Sender<ChunkResult>)>,
    failures: HashMapType<ChunkPos, ChunkUnavailable>,
}

/// Dropping a fetch also removes its registration, even if the chunk never arrives.
pub struct ChunkWaiter<'a> {
    listener: &'a ChunkListener,
    id: u64,
    pos: ChunkPos,
    receiver: oneshot::Receiver<ChunkResult>,
}

impl Future for ChunkWaiter<'_> {
    type Output = ChunkResult;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.receiver)
            .poll(cx)
            .map(|result| result.unwrap_or(Err(ChunkUnavailable::Closed { position: self.pos })))
    }
}

impl Drop for ChunkWaiter<'_> {
    fn drop(&mut self) {
        self.listener
            .single
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending
            .remove(&self.id);
    }
}

#[expect(clippy::type_complexity)]
pub struct ChunkListener {
    single: Mutex<SingleListeners>,
    global: Mutex<Vec<Sender<(ChunkPos, Weak<crate::chunk::ChunkData>)>>>,
    pub failures_changed: tokio::sync::Notify,
}

impl Default for ChunkListener {
    fn default() -> Self {
        Self::new()
    }
}

impl ChunkListener {
    #[must_use]
    pub fn new() -> Self {
        Self {
            single: Mutex::default(),
            global: Mutex::new(Vec::new()),
            failures_changed: tokio::sync::Notify::new(),
        }
    }

    pub fn add_single_chunk_listener(&self, pos: ChunkPos) -> ChunkWaiter<'_> {
        let (tx, rx) = oneshot::channel();
        let mut single = self
            .single
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let id = single.next_id;
        single.next_id += 1;
        // Registration and failure publication use one lock: neither ordering can
        // strand a late subscriber behind a permanently retained read fence.
        if let Some(error) = single.failures.get(&pos) {
            let _ = tx.send(Err(error.clone()));
        } else {
            single.pending.insert(id, (pos, tx));
        }
        ChunkWaiter {
            listener: self,
            id,
            pos,
            receiver: rx,
        }
    }

    pub(crate) fn has_failures(&self) -> bool {
        !self
            .single
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .failures
            .is_empty()
    }

    pub fn failure(&self, pos: ChunkPos) -> Option<ChunkUnavailable> {
        self.single
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .failures
            .get(&pos)
            .cloned()
    }

    pub(crate) fn fail_chunk(&self, pos: ChunkPos, error: ChunkUnavailable) {
        let mut single = self
            .single
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if single.failures.contains_key(&pos) {
            return;
        }
        let error = single.failures.entry(pos).or_insert(error).clone();
        let waiting: Vec<_> = single
            .pending
            .iter()
            .filter_map(|(id, (waiting_pos, _))| (*waiting_pos == pos).then_some(*id))
            .collect();
        for id in waiting {
            if let Some((_, send)) = single.pending.remove(&id) {
                let _ = send.send(Err(error.clone()));
            }
        }
        self.failures_changed.notify_waiters();
    }

    #[cfg(test)]
    pub(crate) fn pending_count(&self) -> usize {
        self.single.lock().unwrap().pending.len()
    }

    pub fn add_global_chunk_listener(&self) -> Receiver<(ChunkPos, Weak<crate::chunk::ChunkData>)> {
        let (tx, rx) = crossbeam::channel::unbounded();
        self.global
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(tx);
        rx
    }

    pub fn process_new_chunk(&self, pos: ChunkPos, chunk: &SyncChunk) {
        {
            let mut single = self
                .single
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let waiting: Vec<_> = single
                .pending
                .iter()
                .filter_map(|(id, (waiting_pos, _))| (*waiting_pos == pos).then_some(*id))
                .collect();
            for id in waiting {
                if let Some((_, send)) = single.pending.remove(&id) {
                    let _ = send.send(Ok(chunk.clone()));
                }
            }
        }
        {
            let weak = Arc::downgrade(chunk);
            let mut global = self
                .global
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut i = 0;
            let mut len = global.len();
            while i < len {
                if matches!(global[i].send((pos, weak.clone())), Ok(())) {
                    // log::debug!("global listener {i} send {pos:?}");
                } else {
                    // log::debug!("one global listener dropped");
                    global.remove(i);
                    len -= 1;
                    continue;
                }
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkData;

    #[tokio::test]
    async fn refusal_before_and_after_registration_preserves_healthy_neighbor_delivery() {
        let listener = ChunkListener::new();
        let failed = ChunkPos::new(0, 0);
        let healthy = ChunkPos::new(1, 0);
        let early = listener.add_single_chunk_listener(failed);
        let neighbor = listener.add_single_chunk_listener(healthy);
        let error = ChunkUnavailable::ReadFailed {
            position: failed,
            reason: "refused stored chunk".into(),
        };
        listener.fail_chunk(failed, error.clone());
        let late = listener.add_single_chunk_listener(failed);
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), early)
                .await
                .unwrap()
                .err(),
            Some(error.clone())
        );
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), late)
                .await
                .unwrap()
                .err(),
            Some(error)
        );
        assert_eq!(listener.pending_count(), 1);
        assert!(listener.failure(healthy).is_none());
        let chunk = ChunkData::empty_sync(1, 0);
        listener.process_new_chunk(healthy, &chunk);
        assert!(Arc::ptr_eq(
            &tokio::time::timeout(std::time::Duration::from_secs(1), neighbor)
                .await
                .unwrap()
                .unwrap(),
            &chunk
        ));
        assert_eq!(listener.pending_count(), 0);
    }
    #[tokio::test]
    async fn closed_notification_is_an_error_instead_of_empty_terrain() {
        let listener = ChunkListener::new();
        let position = ChunkPos::new(0, 0);
        let waiter = listener.add_single_chunk_listener(position);
        listener.single.lock().unwrap().pending.clear();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
                .await
                .unwrap()
                .err(),
            Some(ChunkUnavailable::Closed { position })
        );
    }
}
