use super::{ChunkLevel, ChunkPos, HashMapType};
use crate::chunk_system::chunk_state::StagedChunkEnum;
use crate::chunk_system::worker_logic::WriteRequest;
use crate::level::Level;
use crossbeam::queue::SegQueue;
use std::collections::hash_map::Entry;
use std::mem::swap;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, Condvar, Mutex};

pub type LevelChange = (
    HashMapType<ChunkPos, (StagedChunkEnum, StagedChunkEnum)>,
    ChunkLevel,
);

pub struct LevelChannel {
    pub value: Mutex<(Option<LevelChange>, Option<Vec<ChunkPos>>)>,
    pub notify: Condvar,
    write_requests: SegQueue<WriteRequest>,
}

impl Default for LevelChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl LevelChannel {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            value: Mutex::new((None, None)),
            notify: Condvar::new(),
            write_requests: SegQueue::new(),
        }
    }

    pub fn request_flush(&self) -> tokio::sync::oneshot::Receiver<Result<(), String>> {
        let (send, receive) = tokio::sync::oneshot::channel();
        // Share the waiter's mutex to avoid losing a wakeup between its
        // empty-queue check and condvar wait.
        let _guard = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.write_requests.push(WriteRequest::Flush(vec![send]));
        self.notify.notify_one();
        receive
    }

    pub(crate) fn request_entity_save(&self) {
        let _guard = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.write_requests.push(WriteRequest::Entities);
        self.notify.notify_one();
    }

    pub(crate) fn take_write_requests(&self) -> Vec<WriteRequest> {
        let mut requests = Vec::new();
        while let Some(request) = self.write_requests.pop() {
            requests.push(request);
        }
        requests
    }
    pub fn set_both(
        &self,
        new_value: (
            HashMapType<ChunkPos, (StagedChunkEnum, StagedChunkEnum)>,
            ChunkLevel,
        ),
        pos: Vec<ChunkPos>,
    ) {
        // debug!("set new level and priority");
        let mut value = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        value.1 = Some(pos);
        if let Some(old) = &mut value.0 {
            for (pos, change) in new_value.0 {
                match old.0.entry(pos) {
                    Entry::Occupied(mut entry) => {
                        let tmp = entry.get_mut();
                        debug_assert_eq!(tmp.1, change.0);
                        if tmp.0 == change.1 {
                            entry.remove();
                        } else {
                            tmp.1 = change.1;
                        }
                    }
                    Entry::Vacant(entry) => {
                        debug_assert_ne!(change.0, change.1);
                        entry.insert(change);
                    }
                }
            }
            old.1 = new_value.1;
        } else {
            value.0 = Some(new_value);
        }
        self.notify.notify_one();
    }
    pub fn set_level(
        &self,
        new_value: (
            HashMapType<ChunkPos, (StagedChunkEnum, StagedChunkEnum)>,
            ChunkLevel,
        ),
    ) {
        // debug!("set new level");
        let mut value = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(old) = &mut value.0 {
            for (pos, change) in new_value.0 {
                match old.0.entry(pos) {
                    Entry::Occupied(mut entry) => {
                        let tmp = entry.get_mut();
                        debug_assert_eq!(tmp.1, change.0);
                        if tmp.0 == change.1 {
                            entry.remove();
                        } else {
                            tmp.1 = change.1;
                        }
                    }
                    Entry::Vacant(entry) => {
                        debug_assert_ne!(change.0, change.1);
                        entry.insert(change);
                    }
                }
            }
            old.1 = new_value.1;
        } else {
            value.0 = Some(new_value);
        }
        self.notify.notify_one();
    }
    pub fn set_priority(&self, pos: Vec<ChunkPos>) {
        // debug!("set new priority");
        self.value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .1 = Some(pos);
        self.notify.notify_one();
    }
    pub fn get(&self) -> (Option<LevelChange>, Option<Vec<ChunkPos>>) {
        let mut lock = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut ret = (None, None);
        swap(&mut ret, &mut *lock);
        ret
    }
    pub fn wait_and_get(&self, level: &Arc<Level>) -> (Option<LevelChange>, Option<Vec<ChunkPos>>) {
        let mut lock = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while lock.0.is_none()
            && lock.1.is_none()
            && !level.should_unload.load(SeqCst)
            && !level.should_save.load(SeqCst)
            && self.write_requests.is_empty()
            && !level.shut_down_chunk_system.load(SeqCst)
        {
            let (new_lock, timeout_res) = self
                .notify
                .wait_timeout(lock, std::time::Duration::from_secs(1))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            lock = new_lock;
            if timeout_res.timed_out() {
                break;
            }
        }
        let mut ret = (None, None);
        swap(&mut ret, &mut *lock);
        ret
    }
    pub fn notify(&self) {
        let val = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        drop(val);
        self.notify.notify_one();
    }
}
