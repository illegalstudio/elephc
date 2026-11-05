//! Purpose:
//! Converts the target ABI's `long double` storage to and from PHP's `float`.
//!
//! Called from:
//! - `crate::place::read_float` and `crate::place::write_float`.
//!
//! Key details:
//! - Apple targets store `long double` as an 8-byte IEEE double; Linux stores the x87
//!   80-bit extended value on x86_64 and IEEE binary128 on aarch64, both in 16 bytes.
//! - Reads round to nearest, ties to even, matching C's `(double)` conversion. Writes
//!   preserve the sign of zero and canonicalize NaN payloads.

/// Reads a target `long double` at `addr` as a PHP float.
pub fn read(addr: usize, size: usize) -> f64 {
    if size <= 8 {
        return unsafe { std::ptr::read_unaligned(addr as *const f64) };
    }
    let mut bytes = [0u8; 16];
    unsafe { std::ptr::copy_nonoverlapping(addr as *const u8, bytes.as_mut_ptr(), 16) };
    if cfg!(target_arch = "x86_64") {
        read_x87(bytes)
    } else {
        read_binary128(bytes)
    }
}

/// Writes a PHP float into target `long double` storage at `addr`.
pub fn write(addr: usize, size: usize, value: f64) {
    if size <= 8 {
        unsafe { std::ptr::write_unaligned(addr as *mut f64, value) };
        return;
    }
    let bytes = if cfg!(target_arch = "x86_64") {
        write_x87(value)
    } else {
        write_binary128(value)
    };
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, 16) };
}

/// Encodes a PHP float into the target's `long double` byte representation.
pub fn encode(value: f64, size: usize) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    write(bytes.as_mut_ptr() as usize, size, value);
    bytes
}

/// Decodes the target's `long double` byte representation into a PHP float.
pub fn decode(bytes: &[u8], size: usize) -> f64 {
    if size <= 8 {
        let mut raw = [0u8; 8];
        let length = bytes.len().min(8);
        raw[..length].copy_from_slice(&bytes[..length]);
        return f64::from_ne_bytes(raw);
    }
    let mut buffer = [0u8; 16];
    let length = bytes.len().min(16);
    buffer[..length].copy_from_slice(&bytes[..length]);
    read(buffer.as_ptr() as usize, size)
}

/// Converts an x87 80-bit extended value stored in 16 bytes to an f64.
fn read_x87(bytes: [u8; 16]) -> f64 {
    let significand = u64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0; 8]));
    let sign_exp = u16::from_le_bytes(bytes[8..10].try_into().unwrap_or([0; 2]));
    let negative = sign_exp & 0x8000 != 0;
    let exponent = i32::from(sign_exp & 0x7FFF);
    let sign_bits = u64::from(negative) << 63;
    if exponent == 0x7FFF {
        if significand & 0x7FFF_FFFF_FFFF_FFFF == 0 {
            return if negative { f64::NEG_INFINITY } else { f64::INFINITY };
        }
        return f64::from_bits(0x7FF8_0000_0000_0000);
    }
    if exponent == 0 {
        // Every x87 denormal is below f64's smallest subnormal.
        return if negative { -0.0 } else { 0.0 };
    }
    let f64_exponent = exponent - 15360;
    if f64_exponent >= 0x7FF {
        return if negative { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    if f64_exponent > 0 {
        let mut fraction = (significand >> 11) & ((1u64 << 52) - 1);
        let round = (significand >> 10) & 1;
        let sticky = significand & ((1u64 << 10) - 1) != 0;
        let mut adjusted = f64_exponent as u64;
        if round == 1 && (sticky || fraction & 1 == 1) {
            fraction += 1;
            if fraction == 1u64 << 52 {
                fraction = 0;
                adjusted += 1;
                if adjusted >= 0x7FF {
                    return if negative { f64::NEG_INFINITY } else { f64::INFINITY };
                }
            }
        }
        f64::from_bits(sign_bits | adjusted << 52 | fraction)
    } else {
        // value = significand * 2^(exponent - 16446); f64 quantum is 2^-1074.
        let rounded = round_shift_u64(significand, 15372 - exponent);
        if rounded == 0 {
            return if negative { -0.0 } else { 0.0 };
        }
        if rounded >= 1u64 << 52 {
            return f64::from_bits(sign_bits | 1u64 << 52);
        }
        f64::from_bits(sign_bits | rounded)
    }
}

/// Encodes an f64 as an x87 80-bit extended value in 16 bytes.
fn write_x87(value: f64) -> [u8; 16] {
    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let exponent = i32::from(((bits >> 52) & 0x7FF) as u16);
    let mut fraction = bits & ((1u64 << 52) - 1);
    let mut sign_exp = u16::from(negative) << 15;
    let significand;
    if exponent == 0x7FF {
        sign_exp |= 0x7FFF;
        significand = if fraction == 0 {
            0x8000_0000_0000_0000
        } else {
            0xC000_0000_0000_0000
        };
    } else if exponent == 0 && fraction == 0 {
        significand = 0;
    } else {
        let unbiased = if exponent == 0 {
            let mut unbiased = -1022;
            while fraction & (1u64 << 52) == 0 {
                fraction <<= 1;
                unbiased -= 1;
            }
            fraction &= (1u64 << 52) - 1;
            unbiased
        } else {
            exponent - 1023
        };
        significand = (1u64 << 63) | (fraction << 11);
        sign_exp |= (unbiased + 16383) as u16;
    }
    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&significand.to_le_bytes());
    bytes[8..10].copy_from_slice(&sign_exp.to_le_bytes());
    bytes
}

