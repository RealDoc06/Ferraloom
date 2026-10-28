use super::FunctionLibrary;

/// Identifies one native FunctionCommand accumulator in its execution scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectorId(u32);

/// A caller's stack boundary in the canonical function manager. Nested execution
/// may allocate more collectors without changing the outer callback identities.
#[derive(Debug)]
pub struct FunctionScope(usize);

#[derive(Debug, Default)]
struct Accumulator {
    any_result: bool,
    sum: i32,
}

/// Port of ServerFunctionManager. Definitions and FunctionCommand's temporary
/// result accumulators have one live server owner, with no callback-owned state.
#[derive(Debug, Default)]
pub struct ServerFunctionManager {
    library: FunctionLibrary,
    collectors: Vec<Option<Accumulator>>,
}
impl ServerFunctionManager {
    pub fn new(library: FunctionLibrary) -> Self {
        Self {
            library,
            collectors: Vec::new(),
        }
    }
    pub fn library(&self) -> &FunctionLibrary {
        &self.library
    }
    pub fn begin_execution(&self) -> FunctionScope {
        FunctionScope(self.collectors.len())
    }
    /// Release collectors even when frame discard or quota exhaustion skipped
    /// their final queue action. No effects are deferred until this cleanup.
    pub fn end_execution(&mut self, scope: FunctionScope) {
        self.collectors.truncate(scope.0);
    }
    pub fn create_collector(&mut self) -> CollectorId {
        // Native contexts cap queued actions below u32::MAX, and every collector
        // belongs to a call in that same finite command execution.
        let id =
            CollectorId(u32::try_from(self.collectors.len()).expect("function collector limit"));
        self.collectors.push(Some(Accumulator::default()));
        id
    }
    /// Port of FunctionCommand's Accumulator#add, invoked at the callback point.
    pub fn add_result(&mut self, id: CollectorId, value: i32) {
        let accumulator = self.collectors[id.0 as usize]
            .as_mut()
            .expect("active function result collector");
        accumulator.any_result = true;
        accumulator.sum = accumulator.sum.wrapping_add(value);
    }
    /// Native's final queue action calls the original callback only when a
    /// function returned. A failed return still contributes zero and counts.
    pub fn take_result(&mut self, id: CollectorId) -> Option<i32> {
        let accumulator = self.collectors[id.0 as usize]
            .take()
            .expect("active function result collector");
        accumulator.any_result.then_some(accumulator.sum)
    }
}
