//! One owned continuation, recovered by the last participant without waiting.
use std::sync::{Arc, Mutex, OnceLock};

pub(super) struct OrderedBatch<T, C> {
    outputs: Vec<OnceLock<T>>,
    // Only a one-shot ownership handoff. No callback or game mutation runs
    // under a guard; the final Arc owner recovers this mutex by value.
    continuation: Mutex<Option<C>>,
}
impl<T, C> OrderedBatch<T, C> {
    pub fn new(count: usize, continuation: C) -> Arc<Self> {
        assert!(count > 0);
        Arc::new(Self {
            outputs: (0..count).map(|_| OnceLock::new()).collect(),
            continuation: Mutex::new(Some(continuation)),
        })
    }
    pub fn complete(self: Arc<Self>, ordinal: usize, output: T) -> Option<(C, Vec<T>)> {
        assert!(
            self.outputs[ordinal].set(output).is_ok(),
            "duplicate batch result"
        );
        self.release()
    }
    /// Every participant, including the dispatcher, releases through here.
    /// Unlike try_unwrap-and-drop, into_inner guarantees exactly one recovery
    /// even when finishers release their Arcs concurrently.
    pub fn release(self: Arc<Self>) -> Option<(C, Vec<T>)> {
        let owned = Arc::into_inner(self)?;
        let continuation = owned
            .continuation
            .into_inner()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .expect("batch owns its continuation");
        let outputs = owned
            .outputs
            .into_iter()
            .map(|slot| {
                slot.into_inner()
                    .expect("released participant filled its slot")
            })
            .collect();
        Some((continuation, outputs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, sync::mpsc, time::Duration};

    #[test]
    fn dispatcher_releases_after_all_workers_without_losing_completion() {
        let batch = OrderedBatch::new(3, Cell::new(7));
        for ordinal in (0..3).rev() {
            assert!(batch.clone().complete(ordinal, ordinal).is_none());
        }
        let (continuation, values) = batch.release().unwrap();
        assert_eq!(continuation.get(), 7);
        assert_eq!(values, [0, 1, 2]);
    }

    #[test]
    fn concurrent_last_finishers_recover_once_in_submission_order() {
        let batch = OrderedBatch::new(32, Cell::new(11));
        let (send, receive) = mpsc::channel();
        let mut workers = Vec::new();
        for ordinal in (0..32).rev() {
            let worker = batch.clone();
            let send = send.clone();
            workers.push(std::thread::spawn(move || {
                if let Some(output) = worker.complete(ordinal, ordinal) {
                    send.send(output).unwrap();
                }
            }));
        }
        if let Some(output) = batch.release() {
            send.send(output).unwrap();
        }
        drop(send);
        let (continuation, values) = receive.recv_timeout(Duration::from_secs(5)).unwrap();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(continuation.get(), 11);
        assert_eq!(values, (0..32).collect::<Vec<_>>());
        assert!(matches!(
            receive.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }
}