/// Converts an IEEE binary128 value stored in 16 bytes to an f64.
fn read_binary128(bytes: [u8; 16]) -> f64 {
    let lo = u64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0; 8]));
    let hi = u64::from_le_bytes(bytes[8..16].try_into().unwrap_or([0; 8]));
    let negative = hi >> 63 != 0;
    let exponent = i32::from(((hi >> 48) & 0x7FFF) as u16);
    let fraction = u128::from(hi & 0x0000_FFFF_FFFF_FFFF) << 64 | u128::from(lo);
    let sign_bits = u64::from(negative) << 63;
    if exponent == 0x7FFF {
        if fraction == 0 {
            return if negative { f64::NEG_INFINITY } else { f64::INFINITY };
        }
        return f64::from_bits(0x7FF8_0000_0000_0000);
    }
    if exponent == 0 {
        // Every binary128 denormal is below f64's smallest subnormal.
        return if negative { -0.0 } else { 0.0 };
    }
    let f64_exponent = exponent - 15360;
    if f64_exponent >= 0x7FF {
        return if negative { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    if f64_exponent > 0 {
        let mut rounded_fraction = ((fraction >> 60) as u64) & ((1u64 << 52) - 1);
        let round = (fraction >> 59) & 1;
        let sticky = fraction & ((1u128 << 59) - 1) != 0;
        let mut adjusted = f64_exponent as u64;
        if round == 1 && (sticky || rounded_fraction & 1 == 1) {
            rounded_fraction += 1;
            if rounded_fraction == 1u64 << 52 {
                rounded_fraction = 0;
                adjusted += 1;
                if adjusted >= 0x7FF {
                    return if negative { f64::NEG_INFINITY } else { f64::INFINITY };
                }
            }
        }
        f64::from_bits(sign_bits | adjusted << 52 | rounded_fraction)
    } else {
        // value = (1.fraction) * 2^(exponent - 16383); f64 quantum is 2^-1074.
        let mantissa = (1u128 << 112) | fraction;
        let rounded = round_shift_u128(mantissa, 15421 - exponent);
        if rounded == 0 {
            return if negative { -0.0 } else { 0.0 };
        }
        if rounded >= 1u64 << 52 {
            return f64::from_bits(sign_bits | 1u64 << 52);
        }
        f64::from_bits(sign_bits | rounded)
    }
}

/// Encodes an f64 as an IEEE binary128 value in 16 bytes.
fn write_binary128(value: f64) -> [u8; 16] {
    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let exponent = i32::from(((bits >> 52) & 0x7FF) as u16);
    let mut fraction = bits & ((1u64 << 52) - 1);
    let sign = u128::from(negative) << 127;
    let (high_fraction, low_fraction);
    if exponent == 0x7FF {
        let payload = if fraction == 0 { 0 } else { 1u128 << 111 };
        high_fraction = (payload >> 64) as u64;
        low_fraction = payload as u64;
        let hi = (sign >> 64) as u64 | (0x7FFF << 48) | high_fraction;
        return pack_binary128(hi, low_fraction);
    }
    if exponent == 0 && fraction == 0 {
        return pack_binary128((sign >> 64) as u64, 0);
    }
    let unbiased = if exponent == 0 {
        let mut unbiased = -1022;
        while fraction & (1u64 << 52) == 0 {
            fraction <<= 1;
            unbiased -= 1;
        }
        fraction &= (1u64 << 52) - 1;
        unbiased
    } else {
        exponent - 1023
    };
    let binary_fraction = u128::from(fraction) << 60;
    let binary_exponent = u128::from((unbiased + 16383) as u16) << 112;
    let packed = sign | binary_exponent | binary_fraction;
    pack_binary128((packed >> 64) as u64, packed as u64)
}

/// Packs the high and low words of a binary128 value in little-endian order.
fn pack_binary128(hi: u64, lo: u64) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&lo.to_le_bytes());
    bytes[8..16].copy_from_slice(&hi.to_le_bytes());
    bytes
}

