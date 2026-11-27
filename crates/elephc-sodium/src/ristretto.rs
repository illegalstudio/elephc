//! Purpose:
//! C ABI for libsodium's ristretto255 group and scalar helpers.
//!
//! Called from:
//! - The injected `sodium` PHP prelude through `extern "elephc_sodium"`.
//!
//! Key details:
//! - Points are canonical 32-byte ristretto encodings; scalars are canonical
//!   32-byte values below the group order except for `_scalar_reduce`, which
//!   accepts 64 non-reduced bytes.
//! - Scalar multiplication rejects the identity element, matching libsodium's
//!   `crypto_scalarmult_ristretto255` error path.

use std::ptr;

use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::Identity;

use crate::{bytes, guarded, publish};

/// Parses a canonical ristretto point.
fn point(bytes: &[u8]) -> Option<RistrettoPoint> {
    let encoded: [u8; 32] = bytes.try_into().ok()?;
    CompressedRistretto(encoded).decompress()
}

/// Parses a 32-byte scalar, reducing non-canonical values modulo the group order.
fn scalar(bytes: &[u8]) -> Option<Scalar> {
    let encoded: [u8; 32] = bytes.try_into().ok()?;
    Some(Scalar::from_bytes_mod_order(encoded))
}

/// Publishes a compressed point.
fn publish_point(point: &RistrettoPoint) -> *const u8 {
    publish(&point.compress().to_bytes())
}

/// C ABI: maps 64 uniform bytes to a ristretto point.
///
/// # Safety
/// `input` must reference `input_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_from_hash(
    input: *const u8,
    input_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(input) = bytes(input, input_len) else {
            return ptr::null();
        };
        if input.len() != 64 {
            return ptr::null();
        }
        let wide: [u8; 64] = input.try_into().expect("hash length");
        publish_point(&RistrettoPoint::from_uniform_bytes(&wide))
    })
}

/// C ABI: reports whether a 32-byte encoding is a valid ristretto point.
///
/// # Safety
/// `input` must reference `input_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_is_valid_point(
    input: *const u8,
    input_len: usize,
) -> i32 {
    guarded(0, || {
        let Some(input) = bytes(input, input_len) else {
            return 0;
        };
        i32::from(input.len() == 32 && point(input).is_some())
    })
}

/// C ABI: ristretto point addition.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_add(
    left: *const u8,
    left_len: usize,
    right: *const u8,
    right_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(left), Some(right)) = (bytes(left, left_len), bytes(right, right_len)) else {
            return ptr::null();
        };
        let (Some(left), Some(right)) = (point(left), point(right)) else {
            return ptr::null();
        };
        publish_point(&(left + right))
    })
}

/// C ABI: ristretto point subtraction.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_sub(
    left: *const u8,
    left_len: usize,
    right: *const u8,
    right_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(left), Some(right)) = (bytes(left, left_len), bytes(right, right_len)) else {
            return ptr::null();
        };
        let (Some(left), Some(right)) = (point(left), point(right)) else {
            return ptr::null();
        };
        publish_point(&(left - right))
    })
}

/// C ABI: reduces 64 non-reduced bytes to a canonical scalar.
///
/// # Safety
/// `input` must reference `input_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_scalar_reduce(
    input: *const u8,
    input_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(input) = bytes(input, input_len) else {
            return ptr::null();
        };
        if input.len() != 64 {
            return ptr::null();
        }
        let wide: [u8; 64] = input.try_into().expect("scalar length");
        publish(&Scalar::from_bytes_mod_order_wide(&wide).to_bytes())
    })
}

/// C ABI: canonical scalar addition.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_scalar_add(
    left: *const u8,
    left_len: usize,
    right: *const u8,
    right_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(left), Some(right)) = (bytes(left, left_len), bytes(right, right_len)) else {
            return ptr::null();
        };
        let (Some(left), Some(right)) = (scalar(left), scalar(right)) else {
            return ptr::null();
        };
        publish(&(left + right).to_bytes())
    })
}

/// C ABI: canonical scalar subtraction.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_scalar_sub(
    left: *const u8,
    left_len: usize,
    right: *const u8,
    right_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(left), Some(right)) = (bytes(left, left_len), bytes(right, right_len)) else {
            return ptr::null();
        };
        let (Some(left), Some(right)) = (scalar(left), scalar(right)) else {
            return ptr::null();
        };
        publish(&(left - right).to_bytes())
    })
}

/// C ABI: canonical scalar multiplication.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_scalar_mul(
    left: *const u8,
    left_len: usize,
    right: *const u8,
    right_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(left), Some(right)) = (bytes(left, left_len), bytes(right, right_len)) else {
            return ptr::null();
        };
        let (Some(left), Some(right)) = (scalar(left), scalar(right)) else {
            return ptr::null();
        };
        publish(&(left * right).to_bytes())
    })
}

/// C ABI: additive inverse of a canonical scalar.
///
/// # Safety
/// `input` must reference `input_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_scalar_negate(
    input: *const u8,
    input_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(input) = bytes(input, input_len) else {
            return ptr::null();
        };
        let Some(value) = scalar(input) else {
            return ptr::null();
        };
        publish(&(-value).to_bytes())
    })
}

/// C ABI: additive complement `1 - s` of a canonical scalar.
///
/// # Safety
/// `input` must reference `input_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_scalar_complement(
    input: *const u8,
    input_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(input) = bytes(input, input_len) else {
            return ptr::null();
        };
        let Some(value) = scalar(input) else {
            return ptr::null();
        };
        publish(&(Scalar::ONE - value).to_bytes())
    })
}

/// C ABI: multiplicative inverse of a non-zero canonical scalar.
///
/// # Safety
/// `input` must reference `input_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_scalar_invert(
    input: *const u8,
    input_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(input) = bytes(input, input_len) else {
            return ptr::null();
        };
        let Some(value) = scalar(input) else {
            return ptr::null();
        };
        if value == Scalar::ZERO {
            return ptr::null();
        }
        publish(&value.invert().to_bytes())
    })
}

/// C ABI: ristretto scalar multiplication `n * p`.
///
/// # Safety
/// Both pointers must reference their declared readable lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_scalarmult(
    scalar_bytes: *const u8,
    scalar_len: usize,
    point_bytes: *const u8,
    point_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let (Some(scalar_bytes), Some(point_bytes)) = (
            bytes(scalar_bytes, scalar_len),
            bytes(point_bytes, point_len),
        ) else {
            return ptr::null();
        };
        let (Some(value), Some(point)) = (scalar(scalar_bytes), point(point_bytes)) else {
            return ptr::null();
        };
        let result = point * value;
        if result == RistrettoPoint::identity() {
            return ptr::null();
        }
        publish_point(&result)
    })
}

/// C ABI: ristretto base-point multiplication.
///
/// # Safety
/// `scalar_bytes` must reference `scalar_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_sodium_ristretto255_scalarmult_base(
    scalar_bytes: *const u8,
    scalar_len: usize,
) -> *const u8 {
    guarded(ptr::null(), || {
        let Some(scalar_bytes) = bytes(scalar_bytes, scalar_len) else {
            return ptr::null();
        };
        let Some(value) = scalar(scalar_bytes) else {
            return ptr::null();
        };
        let result = RistrettoPoint::mul_base(&value);
        if result == RistrettoPoint::identity() {
            return ptr::null();
        }
        publish_point(&result)
    })
}
