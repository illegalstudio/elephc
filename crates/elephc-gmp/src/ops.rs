//! Purpose:
//! Implements every raw GMP operation the PHP prelude asks the bridge to perform.
//!
//! Called from:
//! - `crate::abi`'s extern entry points, one per operation family.
//!
//! Key details:
//! - Operands arrive as canonical decimal strings and results leave the same way.
//! - User-facing validation (base ranges, argument names, negative operands) lives in
//!   the PHP prelude; this layer still fails closed on division by zero and invalid
//!   internal input instead of invoking undefined libgmp behaviour.
//! - Multi-value results use `|`: quotient|remainder, root|remainder, g|s|t.

use crate::ffi::*;
use crate::mpz::Mpz;
use crate::packet::*;

/* Binary operation codes. Keep in step with `elephc_gmp_binary` callers in
 * `src/gmp_prelude/prelude.php`; the Rust tests pin every variant. */
pub const BIN_ADD: i32 = 0;
pub const BIN_SUB: i32 = 1;
pub const BIN_MUL: i32 = 2;
pub const BIN_TDIV_Q: i32 = 3;
pub const BIN_MOD: i32 = 4;
pub const BIN_DIVEXACT: i32 = 5;
pub const BIN_AND: i32 = 6;
pub const BIN_OR: i32 = 7;
pub const BIN_XOR: i32 = 8;
pub const BIN_GCD: i32 = 9;
pub const BIN_LCM: i32 = 10;
pub const BIN_CMP: i32 = 11;
pub const BIN_JACOBI: i32 = 12;
pub const BIN_LEGENDRE: i32 = 13;
pub const BIN_KRONECKER: i32 = 14;
pub const BIN_HAMDIST: i32 = 15;
pub const BIN_INVERT: i32 = 16;
pub const BIN_DIV_QR_TDIV: i32 = 17;
pub const BIN_DIV_QR_CDIV: i32 = 18;
pub const BIN_DIV_QR_FDIV: i32 = 19;
pub const BIN_GCDEXT: i32 = 20;

/* Unary operation codes. */
pub const UN_NEG: i32 = 0;
pub const UN_ABS: i32 = 1;
pub const UN_COM: i32 = 2;
pub const UN_NEXTPRIME: i32 = 3;
pub const UN_SQRT: i32 = 4;
pub const UN_FACT: i32 = 5;
pub const UN_SIGN: i32 = 6;
pub const UN_POPCOUNT: i32 = 7;
pub const UN_PERFECT_SQUARE: i32 = 8;
pub const UN_PERFECT_POWER: i32 = 9;
pub const UN_PROB_PRIME: i32 = 10;
pub const UN_ROOT: i32 = 11;
pub const UN_ROOTREM: i32 = 12;
pub const UN_SQRTREM: i32 = 13;
pub const UN_POW: i32 = 14;
pub const UN_SHL: i32 = 15;
pub const UN_SHR: i32 = 16;
pub const UN_TESTBIT: i32 = 17;
pub const UN_SCAN0: i32 = 20;
pub const UN_SCAN1: i32 = 21;
pub const UN_BINOMIAL: i32 = 22;

/* Ternary operation codes. */
pub const TER_POWM: i32 = 0;

/// Parses a canonical decimal operand or reports an internal protocol error.
fn operand(value: &str) -> Result<Mpz, String> {
    Mpz::from_decimal(value)
}

