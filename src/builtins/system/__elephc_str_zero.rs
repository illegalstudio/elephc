//! Purpose:
//! Wipes a uniquely owned PHP string payload for the sodium prelude.
//!
//! Called from:
//! - `src/sodium_prelude/mutators.rs` through `sodium_memzero()`.
//!
//! Key details:
//! - The value stays owned by the caller; only its uniquely owned payload bytes are zeroed.
//! - Backend lowering delegates the target-specific heap classification to `__rt_str_zero`.

use super::core_support::core_builtin_home;
core_builtin_home!("__elephc_str_zero", StrZero);
