//! Lending a value of the game to the plugin calls made while it is held
//! elsewhere: a guest's turn holds the hub outside `Game`, and an event
//! fired inside the turn needs it. The game has no `unsafe`; this is the
//! one safe way across.
use std::{any::TypeId, cell::RefCell};

thread_local! {
    /// The values lent on this thread, innermost last; a null while borrowed.
    static LENT: RefCell<Vec<(TypeId, *mut ())>> = const { RefCell::new(Vec::new()) };
}

/// Lends `value` to [`borrow_lent`] on this thread for the length of `f`.
/// Without plugins running, just `f`.
pub fn lend<T: 'static, R>(value: &mut T, f: impl FnOnce() -> R) -> R {
    if !crate::running() {
        return f();
    }
    lend_always(value, f)
}

fn lend_always<T: 'static, R>(value: &mut T, f: impl FnOnce() -> R) -> R {
    struct Pop;
    impl Drop for Pop {
        fn drop(&mut self) {
            LENT.with_borrow_mut(|l| l.pop());
        }
    }
    let ptr: *mut T = value;
    LENT.with_borrow_mut(|l| l.push((TypeId::of::<T>(), ptr.cast())));
    let _pop = Pop;
    f()
}

/// The innermost value of type `T` lent on this thread, for `f` only; none
/// when nothing is lent or it is borrowed already.
pub fn borrow_lent<T: 'static, R>(f: impl FnOnce(Option<&mut T>) -> R) -> R {
    let id = TypeId::of::<T>();
    let found = LENT.with_borrow_mut(|l| {
        let (i, entry) = l
            .iter_mut()
            .enumerate()
            .rev()
            .find(|(_, (t, _))| *t == id)?;
        let ptr = std::mem::replace(&mut entry.1, std::ptr::null_mut());
        (!ptr.is_null()).then_some((i, ptr))
    });
    let Some((i, ptr)) = found else {
        return f(None);
    };
    struct Restore(usize, *mut ());
    impl Drop for Restore {
        fn drop(&mut self) {
            LENT.with_borrow_mut(|l| l[self.0].1 = self.1);
        }
    }
    let restore = Restore(i, ptr);
    // SAFETY: `lend` keeps the value mutably borrowed while its entry is in
    // LENT (the entry is popped before `lend` returns), its type is `T` by
    // the TypeId, and the entry is null while this reference lives, so no
    // other reference to it exists.
    f(Some(unsafe { &mut *restore.1.cast::<T>() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lent_value_is_reachable_once_inside_the_lending_only() {
        let mut hub = 5_u32;
        assert!(borrow_lent::<u32, _>(|v| v.is_none()));
        lend_always(&mut hub, || {
            borrow_lent::<u32, _>(|v| {
                let v = v.unwrap();
                *v += 1;
                // Borrowed already: a second reach finds nothing.
                assert!(borrow_lent::<u32, _>(|w| w.is_none()));
            });
            assert!(borrow_lent::<i64, _>(|v| v.is_none()));
            let mut inner = 10_u32;
            lend_always(&mut inner, || borrow_lent::<u32, _>(|v| *v.unwrap() += 1));
            assert_eq!(inner, 11);
            borrow_lent::<u32, _>(|v| *v.unwrap() += 1);
        });
        assert_eq!(hub, 7);
        assert!(borrow_lent::<u32, _>(|v| v.is_none()));
    }
}
