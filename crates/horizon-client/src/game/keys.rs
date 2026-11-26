//! The keys the game's logic reads from `Game::keys`: what a guest's input
//! stands for while it acts (`enter_guest` holds sneak for a sneaking guest)
//! and the movement keys of the window's player, which the window's keyboard
//! once filled in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum KeyCode {
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    KeyW,
    KeyA,
    KeyS,
    KeyD,
    Space,
}
