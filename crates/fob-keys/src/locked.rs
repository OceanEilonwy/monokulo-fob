//! Secrets kept in memory that isn't swapped out or written into a core dump.
//!
//! Each [`Locked`] value has pages of its own, so unlocking one never unlocks
//! another's (`munlock` works on whole pages and doesn't count). The pages are
//! locked with `mlock` and left out of core dumps where the system allows it. A
//! failure to lock — over `RLIMIT_MEMLOCK` (64 KiB by default, room for many
//! keys), or a system without it — isn't an error: the value is still wiped
//! when it goes, as it was before.
//!
//! Without the `mlock` feature the pages are neither locked nor kept out of
//! core dumps, and each value is only wiped when it goes.

use std::{
    alloc::{self, Layout},
    fmt,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use zeroize::Zeroize;

/// A `T` on pages of its own, locked into memory, wiped and unlocked when dropped.
pub struct Locked<T: Zeroize> {
    ptr: NonNull<T>,
    layout: Layout,
}

// It owns its `T` as a Box would.
unsafe impl<T: Zeroize + Send> Send for Locked<T> {}
unsafe impl<T: Zeroize + Sync> Sync for Locked<T> {}

fn page_size() -> usize {
    #[cfg(all(unix, feature = "mlock"))]
    {
        // SAFETY: sysconf only reads a system value.
        let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if size > 0 {
            return size as usize;
        }
    }
    4096
}

impl<T: Zeroize> Locked<T> {
    /// Moves `value` onto locked pages. The place it was moved from is the
    /// caller's to wipe, if it was anywhere but a register or the stack.
    pub fn new(value: T) -> Locked<T> {
        let page = page_size();
        let size = std::mem::size_of::<T>().max(1).div_ceil(page) * page;
        let layout = Layout::from_size_align(size, page.max(std::mem::align_of::<T>())).expect("a page-sized layout");
        // SAFETY: the layout's size isn't zero.
        let raw = unsafe { alloc::alloc_zeroed(layout) };
        let Some(ptr) = NonNull::new(raw as *mut T) else { alloc::handle_alloc_error(layout) };
        // SAFETY: `ptr` is fresh, page-aligned (so aligned for T) and at least size_of::<T>() long.
        unsafe { ptr.as_ptr().write(value) };
        lock(raw, size);
        Locked { ptr, layout }
    }
}

#[cfg(all(unix, feature = "mlock"))]
fn lock(raw: *mut u8, size: usize) {
    // SAFETY: `raw..raw+size` is one live allocation of ours. Failing (EPERM,
    // ENOMEM over the limit) leaves the pages as they were: still wiped on drop.
    unsafe {
        let _ = libc::mlock(raw as *const libc::c_void, size);
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let _ = libc::madvise(raw as *mut libc::c_void, size, libc::MADV_DONTDUMP);
    }
}

#[cfg(not(all(unix, feature = "mlock")))]
fn lock(_raw: *mut u8, _size: usize) {}

#[cfg(all(unix, feature = "mlock"))]
fn unlock(raw: *mut u8, size: usize) {
    // SAFETY: as in `lock`; the pages are ours alone, so no other value's lock is undone.
    unsafe {
        let _ = libc::munlock(raw as *const libc::c_void, size);
    }
}

#[cfg(not(all(unix, feature = "mlock")))]
fn unlock(_raw: *mut u8, _size: usize) {}

impl<T: Zeroize> Drop for Locked<T> {
    fn drop(&mut self) {
        let raw = self.ptr.as_ptr() as *mut u8;
        // SAFETY: `ptr` holds a live T until here, and the allocation is ours,
        // `layout.size()` long; nothing reads it after.
        unsafe {
            (*self.ptr.as_ptr()).zeroize();
            std::ptr::drop_in_place(self.ptr.as_ptr());
            std::slice::from_raw_parts_mut(raw, self.layout.size()).zeroize();
            unlock(raw, self.layout.size());
            alloc::dealloc(raw, self.layout);
        }
    }
}

impl<T: Zeroize> Deref for Locked<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: a live T for as long as self.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T: Zeroize> DerefMut for Locked<T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: a live T, borrowed through &mut self.
        unsafe { self.ptr.as_mut() }
    }
}

impl<T: Zeroize + Clone> Clone for Locked<T> {
    fn clone(&self) -> Self {
        Locked::new((**self).clone())
    }
}

impl<T: Zeroize> fmt::Debug for Locked<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Locked(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_holds_the_value_and_lets_it_change() {
        let mut key = Locked::new([7u8; 32]);
        assert_eq!(*key, [7; 32]);
        key[0] = 1;
        assert_eq!(key[0], 1);
        let copy = key.clone();
        assert_eq!(*copy, *key);
        assert_ne!(copy.ptr.as_ptr() as usize, key.ptr.as_ptr() as usize, "a copy has pages of its own");
        assert_eq!(key.ptr.as_ptr() as usize % page_size(), 0);
        drop(key);
        assert_eq!(copy[0], 1, "dropping one leaves the other");
    }

    #[test]
    fn a_value_with_its_own_heap_is_dropped_too() {
        let words = Locked::new(String::from("washing thirsty occur"));
        assert_eq!(words.as_str(), "washing thirsty occur");
        drop(words);
    }

    #[test]
    fn many_fit_and_none_fails() {
        // More than the default 64 KiB limit: past it, locking fails quietly.
        let keys: Vec<_> = (0..64u8).map(|i| Locked::new([i; 32])).collect();
        assert!(keys.iter().enumerate().all(|(i, k)| k[31] == i as u8));
    }

    #[cfg(all(feature = "mlock", any(target_os = "linux", target_os = "android")))]
    #[test]
    fn its_pages_are_locked_when_the_limit_allows() {
        fn locked_kb() -> Option<u64> {
            let status = std::fs::read_to_string("/proc/self/status").ok()?;
            let line = status.lines().find(|l| l.starts_with("VmLck:"))?;
            line.split_whitespace().nth(1)?.parse().ok()
        }
        let key = Locked::new([1u8; 32]);
        // Other tests lock and unlock at the same time, so this only checks it's counted.
        if let Some(kb) = locked_kb() {
            let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            // SAFETY: getrlimit writes the struct it's given.
            let ok = unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut limit) } == 0;
            if ok && limit.rlim_cur >= 64 * 1024 {
                assert!(kb >= 4, "VmLck {kb} kB with a key locked");
            }
        }
        assert_eq!(*key, [1; 32]);
    }
}
