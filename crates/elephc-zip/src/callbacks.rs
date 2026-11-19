//! Purpose:
//! Bridge-side libzip progress/cancel callback plumbing for `ZipArchive`.
//!
//! Called from:
//! - `crate::archive::Archive::close`, which registers the trampolines before `zip_close`.
//! - `crate::abi` setters used by the zip prelude when PHP registers a callback.
//!
//! Key details:
//! - The bridge never invokes PHP directly. The prelude stores a normalized callable
//!   descriptor and the address of the shared `__rt_curl_invoke_callback` adapter; the
//!   trampolines call that address with a fixed C layout.
//! - Progress callbacks are forwarded exactly as libzip emits them, including the
//!   `0.0` start and `1.0` end values. A no-op close therefore invokes no callback,
//!   matching php-src.
//! - Cancel callbacks return libzip's abort decision. A callback that throws is treated
//!   as a cancellation, matching php-src's abort-on-exception behavior for writes.

use std::cell::Cell;
use std::ffi::c_void;
use std::ptr;

use crate::archive::Archive;

/// Callback-only state that outlives the archive borrow during `zip_close`.
///
/// libzip keeps this pointer for the duration of the close; keeping `Archive` itself
/// out of the trampolines avoids materializing a second `&mut Archive` while
/// `Archive::close()` still holds the first one.
pub(crate) struct CallbackState {
    /// Progress callback installed by the prelude, if any.
    pub(crate) progress: Option<CallbackSlot>,
    /// Cancel callback installed by the prelude, if any.
    pub(crate) cancel: Option<CallbackSlot>,
    /// Progress precision registered with libzip.
    pub(crate) progress_rate: f64,
    /// Set when a bridge callback's adapter reports a PHP throw.
    pub(crate) callback_failed: Cell<bool>,
}

impl CallbackState {
    /// Returns empty callback state for a newly opened archive.
    pub(crate) fn new() -> Self {
        Self {
            progress: None,
            cancel: None,
            progress_rate: 0.0,
            callback_failed: Cell::new(false),
        }
    }
}

/// One PHP argument in the shared callback adapter's `(tag, lo, hi)` shape.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct CallArg {
    /// Runtime Mixed tag (2 = float).
    pub(crate) tag: i64,
    /// Low payload word, or an f64 bit pattern for a float.
    pub(crate) lo: i64,
    /// High payload word.
    pub(crate) hi: i64,
}

/// Fixed layout consumed by `__rt_curl_invoke_callback`.
#[repr(C)]
pub(crate) struct CallSpec {
    /// Number of entries in `argv`, after the adapter's synthetic object argument.
    pub(crate) argc: i64,
    /// Argument triples in source order.
    pub(crate) argv: *const CallArg,
    /// 0 = decode the PHP return as an integer.
    pub(crate) result_kind: i64,
    /// Unused for the integer decode path.
    pub(crate) out_buf: *mut u8,
    /// Unused for the integer decode path.
    pub(crate) out_cap: i64,
    /// Written by the adapter for string results.
    pub(crate) out_len: i64,
    /// Written by the adapter: 0 on success, -1 when the PHP callable threw.
    pub(crate) status: i64,
}

/// The shared adapter's C ABI: `(descriptor, synthetic_object, spec) -> int`.
pub(crate) type CallbackAdapter =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut CallSpec) -> i64;

/// One registered PHP callback: both words are borrowed from the prelude's rooted property.
#[derive(Clone, Copy)]
pub(crate) struct CallbackSlot {
    /// Normalized callable descriptor.
    pub(crate) descriptor: *mut c_void,
    /// Address of `__rt_curl_invoke_callback`.
    pub(crate) adapter: *const c_void,
}

