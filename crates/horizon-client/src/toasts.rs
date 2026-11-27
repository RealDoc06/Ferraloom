use std::time::Instant;

/// `ToastManager.ToastInstance`: slides a toast in and out over 600 ms,
/// eased by the square, and counts how long it has been fully shown.
#[derive(Default)]
pub struct Slide {
    /// When it first updated; the times below are milliseconds since.
    pub origin: Option<Instant>,
    /// `visiblePortion`: 0 off screen, 1 fully in.
    pub portion: f32,
    /// `fullyVisibleFor`.
    pub fully_for: i64,
}

/// `SystemToast`: a yellow title over a white message (or the title alone,
/// lower down) on the system background. It stays until its display time
/// has passed since its last change, or until it is hidden.
pub struct SystemToast {
    pub slide: Slide,
    /// `ToastInstance.firstSlotIndex`: its 32-pixel slot once shown.
    pub slot: Option<usize>,
}
