//! Purpose:
//! Groups all `spl`-area builtin homes into this module so the registry can
//! collect them in one place. Each submodule declares exactly one builtin via
//! `builtin!` and provides its lowering hook.
//!
//! Called from:
//! - `crate::builtins` (`mod spl;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - Add `pub mod <name>;` here for every new SPL builtin home.
//! - Pure-data builtins (no `check` hook) rely on the registry common path to
//!   infer each argument and enforce arity before falling back to the declared
//!   `returns` type.
//! - Builtins with argument-type-dependent behaviour (`spl_object_id`,
//!   `iterator_count`, etc.) supply a `check` hook that computes the return type
//!   and validates the argument types.

pub mod iterator_apply;
pub mod iterator_count;
pub mod iterator_to_array;
pub mod spl_autoload;
pub mod spl_autoload_call;
pub mod spl_autoload_extensions;
pub mod spl_autoload_functions;
pub mod spl_autoload_register;
pub mod spl_autoload_unregister;
pub mod spl_classes;
pub mod spl_object_hash;
pub mod spl_object_id;
