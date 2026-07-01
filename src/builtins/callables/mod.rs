//! Purpose:
//! Groups all `callables`-area builtin homes into this module so the registry can
//! collect them in one place. Each submodule declares exactly one builtin via
//! `builtin!` and provides its lowering hook.
//!
//! Called from:
//! - `crate::builtins` (`mod callables;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - `support` holds shared check hooks used by multiple homes to avoid duplication.
//! - Group A: no check hook (registry common path handles inference and return type).
//! - Group B: shared `check_declared_names` hook (returns `Array<Str>`).
//! - Group C: shared `check_class_like_exists` hook (requires string-literal first arg).
//! - Group E: shared `check_class_relation` hook with `lazy_check: true`.
//! - `class_alias`: always-error local check hook.
//! - `function_exists`: delegates to `callables::check_function_exists` with `lazy_check: true`.

pub(crate) mod support;

// Group A — no check hook
pub mod get_class;
pub mod get_parent_class;
pub mod is_a;
pub mod is_subclass_of;

// Group B — check_declared_names
pub mod get_declared_classes;
pub mod get_declared_interfaces;
pub mod get_declared_traits;

// Group C — check_class_like_exists
pub mod class_exists;
pub mod enum_exists;
pub mod interface_exists;
pub mod trait_exists;

// Group E — check_class_relation + lazy_check
pub mod class_implements;
pub mod class_parents;
pub mod class_uses;

// Callables batch B — lazy_check, delegates to checker::builtins::callables
pub mod call_user_func;
pub mod call_user_func_array;

// Singletons
pub mod class_alias;
pub mod function_exists;
