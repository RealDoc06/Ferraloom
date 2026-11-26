//! A Game's execution role is fixed at construction.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExecutionRole {
    /// A server Game of the component harnesses (game/integrated.rs).
    IntegratedServer,
    DedicatedServer,
}
