//! Purpose:
//! Groups all `math`-area builtin homes into this module so the registry can
//! collect them in one place. Each submodule declares exactly one builtin via
//! `builtin!` and provides its lowering hook.
//!
//! Called from:
//! - `crate::builtins` (`mod math;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - Add `pub mod <name>;` here for every new math builtin home.
//! - These are pure-data builtins: no `check` hook is needed because the
//!   registry common path infers each argument and enforces arity before
//!   falling back to the declared `returns: Float`.

pub mod acos;
pub mod asin;
pub mod atan;
pub mod ceil;
pub mod cos;
pub mod cosh;
pub mod deg2rad;
pub mod exp;
pub mod floor;
pub mod log10;
pub mod log2;
pub mod rad2deg;
pub mod sin;
pub mod sinh;
pub mod sqrt;
pub mod tan;
pub mod tanh;
