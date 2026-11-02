//! Purpose:
//! Declares the Magician binding for `gmp_abs`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Forwards to the compiled GMP prelude helper through the shared native bridge.

eval_builtin! { contract: "gmp_abs", area: Gmp, direct: Gmp, values: Gmp }
