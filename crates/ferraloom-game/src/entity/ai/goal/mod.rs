//! Native goal arbitration. Goal callbacks reborrow canonical state through the
//! runtime, so no selector borrow spans a canUse/start/stop/tick callback.
use super::AiOperation;
use crate::entity::EntityRef;

/// Port of Goal.Flag, in EnumSet/EnumMap encounter order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum Flag {
    Move,
    Look,
    Jump,
    Target,
}
impl Flag {
    pub const ALL: [Self; 4] = [Self::Move, Self::Look, Self::Jump, Self::Target];
}
/// Port of Goal#getFlags and Goal#setFlags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Flags(u8);
impl Flags {
    pub fn insert(&mut self, flag: Flag) {
        self.0 |= 1 << flag as u8;
    }
    pub fn remove(&mut self, flag: Flag) {
        self.0 &= !(1 << flag as u8);
    }
    pub fn contains(self, flag: Flag) -> bool {
        self.0 & (1 << flag as u8) != 0
    }
    pub fn iter(self) -> impl Iterator<Item = Flag> {
        Flag::ALL
            .into_iter()
            .filter(move |flag| self.contains(*flag))
    }
}
/// Port of Goal object identity, local to one Mob's never-reused goal arena.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct GoalId(pub usize);
/// A goal receiver outside its owning Mob, without borrowing that Mob.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct GoalRef {
    pub owner: EntityRef,
    pub id: GoalId,
}
/// Select Mob.goalSelector or Mob.targetSelector.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SelectorKind {
    Goal,
    Target,
}

/// Port of GoalSelector#tick's live Mob entry point; the canonical adapter is pending.
pub fn tick_selector(
    entity: EntityRef,
    selector: SelectorKind,
    level: &mut crate::level::Level,
) -> Result<(), crate::level::LevelError> {
    super::pending(entity, AiOperation::TickSelector(selector), level)
}