/// Rounds `value >> shift` to nearest, ties to even.
fn round_shift_u64(value: u64, shift: i32) -> u64 {
    if shift <= 0 {
        return value;
    }
    if shift >= 64 {
        return if shift == 64 && value > 1u64 << 63 { 1 } else { 0 };
    }
    let shift = shift as u32;
    let quotient = value >> shift;
    let remainder = value & ((1u64 << shift) - 1);
    let half = 1u64 << (shift - 1);
    if remainder > half || (remainder == half && quotient & 1 == 1) {
        quotient + 1
    } else {
        quotient
    }
}

/// Rounds `value >> shift` to nearest, ties to even.
fn round_shift_u128(value: u128, shift: i32) -> u64 {
    if shift <= 0 {
        return value as u64;
    }
    if shift >= 128 {
        return 0;
    }
    let shift = shift as u32;
    let quotient = value >> shift;
    let remainder = value & ((1u128 << shift) - 1);
    let half = 1u128 << (shift - 1);
    let rounded = if remainder > half || (remainder == half && quotient & 1 == 1) {
        quotient + 1
    } else {
        quotient
    };
    rounded as u64
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for x87 and binary128 conversions, run on every host regardless of target.
    //!
    //! Called from:
    //! - `cargo test -p elephc-ffi`.
    //!
    //! Key details:
    //! - Exact bit patterns pin the encodings; round-trips exercise normal, subnormal,
    //!   infinity, and NaN values without relying on the host's `long double` size.

    use super::*;

    /// Asserts that an f64 survives an x87 round-trip bit-for-bit.
    fn assert_x87_roundtrip(value: f64) {
        let decoded = read_x87(write_x87(value));
        if value.is_nan() {
            assert!(decoded.is_nan(), "NaN did not survive: {value}");
        } else {
            assert_eq!(decoded.to_bits(), value.to_bits(), "x87 round-trip for {value}");
        }
    }

    /// Asserts that an f64 survives a binary128 round-trip bit-for-bit.
    fn assert_binary128_roundtrip(value: f64) {
        let decoded = read_binary128(write_binary128(value));
        if value.is_nan() {
            assert!(decoded.is_nan(), "NaN did not survive: {value}");
        } else {
            assert_eq!(decoded.to_bits(), value.to_bits(), "binary128 round-trip for {value}");
        }
    }

    /// Pins the canonical x87 encoding of 1.5 and decodes a raw 1.0.
    #[test]
    fn x87_exact_patterns() {
        let bytes = write_x87(1.5);
        assert_eq!(u64::from_le_bytes(bytes[0..8].try_into().unwrap()), 0xC000_0000_0000_0000);
        assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 0x3FFF);
        let mut one = [0u8; 16];
        one[0..8].copy_from_slice(&0x8000_0000_0000_0000u64.to_le_bytes());
        one[8..10].copy_from_slice(&0x3FFFu16.to_le_bytes());
        assert_eq!(read_x87(one), 1.0);
    }

    /// Pins the canonical binary128 encoding of 1.5 and decodes a raw 1.0.
    #[test]
    fn binary128_exact_patterns() {
        let bytes = write_binary128(1.5);
        assert_eq!(u64::from_le_bytes(bytes[8..16].try_into().unwrap()), 0x3FFF_8000_0000_0000);
        assert_eq!(u64::from_le_bytes(bytes[0..8].try_into().unwrap()), 0);
        let one = pack_binary128(0x3FFF_0000_0000_0000, 0);
        assert_eq!(read_binary128(one), 1.0);
    }

    /// Round-trips representative finite, subnormal, infinite, and NaN values.
    #[test]
    fn long_double_roundtrips() {
        for value in [
            0.0,
            -0.0,
            1.5,
            -2.25,
            1e300,
            -1e300,
            1e-300,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ] {
            assert_x87_roundtrip(value);
            assert_binary128_roundtrip(value);
        }
    }

    /// Rounds an x87 value whose low bits are beyond f64 precision.
    #[test]
    fn x87_reads_round_to_f64() {
        let mut bytes = [0u8; 16];
        // 1 + 2^-53 + 2^-63: rounds up to the next f64 above 1.0.
        let significand = 0x8000_0000_0000_0000u64 | (1u64 << 10) | 1;
        bytes[0..8].copy_from_slice(&significand.to_le_bytes());
        bytes[8..10].copy_from_slice(&0x3FFFu16.to_le_bytes());
        assert_eq!(read_x87(bytes), f64::from_bits(1.0f64.to_bits() + 1));
    }

    /// Rounds a binary128 value whose low bits are beyond f64 precision.
    #[test]
    fn binary128_reads_round_to_f64() {
        // 1 + 2^-53 + 2^-112: rounds up to the next f64 above 1.0.
        let fraction = (1u128 << 59) | 1;
        let hi = 0x3FFF_0000_0000_0000u64 | ((fraction >> 64) as u64 & 0x0000_FFFF_FFFF_FFFF);
        let bytes = pack_binary128(hi, fraction as u64);
        assert_eq!(read_binary128(bytes), f64::from_bits(1.0f64.to_bits() + 1));
    }
}
