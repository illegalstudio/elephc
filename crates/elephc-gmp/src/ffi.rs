//! Purpose:
//! Declares the subset of libgmp's C API the elephc GMP bridge calls.
//!
//! Called from:
//! - `crate::mpz`, `crate::ops` and `crate::abi` for every GMP operation.
//!
//! Key details:
//! - The declarations mirror `gmp.h` for the LP64 targets elephc supports
//!   (`mp_limb_t` is `u64`, `mp_bitcnt_t`/`unsigned long` are `u64`).
//! - No symbol is linked here: the bridge is a staticlib whose `__gmpz_*`
//!   references are resolved by the managed `gmp` package at final link time.

#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_int, c_ulong, c_void};

/// GNU MP's `__mpz_struct`; `mpz_t` is an array of one of these.
#[repr(C)]
pub struct MpzRaw {
    pub alloc: c_int,
    pub size: c_int,
    pub data: *mut u64,
}

/// GNU MP's `__gmp_randstate_struct` for the 64-bit LP64 layout.
#[repr(C)]
pub struct RandStateRaw {
    pub seed: MpzRaw,
    pub algorithm: c_int,
    pub padding: c_int,
    pub algorithm_data: *mut c_void,
}

extern "C" {
    /// Initializes the destination GMP integer to zero.
    pub fn __gmpz_init(rop: *mut MpzRaw);
    /// Releases the limbs owned by an initialized GMP integer.
    pub fn __gmpz_clear(rop: *mut MpzRaw);
    /// Copies an initialized GMP integer into the destination.
    pub fn __gmpz_set(rop: *mut MpzRaw, op: *const MpzRaw);
    /// Assigns a signed machine-word value to the destination.
    pub fn __gmpz_set_si(rop: *mut MpzRaw, op: c_ulong);
    /// Parses a NUL-terminated integer in the selected base and reports conversion status.
    pub fn __gmpz_set_str(rop: *mut MpzRaw, s: *const c_char, base: c_int) -> c_int;
    /// Formats an integer in the selected base into a C string.
    pub fn __gmpz_get_str(s: *mut c_char, base: c_int, op: *const MpzRaw) -> *mut c_char;
    /// Reports the digit count used to size a base-specific output buffer.
    pub fn __gmpz_sizeinbase(op: *const MpzRaw, base: c_int) -> usize;
    /// Converts an integer to a signed machine-word value.
    pub fn __gmpz_get_si(op: *const MpzRaw) -> c_ulong;
    /// Converts an integer to an unsigned machine-word value.
    pub fn __gmpz_get_ui(op: *const MpzRaw) -> c_ulong;
    /// Reports whether an integer fits in a native signed long.
    pub fn __gmpz_fits_slong_p(op: *const MpzRaw) -> c_int;
    /// Converts an integer to a double-precision floating-point value.
    pub fn __gmpz_get_d(op: *const MpzRaw) -> f64;
    /// Compares two arbitrary-precision integers.
    pub fn __gmpz_cmp(op1: *const MpzRaw, op2: *const MpzRaw) -> c_int;
    /// Compares an arbitrary-precision integer with an unsigned machine word.
    pub fn __gmpz_cmp_ui(op1: *const MpzRaw, op2: c_ulong) -> c_int;

    /// Stores the sum of two integers in the destination.
    pub fn __gmpz_add(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    /// Adds an unsigned machine word to an arbitrary-precision integer.
    pub fn __gmpz_add_ui(rop: *mut MpzRaw, op1: *const MpzRaw, op2: c_ulong);
    /// Stores the difference of two integers in the destination.
    pub fn __gmpz_sub(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    /// Stores the product of two integers in the destination.
    pub fn __gmpz_mul(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    /// Stores the negated operand in the destination.
    pub fn __gmpz_neg(rop: *mut MpzRaw, op: *const MpzRaw);
    /// Stores the absolute value of the operand in the destination.
    pub fn __gmpz_abs(rop: *mut MpzRaw, op: *const MpzRaw);
    /// Stores the bitwise complement of the operand in the destination.
    pub fn __gmpz_com(rop: *mut MpzRaw, op: *const MpzRaw);

    /// Computes an integer quotient rounded toward zero.
    pub fn __gmpz_tdiv_q(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    /// Computes the remainder for division rounded toward zero.
    pub fn __gmpz_tdiv_r(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    /// Computes both quotient and remainder for division rounded toward zero.
    pub fn __gmpz_tdiv_qr(q: *mut MpzRaw, r: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    /// Computes an integer quotient rounded toward positive infinity.
    pub fn __gmpz_cdiv_q(q: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    /// Computes the remainder for division rounded toward positive infinity.
    pub fn __gmpz_cdiv_r(r: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    /// Computes quotient and remainder for division rounded toward positive infinity.
    pub fn __gmpz_cdiv_qr(q: *mut MpzRaw, r: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    /// Computes an integer quotient rounded toward negative infinity.
    pub fn __gmpz_fdiv_q(q: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    /// Computes the remainder for division rounded toward negative infinity.
    pub fn __gmpz_fdiv_r(r: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    /// Computes quotient and remainder for division rounded toward negative infinity.
    pub fn __gmpz_fdiv_qr(q: *mut MpzRaw, r: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    /// Computes the nonnegative remainder modulo the divisor.
    pub fn __gmpz_mod(r: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    /// Divides two integers when the caller has established exact divisibility.
    pub fn __gmpz_divexact(q: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);

    /// Stores the bitwise intersection of two integers.
    pub fn __gmpz_and(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    /// Stores the bitwise union of two integers.
    pub fn __gmpz_ior(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    /// Stores the bitwise exclusive-or of two integers.
    pub fn __gmpz_xor(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    /// Multiplies an integer by a power of two.
    pub fn __gmpz_mul_2exp(rop: *mut MpzRaw, op1: *const MpzRaw, op2: c_ulong);
    /// Divides by a power of two, rounding toward negative infinity.
    pub fn __gmpz_fdiv_q_2exp(rop: *mut MpzRaw, op1: *const MpzRaw, op2: c_ulong);

    /// Computes the factorial of an unsigned machine-word operand.
    pub fn __gmpz_fac_ui(rop: *mut MpzRaw, n: c_ulong);
    /// Computes the binomial coefficient of an integer and an unsigned index.
    pub fn __gmpz_bin_ui(rop: *mut MpzRaw, n: *const MpzRaw, k: c_ulong);
    /// Raises an integer to an unsigned machine-word exponent.
    pub fn __gmpz_pow_ui(rop: *mut MpzRaw, base: *const MpzRaw, exp: c_ulong);
    /// Computes an integer power modulo the supplied modulus.
    pub fn __gmpz_powm(rop: *mut MpzRaw, base: *const MpzRaw, exp: *const MpzRaw, modulus: *const MpzRaw);
    /// Computes the integer square root of a nonnegative operand.
    pub fn __gmpz_sqrt(rop: *mut MpzRaw, op: *const MpzRaw);
    /// Computes the integer square root and its remainder.
    pub fn __gmpz_sqrtrem(rop1: *mut MpzRaw, rop2: *mut MpzRaw, op: *const MpzRaw);
    /// Computes an integer nth root and reports whether it is exact.
    pub fn __gmpz_root(rop: *mut MpzRaw, op: *const MpzRaw, n: c_ulong) -> c_int;
    /// Computes an integer nth root and its remainder.
    pub fn __gmpz_rootrem(rop1: *mut MpzRaw, rop2: *mut MpzRaw, op: *const MpzRaw, n: c_ulong);
    /// Reports whether an integer is a perfect square.
    pub fn __gmpz_perfect_square_p(op: *const MpzRaw) -> c_int;
    /// Reports whether an integer is a perfect power.
    pub fn __gmpz_perfect_power_p(op: *const MpzRaw) -> c_int;
    /// Classifies an integer with GMP primality checks using the requested repetitions.
    pub fn __gmpz_probab_prime_p(n: *const MpzRaw, reps: c_int) -> c_int;
    /// Finds the next prime candidate after the operand.
    pub fn __gmpz_nextprime(rop: *mut MpzRaw, op: *const MpzRaw);

    /// Computes the greatest common divisor of two integers.
    pub fn __gmpz_gcd(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    /// Computes the least common multiple of two integers.
    pub fn __gmpz_lcm(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    /// Computes the greatest common divisor and its Bezout coefficients.
    pub fn __gmpz_gcdext(g: *mut MpzRaw, s: *mut MpzRaw, t: *mut MpzRaw, a: *const MpzRaw, b: *const MpzRaw);
    /// Computes a modular inverse and reports whether one exists.
    pub fn __gmpz_invert(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw) -> c_int;
    /// Computes the Jacobi symbol of the supplied integers.
    pub fn __gmpz_jacobi(a: *const MpzRaw, b: *const MpzRaw) -> c_int;
    /// Computes the Legendre symbol for the supplied prime modulus.
    pub fn __gmpz_legendre(a: *const MpzRaw, p: *const MpzRaw) -> c_int;

    /// Reads one bit of an integer.
    pub fn __gmpz_tstbit(op: *const MpzRaw, bit: c_ulong) -> c_int;
    /// Sets one bit of the destination integer.
    pub fn __gmpz_setbit(rop: *mut MpzRaw, bit: c_ulong);
    /// Clears one bit of the destination integer.
    pub fn __gmpz_clrbit(rop: *mut MpzRaw, bit: c_ulong);
    /// Counts the set bits in an integer.
    pub fn __gmpz_popcount(op: *const MpzRaw) -> c_ulong;
    /// Computes the Hamming distance between two integers.
    pub fn __gmpz_hamdist(op1: *const MpzRaw, op2: *const MpzRaw) -> c_ulong;
    /// Finds a zero bit at or after the requested starting position.
    pub fn __gmpz_scan0(op: *const MpzRaw, starting_bit: c_ulong) -> c_ulong;
    /// Finds a set bit at or after the requested starting position.
    pub fn __gmpz_scan1(op: *const MpzRaw, starting_bit: c_ulong) -> c_ulong;

    /// Imports fixed-size words using the selected ordering, byte layout and nail bits.
    pub fn __gmpz_import(
        rop: *mut MpzRaw,
        count: usize,
        order: c_int,
        size: usize,
        endian: c_int,
        nails: usize,
        op: *const c_void,
    );
    /// Exports an integer into fixed-size words with the selected byte layout.
    pub fn __gmpz_export(
        rop: *mut c_void,
        countp: *mut usize,
        order: c_int,
        size: usize,
        endian: c_int,
        nails: usize,
        op: *const MpzRaw,
    ) -> usize;

    /// Initializes a GMP Mersenne Twister random state.
    pub fn __gmp_randinit_mt(state: *mut RandStateRaw);
    /// Seeds an initialized random state from an arbitrary-precision integer.
    pub fn __gmp_randseed(state: *mut RandStateRaw, seed: *const MpzRaw);
    /// Releases the storage owned by a GMP random state.
    pub fn __gmp_randclear(state: *mut RandStateRaw);
    /// Draws a uniformly distributed integer with the requested bit width.
    pub fn __gmpz_urandomb(rop: *mut MpzRaw, state: *mut RandStateRaw, n: c_ulong);
    /// Draws a uniformly distributed nonnegative integer below the supplied bound.
    pub fn __gmpz_urandomm(rop: *mut MpzRaw, state: *mut RandStateRaw, n: *const MpzRaw);

    pub static __gmp_version: *const c_char;
}
