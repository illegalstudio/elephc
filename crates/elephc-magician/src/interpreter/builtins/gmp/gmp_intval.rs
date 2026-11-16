//! Purpose:
//! Declares the Magician binding for `gmp_intval`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Forwards to the compiled GMP prelude helper through the shared native bridge.

eval_builtin! { contract: "gmp_intval", area: Gmp, direct: Gmp, values: Gmp }
