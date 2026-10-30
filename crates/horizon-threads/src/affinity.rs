//! Linux per-thread affinity, restricted to the caller's original CPU mask.
use std::{collections::BTreeSet, sync::OnceLock};

struct Plan {
    tick: usize,
    workers: Vec<usize>,
    threads: usize,
}
static PLAN: OnceLock<Plan> = OnceLock::new();

fn cpu_list(text: &str) -> Option<BTreeSet<usize>> {
    let mut cpus = BTreeSet::new();
    for part in text.trim().split(',') {
        let (lo, hi) = part.split_once('-').unwrap_or((part, part));
        let (lo, hi) = (lo.parse::<usize>().ok()?, hi.parse::<usize>().ok()?);
        if hi < lo || hi >= libc::CPU_SETSIZE as usize {
            return None;
        }
        cpus.extend(lo..=hi);
    }
    Some(cpus)
}
fn plan(allowed: &[usize], siblings: &BTreeSet<usize>, parallelism: usize) -> Option<Plan> {
    let tick = *allowed.first()?;
    if !siblings.contains(&tick) {
        return None;
    }
    let workers: Vec<_> = allowed
        .iter()
        .copied()
        .filter(|c| !siblings.contains(c))
        .collect();
    let threads = workers.len().min(parallelism.saturating_sub(1));
    (threads > 0).then_some(Plan {
        tick,
        workers,
        threads,
    })
}
fn apply(cpus: &[usize]) -> bool {
    // SAFETY: cpu_set_t is a plain bit mask; every CPU is checked before CPU_SET.
    let mut mask: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    for &cpu in cpus {
        if cpu >= libc::CPU_SETSIZE as usize {
            return false;
        }
        // SAFETY: cpu is within this initialized mask's bounds.
        unsafe { libc::CPU_SET(cpu, &mut mask) };
    }
    // SAFETY: pid 0 changes only this calling thread; mask has the supplied size.
    unsafe { libc::sched_setaffinity(0, std::mem::size_of_val(&mask), &mask) == 0 }
}
/// Capture the taskset/cgroup intersection before workers start. No CPU mask
/// changes until each thread selects its lane. Single-core or single-CPU-quota
/// runs keep their original mask rather than stranding workers on an empty lane.
pub fn configure_server_affinity() -> bool {
    if PLAN.get().is_some() {
        return true;
    }
    // SAFETY: a zeroed cpu_set_t is valid, and the syscall fills this sized buffer.
    let mut mask: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    if unsafe { libc::sched_getaffinity(0, std::mem::size_of_val(&mask), &mut mask) } != 0 {
        return false;
    }
    let allowed: Vec<_> = (0..libc::CPU_SETSIZE as usize)
        // SAFETY: every index is within the initialized CPU mask.
        .filter(|&cpu| unsafe { libc::CPU_ISSET(cpu, &mask) })
        .collect();
    let Some(&tick) = allowed.first() else {
        return false;
    };
    let Ok(text) = std::fs::read_to_string(format!(
        "/sys/devices/system/cpu/cpu{tick}/topology/thread_siblings_list"
    )) else {
        return false;
    };
    let Some(siblings) = cpu_list(&text) else {
        return false;
    };
    let parallelism = std::thread::available_parallelism().map_or(1, |n| n.get());
    let Some(plan) = plan(&allowed, &siblings, parallelism) else {
        return false;
    };
    PLAN.set(plan).is_ok()
}
pub fn tick_affinity() -> bool {
    PLAN.get().is_some_and(|p| apply(&[p.tick]))
}
pub fn worker_affinity() -> bool {
    PLAN.get().is_some_and(|p| apply(&p.workers))
}
pub fn worker_threads() -> Option<usize> {
    PLAN.get().map(|p| p.threads)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reserve_a_physical_core_without_expanding_the_invocation_mask() {
        let siblings = cpu_list("6,16\n").unwrap();
        let p = plan(&[6, 7, 8, 9, 16, 17, 18, 19], &siblings, 8).unwrap();
        assert_eq!(p.tick, 6);
        assert_eq!(p.workers, [7, 8, 9, 17, 18, 19]);
        assert_eq!(p.threads, 6);
        let cpus = [6, 7, 8, 9, 16, 17, 18, 19];
        assert_eq!(plan(&cpus, &siblings, 4).unwrap().threads, 3);
        assert!(plan(&cpus, &siblings, 1).is_none());
        assert_eq!(plan(&[6, 7], &siblings, 2).unwrap().workers, [7]);
        assert!(plan(&[6, 16], &siblings, 2).is_none());
        assert!(cpu_list("9-6").is_none());
        assert_eq!(cpu_list("6-9,16-19").unwrap().len(), 8);
    }
}
