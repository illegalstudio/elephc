//! Purpose:
//! Exposes the IEEE-754 bit conversions PHP's `pack()`/`unpack()` need for its
//! float and double format codes.
//!
//! Called from:
//! - The generated program's `__elephc_pack_*` C ABI calls declared by
//!   `src/pack_prelude.rs` and linked through the `elephc_pack` bridge.
//!
//! Key details:
//! - Rust owns the exact IEEE-754 conversion and rounding for `f32`; the PHP prelude owns
//!   format parsing, byte order, integer narrowing, and string padding.
//! - Every function is a total, allocation-free scalar conversion.

/// Returns the raw IEEE-754 binary64 bits of `value`.
#[no_mangle]
pub extern "C" fn elephc_pack_f64_bits(value: f64) -> u64 {
    value.to_bits()
}

/// Rebuilds a binary64 value from its raw bits.
#[no_mangle]
pub extern "C" fn elephc_pack_bits_f64(bits: u64) -> f64 {
    f64::from_bits(bits)
}

/// Rounds `value` to binary32 and returns its raw IEEE-754 bits.
#[no_mangle]
pub extern "C" fn elephc_pack_f32_bits(value: f64) -> u32 {
    (value as f32).to_bits()
}

/// Rebuilds a binary32 value from its raw bits, widened to binary64.
#[no_mangle]
pub extern "C" fn elephc_pack_bits_f32(bits: u32) -> f64 {
    f32::from_bits(bits) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips binary64 bits through the C ABI conversion.
    #[test]
    fn f64_bits_round_trip() {
        for value in [0.0, -0.0, 1.0, -1.0, 3.14, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(elephc_pack_bits_f64(elephc_pack_f64_bits(value)), value);
        }
    }

    /// Round-trips binary32 bits through the C ABI conversion.
    #[test]
    fn f32_bits_round_trip() {
        for value in [0.0, -0.0, 1.0, -1.0, 3.14, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                elephc_pack_bits_f32(elephc_pack_f32_bits(f64::from(value))),
                f64::from(value)
            );
        }
    }
}
