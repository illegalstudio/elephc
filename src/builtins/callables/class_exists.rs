//! Purpose:
//! Home of the PHP `class_exists` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The check hook validates that the first argument is a string literal and the
//!   optional autoload argument is a literal bool or int (AOT constraint).
//! - Arguments are pre-inferred by the registry common path before the hook runs.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "class_exists",
    area: Callables,
    params: [class: Str, autoload: Bool = DefaultSpec::Bool(true)],
    returns: Bool,
    check: crate::builtins::callables::support::check_class_like_exists,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ClassExists,
    ),
    summary: "Checks whether the class has been defined.",
    php_manual: "function.class-exists",
}
