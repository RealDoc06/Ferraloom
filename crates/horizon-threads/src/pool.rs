//! A pool of worker threads for the tick's short parallel passes over borrowed
//! data, and another for the passes that run beside the tick on a thread of
//! their own (`for_each_mut_background`), so neither holds up the other. Nothing
//! here orders the work: a pass must give the same result whichever thread
//! takes which item.
use std::{
    any::Any,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::{Condvar, Mutex, OnceLock, PoisonError, TryLockError},
};

/// A pass's closure, its lifetime erased while [`broadcast`] keeps it alive.
#[derive(Clone, Copy)]
struct Job(*const (dyn Fn() + Sync + 'static));
// SAFETY: the closure is Sync, and broadcast doesn't return before every
// thread that took the pointer has finished with it.
unsafe impl Send for Job {}

#[derive(Default)]
struct State {
    /// The current pass; workers who wake after it ended just go back to sleep.
    job: Option<Job>,
    generation: u64,
    running: usize,
    panic: Option<Box<dyn Any + Send>>,
}

struct Pool {
    state: Mutex<State>,
    wake: Condvar,
    idle: Condvar,
    /// Held for a pass: another that comes meanwhile runs on its own thread.
    pass: Mutex<()>,
}

impl Pool {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn work(&self) {
        let mut last = 0;
        loop {
            let job = {
                let mut state = self.lock();
                let job = loop {
                    match state.job {
                        Some(job) if state.generation != last => break job,
                        _ => {
                            state = self
                                .wake
                                .wait(state)
                                .unwrap_or_else(PoisonError::into_inner)
                        }
                    }
                };
                last = state.generation;
                state.running += 1;
                job
            };
            // SAFETY: `running` holds broadcast until this call has returned.
            let result = catch_unwind(AssertUnwindSafe(|| unsafe { (*job.0)() }));
            let mut state = self.lock();
            if let Err(panic) = result {
                state.panic = Some(panic);
            }
            state.running -= 1;
            if state.running == 0 {
                self.idle.notify_all();
            }
        }
    }
}

/// `threads` workers on the worker CPUs. None when no thread could start.
fn start(name: &'static str, threads: usize) -> Option<&'static Pool> {
    let pool: &'static Pool = Box::leak(Box::new(Pool {
        state: Mutex::default(),
        wake: Condvar::new(),
        idle: Condvar::new(),
        pass: Mutex::new(()),
    }));
    let started = (0..threads)
        .filter(|i| {
            std::thread::Builder::new()
                .name(format!("{name}-{i}"))
                .spawn(move || {
                    crate::worker_affinity();
                    pool.work();
                })
                .is_ok()
        })
        .count();
    (started > 0).then_some(pool)
}

/// One per worker CPU of the affinity plan, or per core besides the caller's.
fn worker_count() -> usize {
    crate::worker_threads()
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()) - 1)
}

/// The tick's workers, started the first time.
fn pool() -> Option<&'static Pool> {
    static POOL: OnceLock<Option<&'static Pool>> = OnceLock::new();
    *POOL.get_or_init(|| start("horizon-pass", worker_count()))
}

/// The workers of the passes beside the tick, started the first time: half
/// as many, as the same CPUs have the writer threads and the tick's workers.
fn background() -> Option<&'static Pool> {
    static POOL: OnceLock<Option<&'static Pool>> = OnceLock::new();
    *POOL.get_or_init(|| start("horizon-beside", worker_count() / 2))
}

/// Run `f` on this thread and on as many workers as get to it before this
/// thread's own call returns; returns when all of them have finished. A worker
/// busy elsewhere doesn't hold it up, so `f` must take its work from something
/// shared and stop when none is left (see [`for_each_mut`]).
pub fn broadcast(f: &(dyn Fn() + Sync)) {
    broadcast_on(pool(), f);
}

