//! Off-tick CPU pools. Completion order never determines simulation integration.
use anyhow::{Result, anyhow};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use tokio::sync::oneshot;
type Job = Box<dyn FnOnce() + Send>;
pub struct JobPool {
    senders: Vec<mpsc::Sender<Job>>,
    next: AtomicUsize,
}
impl JobPool {
    pub fn new(name: &str, count: usize) -> Result<Self> {
        let mut senders = Vec::new();
        for index in 0..count {
            let (send, recv) = mpsc::channel::<Job>();
            std::thread::Builder::new()
                .name(format!("{name}-{index}"))
                .spawn(move || {
                    while let Ok(job) = recv.recv() {
                        job();
                    }
                })?;
            senders.push(send);
        }
        Ok(Self {
            senders,
            next: AtomicUsize::new(0),
        })
    }
    pub(crate) fn parallelism(&self) -> usize {
        self.senders.len()
    }
    pub fn submit<T: Send + 'static>(
        &self,
        job: impl FnOnce() -> T + Send + 'static,
    ) -> oneshot::Receiver<T> {
        let (send, recv) = oneshot::channel();
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.senders.len();
        let _ = self.senders[index].send(Box::new(move || {
            let _ = send.send(job());
        }));
        recv
    }
    pub async fn run<T: Send + 'static>(
        &self,
        job: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T> {
        self.submit(job)
            .await
            .map_err(|_| anyhow!("off-tick worker terminated"))
    }
}
