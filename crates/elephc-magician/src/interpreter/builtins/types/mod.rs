//! Purpose:
//! Per-builtin eval registry entries and implementations for scalar type and
//! conversion functions.
//!
//! Called from:
//! - `crate::interpreter::builtins` module loading.
//!
//! Key details:
//! - Leaf files register metadata through `eval_builtin!` and own their
//!   PHP-visible direct/by-value wrappers.

mod boolval;
mod floatval;
mod gettype;
mod intval;
mod is_array;
mod is_bool;
mod is_double;
mod is_finite;
mod is_float;
mod is_infinite;
mod is_int;
mod is_integer;
mod is_iterable;
mod is_long;
mod is_nan;
mod is_null;
mod is_numeric;
mod is_object;
mod is_real;
mod is_resource;
mod is_scalar;
mod is_string;
mod settype;
mod strval;

pub(in crate::interpreter) use boolval::*;
pub(in crate::interpreter) use floatval::*;
pub(in crate::interpreter) use gettype::*;
pub(in crate::interpreter) use intval::*;
pub(in crate::interpreter) use is_array::*;
pub(in crate::interpreter) use is_bool::*;
pub(in crate::interpreter) use is_double::*;
pub(in crate::interpreter) use is_finite::*;
pub(in crate::interpreter) use is_float::*;
pub(in crate::interpreter) use is_infinite::*;
pub(in crate::interpreter) use is_int::*;
pub(in crate::interpreter) use is_integer::*;
pub(in crate::interpreter) use is_iterable::*;
pub(in crate::interpreter) use is_long::*;
pub(in crate::interpreter) use is_nan::*;
pub(in crate::interpreter) use is_null::*;
pub(in crate::interpreter) use is_numeric::*;
pub(in crate::interpreter) use is_object::*;
pub(in crate::interpreter) use is_real::*;
pub(in crate::interpreter) use is_resource::*;
pub(in crate::interpreter) use is_scalar::*;
pub(in crate::interpreter) use is_string::*;
pub(in crate::interpreter) use settype::*;
pub(in crate::interpreter) use strval::*;
