//! Purpose:
//! Groups all `types`-area builtin homes into this module so the registry can
//! collect them in one place. Each submodule declares exactly one builtin via
//! `builtin!` and provides its lowering hook.
//!
//! Called from:
//! - `crate::builtins` (`mod types;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - Add `pub mod <name>;` here for every new types builtin home.
//! - Pure-data builtins (no `check` hook) rely on the registry common path to
//!   infer each argument and enforce arity before falling back to the declared
//!   `returns` type.
//! - Builtins with validation logic (`settype`) use `lazy_check: true` so the
//!   check hook controls argument inference order.

pub mod boolval;
pub mod floatval;
pub mod get_resource_id;
pub mod get_resource_type;
pub mod gettype;
pub mod intval;
pub mod is_array;
pub mod is_bool;
pub mod is_callable;
pub mod is_finite;
pub mod is_float;
pub mod is_infinite;
pub mod is_int;
pub mod is_iterable;
pub mod is_nan;
pub mod is_null;
pub mod is_numeric;
pub mod is_object;
pub mod is_resource;
pub mod is_scalar;
pub mod is_string;
pub mod settype;
