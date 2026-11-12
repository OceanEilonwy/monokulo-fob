//! A value that knows when it last changed, so what's built from it can be
//! kept until it does (decision 412).

use std::sync::atomic::{AtomicU64, Ordering};

/// Generations, handed out once each across the process: two values with
/// the same generation are the same value.
static NEXT: AtomicU64 = AtomicU64::new(1);

/// `T` with its generation. Every mutable borrow takes a new one, so no
/// change can go unnoticed: nothing has to remember to say it changed
/// something. A borrow that changes nothing still counts; that only costs
/// a rebuild. A clone keeps its value's generation, and putting a changed
/// clone back brings the clone's new one with it.
#[derive(Clone, Debug, Default)]
pub(super) struct Tracked<T> {
    value: T,
    generation: u64,
}

impl<T> Tracked<T> {
    pub(super) fn new(value: T) -> Tracked<T> {
        Tracked { value, generation: NEXT.fetch_add(1, Ordering::Relaxed) }
    }

    /// Which version of the value this is.
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    /// Puts `value` in place of this one, as a change.
    pub(super) fn set(&mut self, value: T) {
        **self = value;
    }
}

impl<T> std::ops::Deref for Tracked<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> std::ops::DerefMut for Tracked<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.generation = NEXT.fetch_add(1, Ordering::Relaxed);
        &mut self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_is_a_new_generation_and_a_look_is_not() {
        let mut list = Tracked::new(vec![1]);
        let first = list.generation();
        assert_eq!(list.len(), 1);
        assert_eq!(list.generation(), first, "reading changes nothing");
        list.push(2);
        let second = list.generation();
        assert_ne!(second, first);
        let mut copy = list.clone();
        assert_eq!(copy.generation(), second, "a copy is the same value");
        copy.push(3);
        assert_ne!(copy.generation(), second);
        list = copy;
        assert_ne!(list.generation(), second, "a changed copy put back is a change");
    }
}