/// Port of GoalSelector#tickRunningGoals's live Mob entry point; adapter pending.
pub fn tick_running_goals(
    entity: EntityRef,
    selector: SelectorKind,
    _force: bool,
    level: &mut crate::level::Level,
) -> Result<(), crate::level::LevelError> {
    super::pending(entity, AiOperation::TickRunningGoals(selector), level)
}
/// Port of WrappedGoal. Wrapper identity is separate from the wrapped Goal:
/// removing and re-adding a Goal creates a new wrapper, while old flag locks live.
#[derive(Debug)]
pub struct WrappedGoal {
    pub goal: GoalId,
    pub priority: i32,
    running: bool,
}
impl WrappedGoal {
    /// Port of WrappedGoal#isRunning.
    pub fn is_running(&self) -> bool {
        self.running
    }
}
/// Port of GoalSelector's insertion-ordered available goals and enum flag locks.
/// Removed wrappers remain reachable through native stale locks until cleanup.
#[derive(Debug, Default)]
pub struct GoalSelector {
    wrappers: Vec<WrappedGoal>,
    available: Vec<usize>,
    locked: [Option<usize>; 4],
    disabled: Flags,
    revision: u64,
}
impl GoalSelector {
    /// Port of GoalSelector#addGoal: equality ignores the wrapper priority.
    pub fn add_goal(&mut self, priority: i32, goal: GoalId) {
        if self
            .available
            .iter()
            .any(|&id| self.wrappers[id].goal == goal)
        {
            return;
        }
        let id = self.wrappers.len();
        self.wrappers.push(WrappedGoal {
            goal,
            priority,
            running: false,
        });
        self.available.push(id);
        self.revision = self.revision.wrapping_add(1);
    }
    /// Port of GoalSelector#getAvailableGoals.
    pub fn available_goals(&self) -> impl Iterator<Item = &WrappedGoal> {
        self.available.iter().map(|&id| &self.wrappers[id])
    }
    pub fn locked_goal(&self, flag: Flag) -> Option<GoalId> {
        self.locked[flag as usize].map(|id| self.wrappers[id].goal)
    }
    /// Port of GoalSelector#setControlFlag.
    pub fn set_control_flag(&mut self, flag: Flag, enabled: bool) {
        if enabled {
            self.disabled.remove(flag);
        } else {
            self.disabled.insert(flag);
        }
    }
}
/// Selector storage and goal virtual calls only. The live implementation owns
/// EntityRef plus &mut Level; it borrows Mob.ai only between these calls.
pub trait GoalRuntime {
    type Error: From<AiOperation>;
    fn selector(&self) -> &GoalSelector;
    fn selector_mut(&mut self) -> &mut GoalSelector;
    fn flags(&self, goal: GoalId) -> Flags;
    fn can_use(&mut self, goal: GoalId) -> Result<bool, Self::Error>;
    fn can_continue_to_use(&mut self, goal: GoalId) -> Result<bool, Self::Error> {
        self.can_use(goal)
    }
    fn is_interruptable(&mut self, _goal: GoalId) -> Result<bool, Self::Error> {
        Ok(true)
    }
    fn start(&mut self, _goal: GoalId) -> Result<(), Self::Error> {
        Ok(())
    }
    fn stop(&mut self, _goal: GoalId) -> Result<(), Self::Error> {
        Ok(())
    }
    fn requires_update_every_tick(&mut self, _goal: GoalId) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn tick(&mut self, _goal: GoalId) -> Result<(), Self::Error> {
        Ok(())
    }
}
fn next<R: GoalRuntime>(
    runtime: &R,
    revision: u64,
    index: &mut usize,
) -> Result<Option<usize>, R::Error> {
    if runtime.selector().revision != revision {
        return Err(AiOperation::GoalSelectorMutation.into());
    }
    let goal = runtime.selector().available.get(*index).copied();
    *index += 1;
    Ok(goal)
}
fn stop<R: GoalRuntime>(runtime: &mut R, id: usize) -> Result<(), R::Error> {
    let wrapper = &mut runtime.selector_mut().wrappers[id];
    if wrapper.running {
        wrapper.running = false;
        let goal = wrapper.goal;
        runtime.stop(goal)?;
    }
    Ok(())
}
fn disabled<R: GoalRuntime>(runtime: &R, id: usize) -> bool {
    runtime
        .flags(runtime.selector().wrappers[id].goal)
        .iter()
        .any(|flag| runtime.selector().disabled.contains(flag))
}
/// Port of GoalSelector#tick, including cleanup, arbitration and forced ticks.
pub fn tick<R: GoalRuntime>(runtime: &mut R) -> Result<(), R::Error> {
    let revision = runtime.selector().revision;
    let mut index = 0;
    while let Some(id) = next(runtime, revision, &mut index)? {
        let wrapper = &runtime.selector().wrappers[id];
        let (goal, running) = (wrapper.goal, wrapper.running);
        if running && (disabled(runtime, id) || !runtime.can_continue_to_use(goal)?) {
            stop(runtime, id)?;
        }
    }
    for flag in Flag::ALL {
        if let Some(id) = runtime.selector().locked[flag as usize]
            && !runtime.selector().wrappers[id].running
        {
            runtime.selector_mut().locked[flag as usize] = None;
        }
    }
    let mut index = 0;
    while let Some(id) = next(runtime, revision, &mut index)? {
        if runtime.selector().wrappers[id].running || disabled(runtime, id) {
            continue;
        }
        let goal = runtime.selector().wrappers[id].goal;
        let priority = runtime.selector().wrappers[id].priority;
        let mut replaceable = true;
        for flag in runtime.flags(goal).iter() {
            let lock = runtime.selector().locked[flag as usize];
            let (interruptable, old_priority) = if let Some(lock) = lock {
                let old = &runtime.selector().wrappers[lock];
                let (old_goal, old_priority) = (old.goal, old.priority);
                (runtime.is_interruptable(old_goal)?, old_priority)
            } else {
                (true, i32::MAX)
            };
            if !interruptable || priority >= old_priority {
                replaceable = false;
                break;
            }
        }
        if !replaceable || !runtime.can_use(goal)? {
            continue;
        }
        for flag in runtime.flags(goal).iter() {
            if let Some(old) = runtime.selector().locked[flag as usize] {
                stop(runtime, old)?;
            }
            runtime.selector_mut().locked[flag as usize] = Some(id);
        }
        if !runtime.selector().wrappers[id].running {
            runtime.selector_mut().wrappers[id].running = true;
            runtime.start(goal)?;
        }
    }
    tick_running(runtime, true)
}
/// Port of GoalSelector#tickRunningGoals. This never cleans up or selects goals.
pub fn tick_running<R: GoalRuntime>(runtime: &mut R, force: bool) -> Result<(), R::Error> {
    let revision = runtime.selector().revision;
    let mut index = 0;
    while let Some(id) = next(runtime, revision, &mut index)? {
        let wrapper = &runtime.selector().wrappers[id];
        let (goal, running) = (wrapper.goal, wrapper.running);
        if running && (force || runtime.requires_update_every_tick(goal)?) {
            runtime.tick(goal)?;
        }
    }
    Ok(())
}
/// Port of GoalSelector#removeAllGoals: the predicate runs in both traversals.
pub fn remove_all<R: GoalRuntime>(
    runtime: &mut R,
    mut predicate: impl FnMut(&mut R, GoalId) -> Result<bool, R::Error>,
) -> Result<(), R::Error> {
    let revision = runtime.selector().revision;
    let mut index = 0;
    while let Some(id) = next(runtime, revision, &mut index)? {
        let goal = runtime.selector().wrappers[id].goal;
        if predicate(runtime, goal)? {
            stop(runtime, id)?;
        }
    }
    let mut removed = Vec::new();
    let mut index = 0;
    while let Some(id) = next(runtime, revision, &mut index)? {
        let goal = runtime.selector().wrappers[id].goal;
        if predicate(runtime, goal)? {
            removed.push(id);
        }
    }
    if !removed.is_empty() {
        let selector = runtime.selector_mut();
        selector.available.retain(|id| !removed.contains(id));
        selector.revision = selector.revision.wrapping_add(1);
    }
    Ok(())
}
/// Port of Goal#reducedTickDelay and Goal#adjustedTickDelay.
pub fn adjusted_tick_delay(ticks: i32, every_tick: bool) -> i32 {
    if every_tick {
        ticks
    } else {
        ferraloom_util::mth::positive_ceil_div(ticks, 2)
    }
}
#[cfg(test)]
mod tests;
