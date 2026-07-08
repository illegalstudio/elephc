//! Purpose:
//! Per-builtin declarations for scalar type and conversion functions migrated
//! to the eval builtin registry.
//!
//! Called from:
//! - `crate::interpreter::builtins` module loading.
//!
//! Key details:
//! - Leaf files register metadata through `eval_builtin!`.

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
