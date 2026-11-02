//! A serial field lends its actual value to a regional phase, never a copy.
use std::ops::{Deref, DerefMut};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnershipError {
    LentToRegion,
    AlreadyResident,
}
impl std::fmt::Display for OwnershipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::LentToRegion => {
                "canonical state is lent to an active region; access its live adapter"
            }
            Self::AlreadyResident => "cannot replace canonical state that is already resident",
        })
    }
}
impl std::error::Error for OwnershipError {}

/// No `Default` or `Clone`: an absent owner is an error, not empty state.
pub struct OwnershipSlot<T>(Option<T>);
impl<T> OwnershipSlot<T> {
    pub fn new(value: T) -> Self {
        Self(Some(value))
    }
    #[inline]
    pub fn as_ref(&self) -> Result<&T, OwnershipError> {
        self.0.as_ref().ok_or(OwnershipError::LentToRegion)
    }
    #[inline]
    pub fn as_mut(&mut self) -> Result<&mut T, OwnershipError> {
        self.0.as_mut().ok_or(OwnershipError::LentToRegion)
    }
    pub fn take(&mut self) -> Result<T, OwnershipError> {
        self.0.take().ok_or(OwnershipError::LentToRegion)
    }
    /// A failed restore returns its incoming value and retains current state.
    pub fn restore(&mut self, value: T) -> Result<(), (OwnershipError, T)> {
        if self.0.is_some() {
            return Err((OwnershipError::AlreadyResident, value));
        }
        self.0 = Some(value);
        Ok(())
    }
}
impl<T> From<T> for OwnershipSlot<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}
impl<T> Deref for OwnershipSlot<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        self.as_ref().unwrap_or_else(|error| panic!("{error}"))
    }
}
impl<T> DerefMut for OwnershipSlot<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.as_mut().unwrap_or_else(|error| panic!("{error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lending_keeps_live_mutations_and_refuses_a_stale_restore() {
        let mut slot = OwnershipSlot::new(vec![1]);
        let mut region = slot.take().unwrap();
        assert_eq!(slot.as_ref(), Err(OwnershipError::LentToRegion));
        assert_eq!(slot.take(), Err(OwnershipError::LentToRegion));
        region.push(2);
        slot.restore(region).unwrap();
        slot.push(3);
        let (error, stale) = slot.restore(vec![99]).unwrap_err();
        assert_eq!(error, OwnershipError::AlreadyResident);
        assert_eq!(stale, [99]);
        assert_eq!(slot.as_ref().unwrap(), &[1, 2, 3]);
    }
}
