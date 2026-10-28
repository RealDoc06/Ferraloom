//! Mob-owned controls with short live borrows across virtual callbacks.
mod live;
pub mod runtime;
mod state;
pub use live::{tick_body, tick_jump, tick_look, tick_move};
pub use state::{
    BodyRotationControl, JumpControl, LookControl, MoveControl, Operation, rotate_towards, rotlerp,
};
#[cfg(test)]
mod tests;
