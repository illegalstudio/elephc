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
//! - `min_max_array_element_type()` is the one shared helper here: `min` and `max`
//!   accept the same single-array form, so both home files delegate to it.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

pub mod abs;
pub mod acos;
pub mod asin;
pub mod atan;
pub mod atan2;
pub mod base_convert;
pub mod bcadd;
pub mod bcceil;
pub mod bccomp;
pub mod bcdiv;
pub mod bcdivmod;
pub mod bcfloor;
pub mod bcmod;
pub mod bcmul;
pub mod bcpow;
pub mod bcpowmod;
pub mod bcround;
pub mod bcscale;
pub mod bcsqrt;
pub mod bcsub;
pub mod bindec;
pub mod ceil;
pub mod clamp;
pub mod cos;
pub mod cosh;
pub mod decbin;
pub mod dechex;
pub mod decoct;
pub mod deg2rad;
pub mod exp;
pub mod fdiv;
pub mod floor;
pub mod fmod;
pub mod getrandmax;
pub mod hexdec;
pub mod hypot;
pub mod intdiv;
pub mod log;
pub mod log10;
pub mod log2;
pub mod max;
pub mod min;
pub mod mt_rand;
pub mod octdec;
pub mod pi;
pub mod pow;
pub mod rad2deg;
pub mod rand;
pub mod random_int;
pub mod round;
pub mod sin;
pub mod sinh;
pub mod sqrt;
pub mod tan;
pub mod tanh;

/// Resolves the result type of the single-argument `min()` / `max()` form.
///
/// PHP's one-argument form takes an array and returns one of its elements, so the
/// call's result type is the array's element type. A non-array argument is PHP's
/// `min(): Argument #1 ($value) must be of type array, <type> given` TypeError;
/// elephc reports it at compile time because the argument type is already known.
pub(crate) fn min_max_array_element_type(
    cx: &mut BuiltinCheckCtx,
    name: &str,
) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    match ty {
        PhpType::Array(element) => Ok(*element),
        PhpType::AssocArray { value, .. } => Ok(*value),
        other => Err(CompileError::new(
            cx.span,
            &format!(
                "{}(): Argument #1 ($value) must be of type array, {} given",
                name, other
            ),
        )),
    }
}