/// Evaluates one binary operation.
pub fn binary(op: i32, left: &str, right: &str) -> String {
    let left = match operand(left) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    let right = match operand(right) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    let mut result = Mpz::new();
    match op {
        BIN_ADD => unsafe { __gmpz_add(result.as_mut_ptr(), left.as_ptr(), right.as_ptr()) },
        BIN_SUB => unsafe { __gmpz_sub(result.as_mut_ptr(), left.as_ptr(), right.as_ptr()) },
        BIN_MUL => unsafe { __gmpz_mul(result.as_mut_ptr(), left.as_ptr(), right.as_ptr()) },
        BIN_TDIV_Q => {
            if right.is_zero() {
                return division_by_zero("Division by zero");
            }
            unsafe { __gmpz_tdiv_q(result.as_mut_ptr(), left.as_ptr(), right.as_ptr()) }
        }
        BIN_MOD => {
            if right.is_zero() {
                return division_by_zero("Modulo by zero");
            }
            unsafe { __gmpz_mod(result.as_mut_ptr(), left.as_ptr(), right.as_ptr()) }
        }
        BIN_DIVEXACT => {
            if right.is_zero() {
                return division_by_zero("Division by zero");
            }
            unsafe { __gmpz_divexact(result.as_mut_ptr(), left.as_ptr(), right.as_ptr()) }
        }
        BIN_AND => unsafe { __gmpz_and(result.as_mut_ptr(), left.as_ptr(), right.as_ptr()) },
        BIN_OR => unsafe { __gmpz_ior(result.as_mut_ptr(), left.as_ptr(), right.as_ptr()) },
        BIN_XOR => unsafe { __gmpz_xor(result.as_mut_ptr(), left.as_ptr(), right.as_ptr()) },
        BIN_GCD => unsafe { __gmpz_gcd(result.as_mut_ptr(), left.as_ptr(), right.as_ptr()) },
        BIN_LCM => unsafe { __gmpz_lcm(result.as_mut_ptr(), left.as_ptr(), right.as_ptr()) },
        BIN_CMP => {
            let raw = unsafe { __gmpz_cmp(left.as_ptr(), right.as_ptr()) };
            return ok(sign_text(raw));
        }
        BIN_JACOBI => return ok(unsafe { __gmpz_jacobi(left.as_ptr(), right.as_ptr()) }.to_string()),
        BIN_LEGENDRE => {
            return ok(unsafe { __gmpz_legendre(left.as_ptr(), right.as_ptr()) }.to_string())
        }
        BIN_KRONECKER => {
            return ok(unsafe { __gmpz_jacobi(left.as_ptr(), right.as_ptr()) }.to_string())
        }
        BIN_HAMDIST => {
            let distance = unsafe { __gmpz_hamdist(left.as_ptr(), right.as_ptr()) };
            return ok((distance as i64).to_string());
        }
        BIN_INVERT => {
            if right.is_zero() {
                return division_by_zero("Division by zero");
            }
            if unsafe { __gmpz_invert(result.as_mut_ptr(), left.as_ptr(), right.as_ptr()) } == 0 {
                return false_value();
            }
        }
        BIN_DIV_QR_TDIV | BIN_DIV_QR_CDIV | BIN_DIV_QR_FDIV => {
            if right.is_zero() {
                return division_by_zero("Division by zero");
            }
            let mut remainder = Mpz::new();
            unsafe {
                match op {
                    BIN_DIV_QR_TDIV => __gmpz_tdiv_qr(
                        result.as_mut_ptr(),
                        remainder.as_mut_ptr(),
                        left.as_ptr(),
                        right.as_ptr(),
                    ),
                    BIN_DIV_QR_CDIV => __gmpz_cdiv_qr(
                        result.as_mut_ptr(),
                        remainder.as_mut_ptr(),
                        left.as_ptr(),
                        right.as_ptr(),
                    ),
                    _ => __gmpz_fdiv_qr(
                        result.as_mut_ptr(),
                        remainder.as_mut_ptr(),
                        left.as_ptr(),
                        right.as_ptr(),
                    ),
                }
            }
            return ok(format!("{}|{}", result.to_decimal(), remainder.to_decimal()));
        }
        BIN_GCDEXT => {
            let mut s = Mpz::new();
            let mut t = Mpz::new();
            unsafe {
                __gmpz_gcdext(
                    result.as_mut_ptr(),
                    s.as_mut_ptr(),
                    t.as_mut_ptr(),
                    left.as_ptr(),
                    right.as_ptr(),
                );
            }
            return ok(format!(
                "{}|{}|{}",
                result.to_decimal(),
                s.to_decimal(),
                t.to_decimal()
            ));
        }
        _ => return internal_error(format!("unknown GMP binary operation {op}")),
    }
    ok(result.to_decimal())
}

