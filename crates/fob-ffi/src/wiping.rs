//! An allocator that zeroes memory as it's freed.
//!
//! Secrets pass through memory Fob doesn't control the lifetime of: the
//! buffers UniFFI copies records into, serde's and rusqlite's temporaries,
//! a `String` that grew and left its old copy behind. Wiping every block on
//! free covers all of them at once, where `Zeroizing` only covers the values
//! someone remembered to wrap. It is the process's allocator because this
//! crate is the library the app loads (and the desktop links).

use std::alloc::{GlobalAlloc, Layout, System};

/// The system allocator, zeroing each block before handing it back.
pub struct Wiping;

/// Zeroes `len` bytes at `ptr` in a way the compiler must keep.
///
/// # Safety
/// `ptr` must be valid for writes of `len` bytes.
#[inline]
unsafe fn wipe(ptr: *mut u8, len: usize) {
    // SAFETY: the caller promises `ptr..ptr + len` is writable.
    unsafe { std::ptr::write_bytes(ptr, 0, len) };
    // Zeroing memory that is about to be freed is a dead store the
    // optimiser may delete. An empty asm block that is given the pointer and
    // isn't marked `nomem` may read that memory, so the zeroes have to be
    // there first. (A volatile loop would also hold, a byte at a time.)
    // SAFETY: the asm does nothing; it only stands for a read.
    unsafe { std::arch::asm!("/* {0} */", in(reg) ptr, options(nostack, preserves_flags)) };
}

// SAFETY: every block comes from `System` with the layout it was asked for
// and goes back to `System` with that same layout, so `System`'s guarantees
// carry over; the only addition is writing zeroes into a block while it is
// still ours, before it is freed.
unsafe impl GlobalAlloc for Wiping {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: passed on with the caller's guarantees.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: passed on with the caller's guarantees.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` is a live block of `layout.size()` bytes from this
        // allocator, so it is writable until `System` takes it back.
        unsafe {
            wipe(ptr, layout.size());
            System.dealloc(ptr, layout);
        }
    }

    // `System`'s own realloc may move the block and free the old one
    // unwiped, so growing or shrinking is a new block, a copy, and the old
    // one wiped and freed, as GlobalAlloc's default does but for the wipe.
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller promises `new_size` is non-zero and, rounded up
        // to `layout.align()`, doesn't overflow `isize`: a valid layout.
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        // SAFETY: a valid, non-zero-sized layout.
        let new = unsafe { self.alloc(new_layout) };
        if !new.is_null() {
            // SAFETY: both blocks are live and distinct, and each is at least
            // the smaller size long; the old block is then ours to free.
            unsafe {
                std::ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size));
                self.dealloc(ptr, layout);
            }
        }
        new
    }
}