fn broadcast_on(pool: Option<&'static Pool>, f: &(dyn Fn() + Sync)) {
    let Some(pool) = pool else {
        return f();
    };
    let _pass = match pool.pass.try_lock() {
        Ok(pass) => pass,
        Err(TryLockError::Poisoned(e)) => e.into_inner(),
        Err(TryLockError::WouldBlock) => return f(),
    };
    let job: *const (dyn Fn() + Sync + '_) = f;
    // SAFETY: only the lifetime changes, and the wait below for every worker
    // holding the pointer comes before `f`'s borrow can end.
    let job = unsafe {
        std::mem::transmute::<*const (dyn Fn() + Sync + '_), *const (dyn Fn() + Sync + 'static)>(
            job,
        )
    };
    {
        let mut state = pool.lock();
        state.generation += 1;
        state.job = Some(Job(job));
    }
    pool.wake.notify_all();
    let result = catch_unwind(AssertUnwindSafe(f));
    let mut state = pool.lock();
    state.job = None;
    while state.running > 0 {
        state = pool
            .idle
            .wait(state)
            .unwrap_or_else(PoisonError::into_inner);
    }
    let worker = state.panic.take();
    drop(state);
    if let Err(panic) = result {
        resume_unwind(panic);
    }
    if let Some(panic) = worker {
        resume_unwind(panic);
    }
}

/// `f` over every item, on this thread and the pool's, `grain` items at a
/// time. A slice of no more than `grain` items stays on this thread.
pub fn for_each_mut<T: Send>(items: &mut [T], grain: usize, f: impl Fn(&mut T) + Sync) {
    for_each_mut_on(pool, items, grain, f);
}

/// [`for_each_mut`] for a pass on a thread beside the tick: it takes none of
/// the tick's workers away from the tick's own passes.
pub fn for_each_mut_background<T: Send>(items: &mut [T], grain: usize, f: impl Fn(&mut T) + Sync) {
    for_each_mut_on(background, items, grain, f);
}

/// [`for_each_mut`] with a state each thread of the pass makes with `init`
/// before its first item and lends `f` with each of its items (scratch
/// space, a thread-local cache kept for the pass).
pub fn for_each_mut_with<T: Send, S>(
    items: &mut [T],
    grain: usize,
    init: impl Fn() -> S + Sync,
    f: impl Fn(&mut S, &mut T) + Sync,
) {
    if items.len() <= grain {
        let mut state = init();
        items.iter_mut().for_each(|item| f(&mut state, item));
        return;
    }
    let chunks = Mutex::new(items.chunks_mut(grain));
    broadcast_on(pool(), &|| {
        let mut state = None;
        loop {
            let next = chunks.lock().unwrap_or_else(PoisonError::into_inner).next();
            let Some(chunk) = next else {
                return;
            };
            let mine = state.get_or_insert_with(&init);
            chunk.iter_mut().for_each(|item| f(mine, item));
        }
    });
}

fn for_each_mut_on<T: Send>(
    pool: fn() -> Option<&'static Pool>,
    items: &mut [T],
    grain: usize,
    f: impl Fn(&mut T) + Sync,
) {
    if items.len() <= grain {
        items.iter_mut().for_each(f);
        return;
    }
    let chunks = Mutex::new(items.chunks_mut(grain));
    broadcast_on(pool(), &|| {
        loop {
            let next = chunks.lock().unwrap_or_else(PoisonError::into_inner).next();
            let Some(chunk) = next else {
                return;
            };
            chunk.iter_mut().for_each(&f);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_is_visited_once_also_in_passes_that_overlap() {
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for len in [0, 1, 7, 8, 1000] {
                        let mut items = vec![0u32; len];
                        for_each_mut(&mut items, 8, |item| *item += 1);
                        for_each_mut_background(&mut items, 8, |item| *item += 1);
                        assert!(items.iter().all(|&item| item == 2));
                    }
                });
            }
        });
    }

    #[test]
    fn each_thread_makes_its_state_once_a_pass() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let made = AtomicUsize::new(0);
        let mut items = vec![0usize; 1000];
        for_each_mut_with(
            &mut items,
            8,
            || {
                (
                    made.fetch_add(1, Ordering::Relaxed),
                    std::thread::current().id(),
                )
            },
            |(_, thread), item| {
                assert_eq!(*thread, std::thread::current().id());
                *item += 1;
            },
        );
        assert!(items.iter().all(|&item| item == 1));
        assert!(made.load(Ordering::Relaxed) <= worker_count() + 1);
    }

    #[test]
    fn a_panic_in_a_pass_reaches_the_caller_and_the_pool_survives() {
        let mut items = vec![0u32; 1000];
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            for_each_mut(&mut items, 8, |item| {
                *item += 1;
                assert_ne!(*item, 1, "expected");
            });
        }));
        assert!(outcome.is_err());
        let mut items = vec![0u32; 1000];
        for_each_mut(&mut items, 8, |item| *item += 1);
        assert!(items.iter().all(|&item| item == 1));
    }
}
