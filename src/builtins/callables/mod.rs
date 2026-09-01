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
pub mod get_object_vars;
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
pub mod method_exists;
pub mod preg_replace_callback;
pub mod property_exists;

// Internal object-introspection aliases used by the injected `var_export`
// prelude. They have no PHP-visible counterpart to alias: PHP would use
// `get_object_vars()` / `$v instanceof UnitEnum`, neither of which elephc can
// express for a runtime `mixed` today.
#[allow(non_snake_case)]
pub mod __elephc_object_clone_internal;
#[allow(non_snake_case)]
pub mod __elephc_object_is_enum;
#[allow(non_snake_case)]
pub mod __elephc_object_prop_count;
#[allow(non_snake_case)]
pub mod __elephc_object_prop_name;
#[allow(non_snake_case)]
pub mod __elephc_object_prop_value;
