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

struct Completion<T> {
    value: Mutex<(Option<Result<T>>, Option<Waker>)>,
    ready: Condvar,
}
/// A nonblocking storage operation. Await it from any executor; `wait` is for shutdown/tools only.
pub struct IoTask<T> {
    state: Arc<Completion<T>>,
}
impl<T> IoTask<T> {
    fn pair() -> (Self, Arc<Completion<T>>) {
        let state = Arc::new(Completion {
            value: Mutex::new((None, None)),
            ready: Condvar::new(),
        });
        (
            Self {
                state: state.clone(),
            },
            state,
        )
    }
    pub fn wait(self) -> Result<T> {
        let mut state = self.state.value.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(value) = state.0.take() {
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
        if let Some(value) = state.0.take() {
            Poll::Ready(value)
        } else {
            state.1 = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
impl<T> Completion<T> {
    fn finish(&self, value: Result<T>) {
        let waker = {
            let mut state = self.value.lock().unwrap_or_else(|e| e.into_inner());
            state.0 = Some(value);
            state.1.take()
        };
        self.ready.notify_all();
        if let Some(waker) = waker {
            waker.wake();
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
    if let Some(pos) = order.pop_front() {
        if let Some(pending) = pending.remove(&pos) {
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
