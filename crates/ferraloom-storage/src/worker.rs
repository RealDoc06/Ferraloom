use crate::{ChunkPos, Error, RegionFileStorage, RegionFileVersion, Result};
use ferraloom_nbt::CompoundTag;
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Condvar, Mutex, mpsc},
    task::{Context, Poll, Waker},
    thread::{self, JoinHandle},
};

type Callback<T> = Box<dyn FnOnce(Result<T>) + Send>;
struct CompletionState<T> {
    result: Option<Result<T>>,
    waker: Option<Waker>,
    callback: Option<Callback<T>>,
}
struct Completion<T> {
    value: Mutex<CompletionState<T>>,
    ready: Condvar,
}
/// A nonblocking storage operation. Await it from any executor; `wait` is for shutdown/tools only.
pub struct IoTask<T> {
    state: Arc<Completion<T>>,
}
impl<T> IoTask<T> {
    fn pair() -> (Self, Arc<Completion<T>>) {
        let state = Arc::new(Completion {
            value: Mutex::new(CompletionState {
                result: None,
                waker: None,
                callback: None,
            }),
            ready: Condvar::new(),
        });
        (
            Self {
                state: state.clone(),
            },
            state,
        )
    }
    /// Deliver once without an executor. The callback only hands owned work to
    /// its destination; neither completion nor registration holds the IO guard.
    pub fn on_complete(self, callback: impl FnOnce(Result<T>) + Send + 'static) {
        let callback: Callback<T> = Box::new(callback);
        let result = {
            let mut state = self.state.value.lock().unwrap_or_else(|e| e.into_inner());
            match state.result.take() {
                Some(result) => result,
                None => {
                    state.callback = Some(callback);
                    return;
                }
            }
        };
        callback(result);
    }
    pub fn wait(self) -> Result<T> {
        let mut state = self.state.value.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(value) = state.result.take() {
                return value;
            }
            state = self
                .state
                .ready
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
    }
}
impl<T> Future for IoTask<T> {
    type Output = Result<T>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.value.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(value) = state.result.take() {
            Poll::Ready(value)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
impl<T> Completion<T> {
    fn finish(&self, value: Result<T>) {
        let (callback, waker, value) = {
            let mut state = self.value.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(callback) = state.callback.take() {
                (Some(callback), state.waker.take(), Some(value))
            } else {
                state.result = Some(value);
                (None, state.waker.take(), None)
            }
        };
        self.ready.notify_all();
        if let Some(waker) = waker {
            waker.wake();
        }
        if let Some(callback) = callback {
            callback(value.expect("callback owns the completion"));
        }
    }
}
type Reply<T> = Arc<Completion<T>>;
enum Command {
    Store(ChunkPos, Option<CompoundTag>, Reply<()>),
    Load(ChunkPos, Reply<Option<CompoundTag>>),
    Synchronize(bool, Reply<()>),
    Shutdown(Reply<()>),
}
struct Pending {
    data: Option<CompoundTag>,
    replies: Vec<Reply<()>>,
}

/// Port of IOWorker: foreground reads observe queued writes, which coalesce per chunk.
/// One dedicated thread owns the region cache. Submitting work never performs disk IO.
pub struct IoWorker {
    sender: Option<mpsc::Sender<Command>>,
    thread: Option<JoinHandle<()>>,
}
impl IoWorker {
    pub fn new(folder: impl Into<PathBuf>, version: RegionFileVersion, sync: bool) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let storage = RegionFileStorage::new(folder, version, sync);
        let thread = thread::Builder::new()
            .name("ferraloom-storage".into())
            .spawn(move || run(storage, receiver))?;
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
        })
    }
    fn submit<T>(&self, make: impl FnOnce(Reply<T>) -> Command) -> IoTask<T> {
        let (task, reply) = IoTask::pair();
        if self
            .sender
            .as_ref()
            .is_none_or(|sender| sender.send(make(reply.clone())).is_err())
        {
            reply.finish(Err(Error::Closed));
        }
        task
    }
    pub fn store(&self, pos: ChunkPos, value: Option<CompoundTag>) -> IoTask<()> {
        self.submit(|reply| Command::Store(pos, value, reply))
    }
    pub fn load(&self, pos: ChunkPos) -> IoTask<Option<CompoundTag>> {
        self.submit(|reply| Command::Load(pos, reply))
    }
    /// Waits for every earlier write; `flush` also forces all region files to stable storage.
    pub fn synchronize(&self, flush: bool) -> IoTask<()> {
        self.submit(|reply| Command::Synchronize(flush, reply))
    }
    /// Initiates shutdown without blocking the caller. All preceding writes are drained and synced.
    pub fn shutdown(&mut self) -> IoTask<()> {
        let task = self.submit(Command::Shutdown);
        self.sender.take();
        task
    }
    /// Call off the tick thread. Drop also joins so queued saves cannot be abandoned.
    pub fn close(&mut self) -> Result<()> {
        let result = if self.sender.is_some() {
            self.shutdown().wait()
        } else {
            Ok(())
        };
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| Error::Invalid("storage thread terminated unexpectedly".into()))?;
        }
        result
    }
}
impl Drop for IoWorker {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            log::error!("Failed to close storage worker: {e}");
        }
    }
}
fn run(mut storage: RegionFileStorage, receiver: mpsc::Receiver<Command>) {
    let mut pending = HashMap::<ChunkPos, Pending>::new();
    let mut order = VecDeque::new();
    let mut failures = Vec::new();
    loop {
        let command = if pending.is_empty() {
            match receiver.recv() {
                Ok(c) => Some(c),
                Err(_) => break,
            }
        } else {
            match receiver.try_recv() {
                Ok(c) => Some(c),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        };
        match command {
            Some(Command::Store(pos, data, reply)) => {
                let entry = pending.entry(pos).or_insert_with(|| {
                    order.push_back(pos);
                    Pending {
                        data: None,
                        replies: Vec::new(),
                    }
                });
                entry.data = data;
                entry.replies.push(reply);
            }
            Some(Command::Load(pos, reply)) => reply.finish(match pending.get(&pos) {
                Some(p) => Ok(p.data.clone()),
                None => storage.read(pos),
            }),
            Some(Command::Synchronize(flush, reply)) => {
                drain(&mut storage, &mut pending, &mut order, &mut failures);
                let result = if flush { storage.flush() } else { Ok(()) };
                reply.finish(report_failures(&mut failures).and(result));
            }
            Some(Command::Shutdown(reply)) => {
                drain(&mut storage, &mut pending, &mut order, &mut failures);
                let result = storage.close();
                reply.finish(report_failures(&mut failures).and(result));
                return;
            }
            None => write_one(&mut storage, &mut pending, &mut order, &mut failures),
        }
    }
    drain(&mut storage, &mut pending, &mut order, &mut failures);
    if let Err(e) = storage.close() {
        log::error!("Failed to close region storage: {e}");
    }
}
fn write_one(
    storage: &mut RegionFileStorage,
    pending: &mut HashMap<ChunkPos, Pending>,
    order: &mut VecDeque<ChunkPos>,
    failures: &mut Vec<String>,
) {
    if let Some(pos) = order.pop_front()
        && let Some(pending) = pending.remove(&pos)
    {
        let result = storage.write(pos, pending.data.as_ref());
        let error = result.err().map(|e| e.to_string());
        if let Some(error) = &error {
            log::error!("Failed to store chunk {pos:?}: {error}");
            failures.push(error.clone());
        }
        for reply in pending.replies {
            reply.finish(
                error
                    .as_ref()
                    .map_or(Ok(()), |e| Err(Error::Invalid(e.clone()))),
            );
        }
    }
}
fn drain(
    storage: &mut RegionFileStorage,
    pending: &mut HashMap<ChunkPos, Pending>,
    order: &mut VecDeque<ChunkPos>,
    failures: &mut Vec<String>,
) {
    while !order.is_empty() {
        write_one(storage, pending, order, failures);
    }
}
fn report_failures(failures: &mut Vec<String>) -> Result<()> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::Invalid(std::mem::take(failures).join("; ")))
    }
}

