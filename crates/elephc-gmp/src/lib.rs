//! Purpose:
//! libgmp-backed engine and C ABI for elephc's PHP `gmp` extension bridge.
//!
//! Called from:
//! - Compiled PHP programs through `extern "elephc_gmp"` declarations in the
//!   injected GMP prelude.
//! - `cargo test -p elephc-gmp` when `ELEPHC_LIBGMP_DIR` points at a built libgmp.
//!
//! Key details:
//! - This crate owns only raw big-integer behaviour. PHP class semantics,
//!   argument diagnostics, operator overloading and serialization live in the
//!   prelude so AOT and eval share one implementation.
//! - The staticlib declares `__gmpz_*` symbols without linking libgmp; the
//!   managed `gmp` catalog package supplies `libgmp.a` at final link time.

mod abi;
mod ffi;
mod mpz;
mod ops;
mod packet;
#[cfg(test)]
mod tests;

pub use abi::*;
pub use mpz::{bytes_to_hex, hex_to_bytes, Mpz};
pub use ops::*;
