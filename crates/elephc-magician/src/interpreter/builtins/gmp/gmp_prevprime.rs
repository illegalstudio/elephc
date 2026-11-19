//! Purpose:
//! Declares the Magician binding for `gmp_prevprime`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Forwards to the compiled GMP prelude helper through the shared native bridge; the
//!   helper owns the `$definitely_prime` write because it validates before assigning.

eval_builtin! { contract: "gmp_prevprime", area: Gmp, direct: Gmp, values: Gmp }
