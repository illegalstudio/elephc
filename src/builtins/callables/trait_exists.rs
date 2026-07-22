//! Purpose:
//! Home of the PHP `trait_exists` builtin: its single-source registry declaration and semantic target.
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
    name: "trait_exists",
    area: Callables,
    params: [trait: Str, autoload: Bool = DefaultSpec::Bool(true)],
    returns: Bool,
    check: crate::builtins::callables::support::check_class_like_exists,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::TraitExists,
    ),
    summary: "Checks whether the trait exists.",
    php_manual: "function.trait-exists",
}