/// Evaluates one unary operation with its integer parameter.
pub fn unary(op: i32, value: &str, argument: i64) -> String {
    let value = match operand(value) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    let mut result = Mpz::new();
    match op {
        UN_NEG => unsafe { __gmpz_neg(result.as_mut_ptr(), value.as_ptr()) },
        UN_ABS => unsafe { __gmpz_abs(result.as_mut_ptr(), value.as_ptr()) },
        UN_COM => unsafe { __gmpz_com(result.as_mut_ptr(), value.as_ptr()) },
        UN_NEXTPRIME => unsafe { __gmpz_nextprime(result.as_mut_ptr(), value.as_ptr()) },
        UN_SQRT => {
            if value.sign() < 0 {
                return value_error("must be greater than or equal to 0");
            }
            unsafe { __gmpz_sqrt(result.as_mut_ptr(), value.as_ptr()) }
        }
        UN_FACT => {
            if value.sign() < 0 {
                return value_error("must be greater than or equal to 0");
            }
            unsafe { __gmpz_fac_ui(result.as_mut_ptr(), value.to_u64()) }
        }
        UN_SIGN => return ok(value.sign().to_string()),
        UN_POPCOUNT => return ok((unsafe { __gmpz_popcount(value.as_ptr()) } as i64).to_string()),
        UN_PERFECT_SQUARE => {
            return ok(bool_text(unsafe { __gmpz_perfect_square_p(value.as_ptr()) } != 0))
        }
        UN_PERFECT_POWER => {
            return ok(bool_text(unsafe { __gmpz_perfect_power_p(value.as_ptr()) } != 0))
        }
        UN_PROB_PRIME => {
            return ok(unsafe { __gmpz_probab_prime_p(value.as_ptr(), argument as i32) }.to_string())
        }
        UN_ROOT => {
            if argument <= 0 {
                return value_error("must be greater than 0");
            }
            if argument % 2 == 0 && value.sign() < 0 {
                return value_error("must be odd if argument #1 ($a) is negative");
            }
            unsafe { __gmpz_root(result.as_mut_ptr(), value.as_ptr(), argument as u64) };
        }
        UN_ROOTREM => {
            if argument <= 0 {
                return value_error("must be greater than or equal to 1");
            }
            if argument % 2 == 0 && value.sign() < 0 {
                return value_error("must be odd if argument #1 ($a) is negative");
            }
            let mut remainder = Mpz::new();
            unsafe {
                __gmpz_rootrem(
                    result.as_mut_ptr(),
                    remainder.as_mut_ptr(),
                    value.as_ptr(),
                    argument as u64,
                );
            }
            return ok(format!("{}|{}", result.to_decimal(), remainder.to_decimal()));
        }
        UN_SQRTREM => {
            if value.sign() < 0 {
                return value_error("must be greater than or equal to 0");
            }
            let mut remainder = Mpz::new();
            unsafe {
                __gmpz_sqrtrem(result.as_mut_ptr(), remainder.as_mut_ptr(), value.as_ptr());
            }
            return ok(format!("{}|{}", result.to_decimal(), remainder.to_decimal()));
        }
        UN_POW => {
            if argument < 0 {
                return value_error("must be greater than or equal to 0");
            }
            unsafe { __gmpz_pow_ui(result.as_mut_ptr(), value.as_ptr(), argument as u64) }
        }
        UN_SHL => {
            if argument < 0 {
                return value_error("Shift must be greater than or equal to 0");
            }
            unsafe { __gmpz_mul_2exp(result.as_mut_ptr(), value.as_ptr(), argument as u64) }
        }
        UN_SHR => {
            if argument < 0 {
                return value_error("Shift must be greater than or equal to 0");
            }
            unsafe { __gmpz_fdiv_q_2exp(result.as_mut_ptr(), value.as_ptr(), argument as u64) }
        }
        UN_TESTBIT => {
            if argument < 0 {
                return value_error("must be between 0 and 2147483647 * 64");
            }
            return ok(bool_text(unsafe { __gmpz_tstbit(value.as_ptr(), argument as u64) } != 0));
        }
        UN_SCAN0 => {
            if argument < 0 {
                return value_error("must be between 0 and 2147483647 * 64");
            }
            return ok((unsafe { __gmpz_scan0(value.as_ptr(), argument as u64) } as i64).to_string());
        }
        UN_SCAN1 => {
            if argument < 0 {
                return value_error("must be between 0 and 2147483647 * 64");
            }
            return ok((unsafe { __gmpz_scan1(value.as_ptr(), argument as u64) } as i64).to_string());
        }
        UN_BINOMIAL => {
            if argument < 0 {
                return value_error("must be greater than or equal to 0");
            }
            unsafe { __gmpz_bin_ui(result.as_mut_ptr(), value.as_ptr(), argument as u64) }
        }
        _ => return internal_error(format!("unknown GMP unary operation {op}")),
    }
    ok(result.to_decimal())
}

