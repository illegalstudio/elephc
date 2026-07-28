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
//! - Pure-data builtins (no `check` hook) rely on the registry common path to
//!   infer each argument and enforce arity before falling back to the declared
//!   `returns` type.
//! - Builtins with argument-type-dependent returns (`abs`, `clamp`, `min`, `max`)
//!   supply a `check` hook that computes the precise return type.

pub mod abs;
pub mod acos;
pub mod asin;
pub mod atan;
pub mod atan2;
pub mod ceil;
pub mod clamp;
pub mod cos;
pub mod cosh;
pub mod deg2rad;
pub mod exp;
pub mod fdiv;
pub mod floor;
pub mod fmod;
pub mod hypot;
pub mod intdiv;
pub mod log;
pub mod log10;
pub mod log2;
pub mod max;
pub mod min;
pub mod mt_rand;
pub mod pi;
pub mod pow;
pub mod rad2deg;
pub mod rand;
pub mod random_bytes;
pub mod random_int;
pub mod round;
pub mod sin;
pub mod sinh;
pub mod sqrt;
pub mod tan;
pub mod tanh;
