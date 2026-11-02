//! Named timings within a frame, for finding what makes a frame slow:
//! `HORIZON_SPIKES=MS` prints the spans of every frame whose CPU time
//! exceeds MS milliseconds, and of every server tick (dedicated or
//! integrated) longer than MS. Each span keeps its wall time and the
//! thread's CPU time inside it, so a span that waited (for a lock, the GPU
//! or a core on a busy machine) shows apart from one that worked. Costs two
//! clock reads per span when enabled and a relaxed load otherwise.
use std::{
    cell::RefCell,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

static ENABLED: AtomicBool = AtomicBool::new(false);
thread_local! {
    static SPANS: RefCell<Vec<Span>> = const { RefCell::new(Vec::new()) };
}

/// A named timing: wall milliseconds and the thread's CPU milliseconds in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub name: &'static str,
    pub ms: f32,
    pub cpu_ms: f32,
}

pub fn enable(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}
/// Run `f`, recording its duration under `name` when spans are enabled.
#[inline]
pub fn time<T>(name: &'static str, f: impl FnOnce() -> T) -> T {
    if !ENABLED.load(Ordering::Relaxed) {
        return f();
    }
    let (started, cpu) = (Instant::now(), thread_cpu());
    let out = f();
    let ms = started.elapsed().as_secs_f32() * 1000.0;
    let cpu_ms = thread_cpu()
        .zip(cpu)
        .map_or(ms, |(end, start)| (end - start).as_secs_f32() * 1000.0);
    SPANS.with(|s| s.borrow_mut().push(Span { name, ms, cpu_ms }));
    out
}
/// A span from here to the end of the enclosing scope, for blocks that
/// borrow too much to fit in `time`'s closure: `let _span = spans::scope("x");`
#[inline]
pub fn scope(name: &'static str) -> Scope {
    Scope(
        ENABLED
            .load(Ordering::Relaxed)
            .then(|| (name, Instant::now(), thread_cpu())),
    )
}
/// The running span `scope` returns; it records itself when dropped.
pub struct Scope(Option<(&'static str, Instant, Option<Duration>)>);
impl Drop for Scope {
    fn drop(&mut self) {
        let Some((name, started, cpu)) = self.0.take() else {
            return;
        };
        let ms = started.elapsed().as_secs_f32() * 1000.0;
        let cpu_ms = thread_cpu()
            .zip(cpu)
            .map_or(ms, |(end, start)| (end - start).as_secs_f32() * 1000.0);
        SPANS.with(|s| s.borrow_mut().push(Span { name, ms, cpu_ms }));
    }
}
/// The CPU time this thread has used, where the platform tells it: a tick
/// with more wall time than CPU time waited (for a lock, or for a core on a
/// busy machine).
pub fn thread_cpu() -> Option<Duration> {
    #[cfg(unix)]
    {
        let t = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
        Some(Duration::new(t.tv_sec as u64, t.tv_nsec as u32))
    }
    #[cfg(not(unix))]
    horizon_threads::thread_cpu()
}

/// Mean per tick of each span while a profile runs (`summary_start`), written
/// beside the profile: wall time, unlike its samples.
#[derive(Default)]
struct Summary {
    ticks: u64,
    wall_ms: f64,
    spans: std::collections::BTreeMap<&'static str, (f64, f64)>,
}
static SUMMARY: std::sync::Mutex<Option<Summary>> = std::sync::Mutex::new(None);
static SUMMING: AtomicBool = AtomicBool::new(false);

fn summary() -> std::sync::MutexGuard<'static, Option<Summary>> {
    SUMMARY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
/// Start summing spans, whatever `HORIZON_SPIKES` says.
pub fn summary_start() {
    *summary() = Some(Summary::default());
    SUMMING.store(true, Ordering::Relaxed);
    enable(true);
}
/// Whether a summary is being made: the ticks' spans are to be taken for it.
pub fn summing() -> bool {
    SUMMING.load(Ordering::Relaxed)
}
/// A tick's spans, for the summary.
pub fn summary_add(spans: &[Span], wall_ms: f32) {
    if let Some(sum) = summary().as_mut() {
        sum.ticks += 1;
        sum.wall_ms += f64::from(wall_ms);
        for s in spans {
            let e = sum.spans.entry(s.name).or_default();
            e.0 += f64::from(s.ms);
            e.1 += f64::from(s.cpu_ms);
        }
    }
}
/// A span of another thread's work (which `take` doesn't see), for the summary.
pub fn summary_add_span(name: &'static str, ms: f32, cpu_ms: f32) {
    if summing()
        && let Some(sum) = summary().as_mut()
    {
        let e = sum.spans.entry(name).or_default();
        e.0 += f64::from(ms);
        e.1 += f64::from(cpu_ms);
    }
}
/// Stop summing: per span, the mean wall and CPU milliseconds per tick.
pub fn summary_finish() -> String {
    SUMMING.store(false, Ordering::Relaxed);
    let Some(sum) = summary().take() else {
        return String::new();
    };
    let ticks = sum.ticks.max(1) as f64;
    let mut out = format!(
        "ticks {} mean_wall_ms {:.2}\n",
        sum.ticks,
        sum.wall_ms / ticks
    );
    let mut rows: Vec<_> = sum.spans.into_iter().collect();
    rows.sort_by(|a, b| b.1.0.total_cmp(&a.1.0));
    for (name, (ms, cpu)) in rows {
        out += &format!("{name} wall {:.2} cpu {:.2}\n", ms / ticks, cpu / ticks);
    }
    out
}

/// The frame's spans, summed by name, slowest first; clears them.
pub fn take() -> Vec<Span> {
    let spans = SPANS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let mut sums: Vec<Span> = Vec::new();
    for span in spans {
        match sums.iter_mut().find(|s| s.name == span.name) {
            Some(sum) => {
                sum.ms += span.ms;
                sum.cpu_ms += span.cpu_ms;
            }
            None => sums.push(span),
        }
    }
    sums.sort_by(|a, b| b.ms.total_cmp(&a.ms));
    sums
}
/// `name wall` for spans of at least 0.5 ms, with `/cpu` when the thread
/// worked for less than nine tenths of it: the slow-frame and slow-tick lines.
pub fn describe(spans: &[Span]) -> String {
    let list: Vec<String> = spans
        .iter()
        .filter(|s| s.ms >= 0.5)
        .map(|s| {
            if s.cpu_ms < s.ms * 0.9 {
                format!("{} {:.1}/{:.1}", s.name, s.ms, s.cpu_ms)
            } else {
                format!("{} {:.1}", s.name, s.ms)
            }
        })
        .collect();
    list.join(", ")
}

#[cfg(test)]
mod tests {
    use super::Span;

    /// Spans of one name add up; a span the thread spent mostly waiting
    /// shows its CPU time after the wall time, short ones are left out.
    #[test]
    fn spans_sum_by_name_and_show_waits() {
        super::SPANS.with(|s| {
            s.borrow_mut().extend([
                Span {
                    name: "tick",
                    ms: 2.0,
                    cpu_ms: 2.0,
                },
                Span {
                    name: "submit",
                    ms: 30.0,
                    cpu_ms: 0.2,
                },
                Span {
                    name: "tick",
                    ms: 3.0,
                    cpu_ms: 2.9,
                },
                Span {
                    name: "hud",
                    ms: 0.3,
                    cpu_ms: 0.3,
                },
            ]);
        });
        let spans = super::take();
        assert_eq!(
            spans[0],
            Span {
                name: "submit",
                ms: 30.0,
                cpu_ms: 0.2
            }
        );
        assert_eq!((spans[1].name, spans[1].ms), ("tick", 5.0));
        assert_eq!(super::describe(&spans), "submit 30.0/0.2, tick 5.0");
        assert!(super::take().is_empty());
    }

    /// The thread's CPU clock moves with work done on it, not with time
    /// spent asleep.
    #[cfg(any(unix, all(windows, target_arch = "x86_64")))]
    #[test]
    fn thread_cpu_counts_work_not_sleep() {
        let start = super::thread_cpu().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let slept = super::thread_cpu().unwrap() - start;
        // Busy until the clock shows 10 ms of work (a loaded machine gives
        // the thread less than its wall time).
        let mut x = 0u64;
        let busy = std::time::Instant::now();
        let worked = loop {
            x = std::hint::black_box(x.wrapping_mul(31).wrapping_add(7));
            let worked = super::thread_cpu().unwrap() - start - slept;
            if worked.as_millis() >= 10 || busy.elapsed().as_secs() >= 10 {
                break worked;
            }
        };
        assert!(slept.as_millis() < 25, "{slept:?} while asleep");
        assert!(worked.as_millis() >= 10, "{worked:?} while busy");
    }
}
