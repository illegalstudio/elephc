//! Purpose:
//! A small shared float buffer for marshalling f64 matrices across the int-only
//! extern ABI. The affine transform (6 elements) and `imageconvolution` (a 3×3
//! kernel, 9 elements) push their floating-point values one at a time as 16.16
//! fixed-point integers, then the consuming bridge call reads them back as f64.
//!
//! Called from:
//! - The elephc image prelude (`src/image_prelude.rs`) via `extern "elephc_image"`,
//!   behind `imageaffine` and `imageconvolution` (which call
//!   `elephc_img_fbuf_reset` then `elephc_img_fbuf_push` per matrix element).
//! - `crate::transform` and `crate::filter`, which call [`values`] to read the
//!   pushed matrix when applying the operation.
//!
//! Key details:
//! - Values cross the boundary as 16.16 fixed-point (`round(v * 65536)`), so both
//!   sub-unit coefficients (rotation/scale) and pixel translations round-trip with
//!   ample precision without ever passing an `f64` through the extern ABI.
//! - elephc programs are single-threaded and each use is a synchronous
//!   reset → push×N → consume sequence, so the buffer needs no per-call identity.
//!   It is per-thread all the same, so that two threads describing a matrix at once
//!   cannot interleave their pushes into one another's.

use std::cell::RefCell;
use std::thread::LocalKey;

use crate::ffi_guard;

/// Scale factor for the 16.16 fixed-point values pushed across the ABI.
const FIXED_ONE: f64 = 65536.0;

/// Per-thread buffer holding the most recently pushed matrix, in row-major order.
fn fbuf_cell() -> &'static LocalKey<RefCell<Vec<f64>>> {
    thread_local! {
        static FBUF: RefCell<Vec<f64>> = const { RefCell::new(Vec::new()) };
    }
    &FBUF
}

/// Returns a copy of the buffered matrix values (in push order) for the consuming
/// bridge call to apply.
pub(crate) fn values() -> Vec<f64> {
    fbuf_cell().with(|slot| slot.borrow().clone())
}

/// Clears the float buffer before a new matrix is described.
#[no_mangle]
pub extern "C" fn elephc_img_fbuf_reset() {
    ffi_guard((), move || {
        fbuf_cell().with(|slot| slot.borrow_mut().clear());
    })
}

/// Appends one matrix element, decoding it from 16.16 fixed-point back to f64.
#[no_mangle]
pub extern "C" fn elephc_img_fbuf_push(fixed16: i64) {
    ffi_guard((), move || {
        fbuf_cell().with(|slot| slot.borrow_mut().push(fixed16 as f64 / FIXED_ONE));
    })
}
