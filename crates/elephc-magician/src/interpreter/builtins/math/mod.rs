//! Purpose:
//! Per-builtin declarations for numeric functions migrated to the eval builtin
//! registry.
//!
//! Called from:
//! - `crate::interpreter::builtins` module loading.
//!
//! Key details:
//! - Leaf files register metadata through `eval_builtin!`.
//! - Runtime helpers stay in focused support modules when direct hooks need
//!   expression evaluation.

mod abs;
mod acos;
mod asin;
mod atan;
mod atan2;
mod ceil;
mod clamp;
mod cos;
mod cosh;
mod deg2rad;
mod exp;
mod fdiv;
mod floor;
mod fmod;
mod hypot;
mod intdiv;
mod log;
mod log10;
mod log2;
mod max;
mod min;
mod mt_rand;
mod pi;
mod pow;
mod rand;
mod random_int;
mod rad2deg;
mod round;
mod runtime;
mod sin;
mod sinh;
mod sqrt;
mod tan;
mod tanh;

pub(in crate::interpreter) use runtime::*;