/// Invokes one bridge-owned callback slot with the supplied float/no arguments.
fn invoke(slot: CallbackSlot, args: &[CallArg]) -> (i64, i64) {
    if slot.descriptor.is_null() || slot.adapter.is_null() {
        return (0, 0);
    }
    let mut spec = CallSpec {
        argc: args.len() as i64,
        argv: args.as_ptr(),
        result_kind: 0,
        out_buf: ptr::null_mut(),
        out_cap: 0,
        out_len: 0,
        status: 0,
    };
    // SAFETY: the prelude passes the address returned by the internal adapter builtin,
    // and its only possible implementation has this exact C signature.
    let adapter: CallbackAdapter = unsafe { std::mem::transmute(slot.adapter) };
    let result = unsafe { adapter(slot.descriptor, ptr::null_mut(), &mut spec) };
    (result, spec.status)
}

/// libzip progress trampoline: forwards every libzip progress value to PHP.
pub(crate) unsafe extern "C" fn progress_trampoline(
    _za: *mut crate::ffi::zip_t,
    progress: f64,
    userdata: *mut c_void,
) {
    if userdata.is_null() {
        return;
    }
    let state = unsafe { &*(userdata.cast::<CallbackState>()) };
    if state.callback_failed.get() {
        return;
    }
    let Some(slot) = state.progress else {
        return;
    };
    let arg = CallArg {
        tag: 2,
        lo: progress.to_bits() as i64,
        hi: 0,
    };
    let (_result, status) = invoke(slot, &[arg]);
    if status < 0 {
        state.callback_failed.set(true);
    }
}

/// libzip cancel trampoline: converts the PHP callback's return into libzip's abort code.
pub(crate) unsafe extern "C" fn cancel_trampoline(
    _za: *mut crate::ffi::zip_t,
    userdata: *mut c_void,
) -> i32 {
    if userdata.is_null() {
        return 0;
    }
    let state = unsafe { &*(userdata.cast::<CallbackState>()) };
    if state.callback_failed.get() {
        return 1;
    }
    let Some(slot) = state.cancel else {
        return 0;
    };
    let (result, status) = invoke(slot, &[]);
    if status < 0 {
        state.callback_failed.set(true);
        return 1;
    }
    result as i32
}

/// Per-archive callback state used by the trampolines through a raw back-pointer.
impl Archive {
    /// Records one bridge callback slot and the progress precision, if any.
    pub(crate) fn set_progress_callback(&mut self, descriptor: *mut c_void, adapter: *const c_void, rate: f64) {
        self.callbacks.progress = if descriptor.is_null() || adapter.is_null() {
            None
        } else {
            Some(CallbackSlot { descriptor, adapter })
        };
        self.callbacks.progress_rate = rate;
    }

    /// Records the cancel callback slot, or clears it when either pointer is null.
    pub(crate) fn set_cancel_callback(&mut self, descriptor: *mut c_void, adapter: *const c_void) {
        self.callbacks.cancel = if descriptor.is_null() || adapter.is_null() {
            None
        } else {
            Some(CallbackSlot { descriptor, adapter })
        };
    }

    /// Clears the throw marker before a fresh close attempt.
    pub(crate) fn reset_callback_failed(&self) {
        self.callbacks.callback_failed.set(false);
    }

    /// Registers the configured callbacks on the live libzip handle before closing.
    pub(crate) fn register_callbacks(&mut self, za: *mut crate::ffi::zip_t) {
        let progress = self.callbacks.progress;
        let cancel = self.callbacks.cancel;
        let progress_rate = self.callbacks.progress_rate;
        let userdata = self.callbacks.as_ref() as *const CallbackState as *mut c_void;
        if progress.is_some() {
            let _ = unsafe {
                crate::ffi::zip_register_progress_callback_with_state(
                    za,
                    progress_rate,
                    Some(progress_trampoline),
                    None,
                    userdata,
                )
            };
        }
        if cancel.is_some() {
            let _ = unsafe {
                crate::ffi::zip_register_cancel_callback_with_state(
                    za,
                    Some(cancel_trampoline),
                    None,
                    userdata,
                )
            };
        }
    }
}
