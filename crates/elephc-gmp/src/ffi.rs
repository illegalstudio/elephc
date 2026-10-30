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
    pub fn __gmpz_init(rop: *mut MpzRaw);
    pub fn __gmpz_clear(rop: *mut MpzRaw);
    pub fn __gmpz_set(rop: *mut MpzRaw, op: *const MpzRaw);
    pub fn __gmpz_set_si(rop: *mut MpzRaw, op: c_ulong);
    pub fn __gmpz_set_str(rop: *mut MpzRaw, s: *const c_char, base: c_int) -> c_int;
    pub fn __gmpz_get_str(s: *mut c_char, base: c_int, op: *const MpzRaw) -> *mut c_char;
    pub fn __gmpz_sizeinbase(op: *const MpzRaw, base: c_int) -> usize;
    pub fn __gmpz_get_si(op: *const MpzRaw) -> c_ulong;
    pub fn __gmpz_get_ui(op: *const MpzRaw) -> c_ulong;
    pub fn __gmpz_fits_slong_p(op: *const MpzRaw) -> c_int;
    pub fn __gmpz_get_d(op: *const MpzRaw) -> f64;
    pub fn __gmpz_cmp(op1: *const MpzRaw, op2: *const MpzRaw) -> c_int;
    pub fn __gmpz_cmp_ui(op1: *const MpzRaw, op2: c_ulong) -> c_int;

    pub fn __gmpz_add(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    pub fn __gmpz_add_ui(rop: *mut MpzRaw, op1: *const MpzRaw, op2: c_ulong);
    pub fn __gmpz_sub(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    pub fn __gmpz_mul(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    pub fn __gmpz_neg(rop: *mut MpzRaw, op: *const MpzRaw);
    pub fn __gmpz_abs(rop: *mut MpzRaw, op: *const MpzRaw);
    pub fn __gmpz_com(rop: *mut MpzRaw, op: *const MpzRaw);

    pub fn __gmpz_tdiv_q(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    pub fn __gmpz_tdiv_r(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    pub fn __gmpz_tdiv_qr(q: *mut MpzRaw, r: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    pub fn __gmpz_cdiv_q(q: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    pub fn __gmpz_cdiv_r(r: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    pub fn __gmpz_cdiv_qr(q: *mut MpzRaw, r: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    pub fn __gmpz_fdiv_q(q: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    pub fn __gmpz_fdiv_r(r: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    pub fn __gmpz_fdiv_qr(q: *mut MpzRaw, r: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    pub fn __gmpz_mod(r: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);
    pub fn __gmpz_divexact(q: *mut MpzRaw, n: *const MpzRaw, d: *const MpzRaw);

    pub fn __gmpz_and(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    pub fn __gmpz_ior(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    pub fn __gmpz_xor(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    pub fn __gmpz_mul_2exp(rop: *mut MpzRaw, op1: *const MpzRaw, op2: c_ulong);
    pub fn __gmpz_fdiv_q_2exp(rop: *mut MpzRaw, op1: *const MpzRaw, op2: c_ulong);

    pub fn __gmpz_fac_ui(rop: *mut MpzRaw, n: c_ulong);
    pub fn __gmpz_bin_ui(rop: *mut MpzRaw, n: *const MpzRaw, k: c_ulong);
    pub fn __gmpz_pow_ui(rop: *mut MpzRaw, base: *const MpzRaw, exp: c_ulong);
    pub fn __gmpz_powm(rop: *mut MpzRaw, base: *const MpzRaw, exp: *const MpzRaw, modulus: *const MpzRaw);
    pub fn __gmpz_sqrt(rop: *mut MpzRaw, op: *const MpzRaw);
    pub fn __gmpz_sqrtrem(rop1: *mut MpzRaw, rop2: *mut MpzRaw, op: *const MpzRaw);
    pub fn __gmpz_root(rop: *mut MpzRaw, op: *const MpzRaw, n: c_ulong) -> c_int;
    pub fn __gmpz_rootrem(rop1: *mut MpzRaw, rop2: *mut MpzRaw, op: *const MpzRaw, n: c_ulong);
    pub fn __gmpz_perfect_square_p(op: *const MpzRaw) -> c_int;
    pub fn __gmpz_perfect_power_p(op: *const MpzRaw) -> c_int;
    pub fn __gmpz_probab_prime_p(n: *const MpzRaw, reps: c_int) -> c_int;
    pub fn __gmpz_nextprime(rop: *mut MpzRaw, op: *const MpzRaw);

    pub fn __gmpz_gcd(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    pub fn __gmpz_lcm(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw);
    pub fn __gmpz_gcdext(g: *mut MpzRaw, s: *mut MpzRaw, t: *mut MpzRaw, a: *const MpzRaw, b: *const MpzRaw);
    pub fn __gmpz_invert(rop: *mut MpzRaw, op1: *const MpzRaw, op2: *const MpzRaw) -> c_int;
    pub fn __gmpz_jacobi(a: *const MpzRaw, b: *const MpzRaw) -> c_int;
    pub fn __gmpz_legendre(a: *const MpzRaw, p: *const MpzRaw) -> c_int;

    pub fn __gmpz_tstbit(op: *const MpzRaw, bit: c_ulong) -> c_int;
    pub fn __gmpz_setbit(rop: *mut MpzRaw, bit: c_ulong);
    pub fn __gmpz_clrbit(rop: *mut MpzRaw, bit: c_ulong);
    pub fn __gmpz_popcount(op: *const MpzRaw) -> c_ulong;
    pub fn __gmpz_hamdist(op1: *const MpzRaw, op2: *const MpzRaw) -> c_ulong;
    pub fn __gmpz_scan0(op: *const MpzRaw, starting_bit: c_ulong) -> c_ulong;
    pub fn __gmpz_scan1(op: *const MpzRaw, starting_bit: c_ulong) -> c_ulong;

    pub fn __gmpz_import(
        rop: *mut MpzRaw,
        count: usize,
        order: c_int,
        size: usize,
        endian: c_int,
        nails: usize,
        op: *const c_void,
    );
    pub fn __gmpz_export(
        rop: *mut c_void,
        countp: *mut usize,
        order: c_int,
        size: usize,
        endian: c_int,
        nails: usize,
        op: *const MpzRaw,
    ) -> usize;

    pub fn __gmp_randinit_mt(state: *mut RandStateRaw);
    pub fn __gmp_randseed(state: *mut RandStateRaw, seed: *const MpzRaw);
    pub fn __gmp_randclear(state: *mut RandStateRaw);
    pub fn __gmpz_urandomb(rop: *mut MpzRaw, state: *mut RandStateRaw, n: c_ulong);
    pub fn __gmpz_urandomm(rop: *mut MpzRaw, state: *mut RandStateRaw, n: *const MpzRaw);

    pub static __gmp_version: *const c_char;
}
