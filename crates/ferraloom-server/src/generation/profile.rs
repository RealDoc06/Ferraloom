//! Optional, coarse generation intervals. These never enter parity observations
//! or influence scheduling; enable with FERRALOOM_WORLDGEN_TIMING=1.
use ferraloom_game::level::generation::{ChunkStatus, status_name};
use ferraloom_util::ChunkPos;
use std::{
    collections::BTreeMap,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static ENABLED: OnceLock<bool> = OnceLock::new();
static NEXT_INTERVAL: AtomicU64 = AtomicU64::new(1);

pub(crate) struct Span(Option<Interval>);
struct Interval {
    id: u64,
    parent: Option<u64>,
    phase: &'static str,
    context: Option<(u64, ChunkPos)>,
    status: Option<ChunkStatus>,
    items: usize,
    started_unix_ns: u128,
    start: Instant,
    last_checkpoint: Duration,
    work: BTreeMap<&'static str, (usize, Duration)>,
}
impl Span {
    pub(crate) fn new(phase: &'static str, context: Option<(u64, ChunkPos)>) -> Self {
        Self(
            if *ENABLED.get_or_init(|| {
                std::env::var_os("FERRALOOM_WORLDGEN_TIMING").is_some_and(|value| value == "1")
            }) {
                Some(Interval {
                    id: NEXT_INTERVAL.fetch_add(1, Ordering::Relaxed),
                    parent: None,
                    phase,
                    context,
                    status: None,
                    items: 0,
                    started_unix_ns: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos(),
                    start: Instant::now(),
                    last_checkpoint: Duration::ZERO,
                    work: BTreeMap::new(),
                })
            } else {
                None
            },
        )
    }
    pub(crate) fn status(mut self, status: ChunkStatus) -> Self {
        if let Some(interval) = &mut self.0 {
            interval.status = Some(status);
        }
        self
    }
    pub(crate) fn parent(mut self, parent: Option<u64>) -> Self {
        if let Some(interval) = &mut self.0 {
            interval.parent = parent;
        }
        self
    }
    pub(crate) fn id(&self) -> Option<u64> {
        self.0.as_ref().map(|interval| interval.id)
    }
    pub(crate) fn items(&mut self, items: usize) {
        if let Some(interval) = &mut self.0 {
            interval.items = items;
        }
    }
    /// Attribute successive startup intervals without moving any operation,
    /// including awaits, into a closure. Checkpoints partition this span.
    pub(crate) fn checkpoint(&mut self, phase: &'static str) {
        let Some(interval) = &mut self.0 else { return };
        let elapsed = interval.start.elapsed();
        let entry = interval.work.entry(phase).or_default();
        entry.0 += 1;
        entry.1 += elapsed - interval.last_checkpoint;
        interval.last_checkpoint = elapsed;
    }
    pub(crate) fn measure<T>(&mut self, phase: &'static str, work: impl FnOnce() -> T) -> T {
        let Some(interval) = &mut self.0 else {
            return work();
        };
        let start = Instant::now();
        let result = work();
        let entry = interval.work.entry(phase).or_default();
        entry.0 += 1;
        entry.1 += start.elapsed();
        result
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        let Some(interval) = &self.0 else { return };
        let elapsed = interval.start.elapsed();
        let thread = std::thread::current();
        let work = interval.work.iter().map(|(phase, (count, elapsed))| {
            (*phase, serde_json::json!({"count": count, "elapsed_ms": elapsed.as_secs_f64() * 1000.}))
        }).collect::<BTreeMap<_, _>>();
        eprintln!(
            "WORLDGEN_SPAN {}",
            serde_json::json!({
                "id": interval.id,
                "parent": interval.parent,
                "phase": interval.phase,
                "request": interval.context.map(|(request, _)| request),
                "position": interval.context.map(|(_, pos)| [pos.x, pos.z]),
                "status": interval.status.map(status_name),
                "items": interval.items,
                "started_unix_ns": interval.started_unix_ns,
                "elapsed_ms": elapsed.as_secs_f64() * 1000.,
                "thread": thread.name(),
                "work": work,
            })
        );
    }
}
