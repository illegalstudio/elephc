//! Purpose:
//! Builtin existence helpers used by eval function probes.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Declarative specs are the source of truth for PHP-visible eval builtin names.
//! - Lookup callers pass canonical lowercase PHP symbol names.

use super::{
    eval_declared_builtin_exists, eval_declared_builtin_function_names,
    eval_raw_declared_builtin_spec,
};

/// Returns the eval interpreter's PHP-visible builtin names.
pub(in crate::interpreter) fn eval_php_visible_builtin_function_names() -> &'static [&'static str] {
    eval_declared_builtin_function_names()
}

/// Returns true for PHP-visible builtin names implemented by the eval interpreter.
pub(in crate::interpreter) fn eval_php_visible_builtin_exists(name: &str) -> bool {
    eval_declared_builtin_exists(name)
}

/// Returns the eval builtins that are elephc extensions (no PHP equivalent),
/// in stable sorted order. Strict-PHP binaries hide exactly this set from eval
/// dispatch and introspection. Derived from the RAW registry so the snapshot
/// is independent of the thread's strict-mode state.
pub(in crate::interpreter) fn eval_extension_builtin_names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES
        .get_or_init(|| {
            eval_declared_builtin_function_names()
                .iter()
                .copied()
                .filter(|name| {
                    eval_raw_declared_builtin_spec(name)
                        .is_some_and(|spec| spec.is_extension())
                })
                .collect()
        })
        .as_slice()
}
