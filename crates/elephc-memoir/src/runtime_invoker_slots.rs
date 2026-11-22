//! Purpose:
//! Holds stable, foreign-writable staging for a native by-reference argument.
//!
//! Called from:
//! - Magician's descriptor invoker adapters.
//!
//! Key details:
//! - The staged pair has the same offsets as the generated native writer.
//! - The caller owns and retires any PHP payload stored in the pair.

use core::{cell::UnsafeCell, marker::PhantomPinned, pin::Pin};

use crate::owner_layout::InvokerSlotWords;

/// Address-stable storage that native code can write through a reference marker.
pub struct InvokerSlot {
    words: UnsafeCell<InvokerSlotWords>,
    _pin: PhantomPinned,
}

impl InvokerSlot {
    /// Initializes both words before the caller publishes a native marker.
    pub fn pin(words: InvokerSlotWords) -> Pin<Box<Self>> {
        Box::pin(Self { words: UnsafeCell::new(words), _pin: PhantomPinned })
    }

    /// Lends the stable address to a native marker for this slot's lifetime.
    pub fn as_ptr(self: Pin<&Self>) -> *mut InvokerSlotWords {
        self.words.get()
    }

    /// Copies the current words after native control returns to the owner.
    #[allow(unsafe_code)]
    pub fn snapshot(self: Pin<&Self>) -> InvokerSlotWords {
        // This slot is !Sync, and copying these words cannot reenter its native writer.
        unsafe { self.as_ptr().read() }
    }
}

impl InvokerSlotWords {
    /// Copies a native slot whose representation contains two initialized words.
    ///
    /// # Safety
    /// `address` must point to a live, aligned two-word slot that can be read now.
    #[allow(unsafe_code)]
    pub unsafe fn read_at(address: usize) -> Self {
        unsafe { (address as *const Self).read() }
    }

    /// Stores a complete scalar pair into the caller's native slot.
    ///
    /// # Safety
    /// `address` must point to a live, aligned two-word slot writable by the caller.
    #[allow(unsafe_code)]
    pub unsafe fn write_at(self, address: usize) {
        unsafe { (address as *mut Self).write(self); }
    }

    /// Installs a complete pair and returns the previous pair's ownership to the caller.
    ///
    /// # Safety
    /// `address` must point to a live, initialized two-word slot writable by the caller.
    #[allow(unsafe_code)]
    pub unsafe fn replace_at(self, address: usize) -> Self {
        unsafe { (address as *mut Self).replace(self) }
    }
}
