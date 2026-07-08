//! Purpose:
//! Builtin existence helpers used by eval function probes.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Declarative specs are the source of truth for PHP-visible eval builtin names.
//! - Lookup callers pass canonical lowercase PHP symbol names.

use super::{eval_declared_builtin_exists, eval_declared_builtin_function_names};

/// Returns the eval interpreter's PHP-visible builtin names.
pub(in crate::interpreter) fn eval_php_visible_builtin_function_names() -> &'static [&'static str] {
    eval_declared_builtin_function_names()
}

/// Returns true for PHP-visible builtin names implemented by the eval interpreter.
pub(in crate::interpreter) fn eval_php_visible_builtin_exists(name: &str) -> bool {
    eval_declared_builtin_exists(name)
}