#[cfg(test)]
mod callback_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn completed_task_calls_back_on_registering_thread_outside_guard() {
        let (task, completion) = IoTask::<u32>::pair();
        completion.finish(Ok(42));
        let registering = thread::current().id();
        let (send, receive) = mpsc::channel();
        task.on_complete(move |result| {
            assert!(completion.value.try_lock().is_ok());
            send.send((thread::current().id(), result.unwrap()))
                .unwrap();
        });
        assert_eq!(receive.try_recv().unwrap(), (registering, 42));
        assert!(matches!(
            receive.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn pending_task_calls_back_once_on_completion_outside_guard() {
        let (task, completion) = IoTask::<u32>::pair();
        let held = completion.clone();
        let (send, receive) = mpsc::channel();
        task.on_complete(move |result| {
            assert!(held.value.try_lock().is_ok());
            send.send((thread::current().id(), result.unwrap()))
                .unwrap();
        });
        assert!(matches!(receive.try_recv(), Err(mpsc::TryRecvError::Empty)));
        let worker = thread::spawn(move || {
            completion.finish(Ok(17));
            thread::current().id()
        });
        let (thread, value) = receive.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(thread, worker.join().unwrap());
        assert_eq!(value, 17);
        assert!(matches!(
            receive.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn callback_preserves_errors_before_and_after_registration() {
        for ready in [false, true] {
            let (task, completion) = IoTask::<u32>::pair();
            if ready {
                completion.finish(Err(Error::Closed));
            }
            let held = completion.clone();
            let (send, receive) = mpsc::channel();
            task.on_complete(move |result| {
                assert!(held.value.try_lock().is_ok());
                send.send(result).unwrap();
            });
            if !ready {
                completion.finish(Err(Error::Closed));
            }
            assert!(matches!(
                receive.recv_timeout(Duration::from_secs(5)).unwrap(),
                Err(Error::Closed)
            ));
            assert!(matches!(
                receive.try_recv(),
                Err(mpsc::TryRecvError::Disconnected)
            ));
        }
    }
}
