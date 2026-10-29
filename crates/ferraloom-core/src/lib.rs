//! Server execution policies, independent of Horizon's Game and client libraries.
use std::{num::NonZeroUsize, time::Duration};

/// Per-tick limits on publishing asynchronous chunk work into a live region.
#[derive(Clone, Copy, Debug)]
pub struct IntegrationLimits {
    pub time: Duration,
    pub columns: NonZeroUsize,
}

/// A region keeps one budget across every streaming pump in the same tick.
/// Atomic completions are never interrupted. One completion may exceed the
/// time limit; allowing that first operation guarantees forward progress.
#[derive(Default)]
pub struct IntegrationBudget {
    limits: Option<IntegrationLimits>,
    spent: Duration,
    completed: usize,
    columns: usize,
}
impl IntegrationBudget {
    pub fn new(limits: Option<IntegrationLimits>) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }
    pub fn can_start(&self, elapsed: Duration, column: bool) -> bool {
        self.limits.is_none_or(|limits| {
            self.completed == 0
                || (self.spent.saturating_add(elapsed) < limits.time
                    && (!column || self.columns < limits.columns.get()))
        })
    }
    pub fn complete(&mut self, column: bool) {
        if self.limits.is_some() {
            self.completed += 1;
            self.columns += usize::from(column);
        }
    }
    pub fn charge(&mut self, elapsed: Duration) {
        if self.limits.is_some() {
            self.spent = self.spent.saturating_add(elapsed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn budget() -> IntegrationBudget {
        IntegrationBudget::new(Some(IntegrationLimits {
            time: Duration::from_millis(2),
            columns: NonZeroUsize::new(2).unwrap(),
        }))
    }
    #[test]
    fn repeated_pumps_share_the_tick_time_and_allow_one_overrun() {
        let mut b = budget();
        assert!(b.can_start(Duration::from_millis(9), true));
        b.complete(true);
        b.charge(Duration::from_millis(9));
        assert!(!b.can_start(Duration::ZERO, false));
        assert!(budget().can_start(Duration::ZERO, true));
    }
    #[test]
    fn column_limit_preserves_a_pending_prefix_and_metadata_progress() {
        let mut b = budget();
        for _ in 0..2 {
            assert!(b.can_start(Duration::ZERO, true));
            b.complete(true);
        }
        assert!(!b.can_start(Duration::ZERO, true));
        assert!(b.can_start(Duration::ZERO, false));
        b.complete(false);
        b.charge(Duration::from_millis(1));
        assert!(!b.can_start(Duration::from_millis(1), false));
    }
    #[test]
    fn disabled_budget_is_the_unbounded_reference() {
        let mut b = IntegrationBudget::default();
        b.complete(true);
        b.charge(Duration::MAX);
        assert!(b.can_start(Duration::MAX, true));
    }
}
