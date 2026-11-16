//! Purpose:
//! Exports the panic-contained C ABI the injected GMP prelude calls through
//! `extern "elephc_gmp"`.
//!
//! Called from:
//! - Compiled PHP programs through the prelude's extern block.
//! - `cargo test -p elephc-gmp` with `ELEPHC_GMP_LIB_DIR` pointing at a built libgmp.
//!
//! Key details:
//! - Every entry point returns a packet in a per-thread `CString` cell, or a plain
//!   integer/double where the PHP type has a direct C shape.
//! - Panics never cross the FFI boundary: they become an internal-error packet.

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::ffi::*;
use crate::mpz::Mpz;
use crate::ops;
use crate::packet::*;

thread_local! {
    /// Packet returned by the most recent string-returning call on this thread.
    static PACKET: RefCell<CString> = RefCell::new(CString::default());
    /// PHP's process-global GMP random state, kept per thread for test isolation.
    static RANDOM: RefCell<Option<Box<RandomState>>> = const { RefCell::new(None) };
}

/// Owns one initialized libgmp random state.
struct RandomState {
    raw: RandStateRaw,
}

impl RandomState {
    /// Initializes libgmp's Mersenne Twister generator and seeds it once.
    fn new() -> Self {
        let mut raw = RandStateRaw {
            seed: MpzRaw { alloc: 0, size: 0, data: std::ptr::null_mut() },
            algorithm: 0,
            padding: 0,
            algorithm_data: std::ptr::null_mut(),
        };
        unsafe { __gmp_randinit_mt(&mut raw) };
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        let seed = Mpz::from_i64(seed as i64);
        unsafe { __gmp_randseed(&mut raw, seed.as_ptr()) };
        Self { raw }
    }
}

impl Drop for RandomState {
    /// Releases libgmp's generator state.
    fn drop(&mut self) {
        unsafe { __gmp_randclear(&mut self.raw) };
    }
}

/// Copies a packet into this thread's cell and returns its pointer.
fn stash(packet: String) -> *const c_char {
    let value = CString::new(packet.replace('\0', "?")).unwrap_or_default();
    PACKET.with(|cell| {
        let mut cell = cell.borrow_mut();
        *cell = value;
        cell.as_ptr()
    })
}

/// Runs one packet-producing operation with panic containment.
fn contained(operation: impl FnOnce() -> String) -> *const c_char {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(packet) => stash(packet),
        Err(_) => stash(internal_error("internal GMP panic")),
    }
}

/// Reads a borrowed NUL-terminated argument as a Rust string.
unsafe fn text<'a>(pointer: *const c_char) -> &'a str {
    if pointer.is_null() {
        ""
    } else {
        CStr::from_ptr(pointer).to_str().unwrap_or("")
    }
}

/// Evaluates one binary operation and returns an outcome packet.
///
/// # Safety
/// `left` and `right` must be NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_binary(
    op: c_int,
    left: *const c_char,
    right: *const c_char,
) -> *const c_char {
    let left = text(left).to_string();
    let right = text(right).to_string();
    contained(move || ops::binary(op, &left, &right))
}

/// Evaluates one unary operation with its integer parameter.
///
/// # Safety
/// `value` must be a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_unary(
    op: c_int,
    value: *const c_char,
    argument: i64,
) -> *const c_char {
    let value = text(value).to_string();
    contained(move || ops::unary(op, &value, argument))
}

/// Evaluates modular exponentiation.
///
/// # Safety
/// All three pointers must be NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_ternary(
    op: c_int,
    first: *const c_char,
    second: *const c_char,
    third: *const c_char,
) -> *const c_char {
    let first = text(first).to_string();
    let second = text(second).to_string();
    let third = text(third).to_string();
    contained(move || match op {
        ops::TER_POWM => ops::powm(&first, &second, &third),
        _ => internal_error(format!("unknown GMP ternary operation {op}")),
    })
}

/// Sets or clears one bit.
///
/// # Safety
/// `value` must be a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_bit(
    value: *const c_char,
    index: i64,
    set: c_int,
) -> *const c_char {
    let value = text(value).to_string();
    contained(move || ops::bit(&value, index, set != 0))
}

/// Parses a PHP integer string with an explicit base and returns canonical decimal.
///
/// # Safety
/// `value` must be a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_parse(value: *const c_char, base: i64) -> *const c_char {
    let value = text(value).to_string();
    contained(move || match Mpz::parse_php(&value, base) {
        Ok(parsed) => ok(parsed.to_decimal()),
        Err(message) => value_error(message),
    })
}

/// Formats a canonical decimal value in one GMP base.
///
/// # Safety
/// `value` must be a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_strval(value: *const c_char, base: i64) -> *const c_char {
    let value = text(value).to_string();
    contained(move || match Mpz::from_decimal(&value).and_then(|value| value.to_base(base)) {
        Ok(formatted) => ok(formatted),
        Err(message) => value_error(message),
    })
}