/// Evaluates modular exponentiation.
pub fn powm(base: &str, exponent: &str, modulus: &str) -> String {
    let base = match operand(base) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    let exponent = match operand(exponent) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    let modulus = match operand(modulus) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    if exponent.sign() < 0 {
        return value_error("must be greater than or equal to 0");
    }
    if modulus.is_zero() {
        return division_by_zero("Modulo by zero");
    }
    let mut result = Mpz::new();
    unsafe {
        __gmpz_powm(
            result.as_mut_ptr(),
            base.as_ptr(),
            exponent.as_ptr(),
            modulus.as_ptr(),
        );
    }
    ok(result.to_decimal())
}

/// Sets or clears one bit and returns the mutated value.
pub fn bit(value: &str, index: i64, set: bool) -> String {
    let mut value = match operand(value) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    if index < 0 || index / 64 >= i64::from(i32::MAX) {
        return value_error("must be between 0 and 2147483647 * 64");
    }
    unsafe {
        if set {
            __gmpz_setbit(value.as_mut_ptr(), index as u64)
        } else {
            __gmpz_clrbit(value.as_mut_ptr(), index as u64)
        }
    }
    ok(value.to_decimal())
}

/// Imports a big-endian byte payload with libgmp's word-order rules.
pub fn import(bytes: &[u8], word_size: usize, order: i32, endian: i32) -> String {
    let mut result = Mpz::new();
    let count = if word_size == 0 { 0 } else { bytes.len() / word_size };
    unsafe {
        __gmpz_import(
            result.as_mut_ptr(),
            count,
            order,
            word_size,
            endian,
            0,
            bytes.as_ptr() as *const std::os::raw::c_void,
        );
    }
    ok(result.to_decimal())
}

/// Exports a value as a byte payload with libgmp's word-order rules.
pub fn export(value: &str, word_size: usize, order: i32, endian: i32) -> String {
    let value = match operand(value) {
        Ok(value) => value,
        Err(error) => return internal_error(error),
    };
    if value.is_zero() {
        return ok("");
    }
    if word_size == 0 || word_size > (usize::MAX / 8) {
        return value_error("is too large for argument #1 ($num)");
    }
    // `mpz_export` with `rop == NULL` allocates the probe result with GMP's
    // allocator; the caller cannot free that pointer. Derive the word count from
    // the exact base-2 bit length instead and allocate the destination once.
    let word_bits = word_size * 8;
    let bits = value.bit_length();
    let count = bits.div_ceil(word_bits);
    if count > (usize::MAX / word_size) {
        return value_error("is too large for argument #1 ($num)");
    }
    let length = count * word_size;
    let mut bytes = vec![0u8; length];
    unsafe {
        __gmpz_export(
            bytes.as_mut_ptr() as *mut std::os::raw::c_void,
            std::ptr::null_mut(),
            order,
            word_size,
            endian,
            0,
            value.as_ptr(),
        );
    }
    ok(crate::mpz::bytes_to_hex(&bytes))
}

/// Formats a raw comparison result as PHP's -1/0/1 ordering.
fn sign_text(raw: i32) -> String {
    if raw < 0 {
        "-1".to_string()
    } else if raw > 0 {
        "1".to_string()
    } else {
        "0".to_string()
    }
}

/// Formats a boolean as the prelude's `1`/`0` packet payload.
fn bool_text(value: bool) -> String {
    if value { "1".to_string() } else { "0".to_string() }
}