/// Returns PHP's signed machine word for a canonical decimal value.
///
/// # Safety
/// `value` must be a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_intval(value: *const c_char) -> i64 {
    let value = text(value);
    catch_unwind(|| Mpz::from_decimal(value).map(|value| value.to_i64()).unwrap_or(0))
        .unwrap_or(0)
}

/// Returns libgmp's double for a canonical decimal value.
///
/// # Safety
/// `value` must be a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_double(value: *const c_char) -> f64 {
    let value = text(value);
    catch_unwind(|| Mpz::from_decimal(value).map(|value| value.to_f64()).unwrap_or(0.0))
        .unwrap_or(0.0)
}

/// Reports whether a canonical decimal value fits PHP's signed machine word.
///
/// Every supported target uses a 64-bit `long`, matching the bridge and the compiled
/// program's ABI. A future 32-bit target must pass an explicit word size instead of
/// relying on libgmp's build-time `long`.
///
/// # Safety
/// `value` must be a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_fits_slong(value: *const c_char) -> c_int {
    let value = text(value);
    catch_unwind(|| {
        Mpz::from_decimal(value)
            .map(|value| unsafe { __gmpz_fits_slong_p(value.as_ptr()) })
            .unwrap_or(0)
    })
    .unwrap_or(0)
}

/// Imports a hex-encoded byte payload.
///
/// # Safety
/// `hex` must be a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_import(
    hex: *const c_char,
    word_size: i64,
    order: c_int,
    endian: c_int,
) -> *const c_char {
    let hex = text(hex).to_string();
    contained(move || {
        if word_size < 1 {
            return value_error("must be greater than or equal to 1");
        }
        match crate::mpz::hex_to_bytes(&hex) {
            Ok(bytes) => ops::import(&bytes, word_size as usize, order, endian),
            Err(message) => internal_error(message),
        }
    })
}

/// Exports a value as a hex-encoded byte payload.
///
/// # Safety
/// `value` must be a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_export(
    value: *const c_char,
    word_size: i64,
    order: c_int,
    endian: c_int,
) -> *const c_char {
    let value = text(value).to_string();
    contained(move || {
        if word_size < 1 {
            return value_error("must be greater than or equal to 1");
        }
        ops::export(&value, word_size as usize, order, endian)
    })
}

/// Seeds PHP's GMP random state.
///
/// # Safety
/// `seed` must be a NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_random_seed(seed: *const c_char) -> *const c_char {
    let seed = text(seed).to_string();
    contained(move || {
        let parsed = match Mpz::from_decimal(&seed) {
            Ok(value) => value,
            Err(message) => return value_error(message),
        };
        RANDOM.with(|state| {
            let mut state = state.borrow_mut();
            let state = state.get_or_insert_with(|| Box::new(RandomState::new()));
            unsafe { __gmp_randseed(&mut state.raw, parsed.as_ptr()) };
        });
        ok("")
    })
}

/// Returns a random number with `bits` bits.
#[no_mangle]
pub extern "C" fn elephc_gmp_random_bits(bits: i64) -> *const c_char {
    contained(move || {
        use std::os::raw::c_ulong;
        if bits <= 0 {
            return value_error("must be between 1 and 2147483647");
        }
        RANDOM.with(|state| {
            let mut state = state.borrow_mut();
            let state = state.get_or_insert_with(|| Box::new(RandomState::new()));
            let mut result = Mpz::new();
            unsafe {
                __gmpz_urandomb(result.as_mut_ptr(), &mut state.raw, bits as c_ulong);
            }
            ok(result.to_decimal())
        })
    })
}

/// Returns a random number in `[min, max)`.
///
/// # Safety
/// Both pointers must be NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn elephc_gmp_random_range(
    minimum: *const c_char,
    maximum: *const c_char,
) -> *const c_char {
    let minimum = text(minimum).to_string();
    let maximum = text(maximum).to_string();
    contained(move || {
        let minimum = match Mpz::from_decimal(&minimum) {
            Ok(value) => value,
            Err(message) => return value_error(message),
        };
        let maximum = match Mpz::from_decimal(&maximum) {
            Ok(value) => value,
            Err(message) => return value_error(message),
        };
        let mut range = Mpz::new();
        unsafe {
            __gmpz_sub(range.as_mut_ptr(), maximum.as_ptr(), minimum.as_ptr());
            __gmpz_add_ui(range.as_mut_ptr(), range.as_ptr(), 1);
        }
        RANDOM.with(|state| {
            let mut state = state.borrow_mut();
            let state = state.get_or_insert_with(|| Box::new(RandomState::new()));
            let mut result = Mpz::new();
            unsafe {
                __gmpz_urandomm(result.as_mut_ptr(), &mut state.raw, range.as_ptr());
                __gmpz_add(result.as_mut_ptr(), result.as_ptr(), minimum.as_ptr());
            }
            ok(result.to_decimal())
        })
    })
}

/// Reports the linked libgmp version string.
#[no_mangle]
pub extern "C" fn elephc_gmp_version() -> *const c_char {
    unsafe { __gmp_version }
}
