//! Purpose:
//! Home of the PHP `get_class` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the registry common path infers the optional argument and
//!   returns the declared `Str` type.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "get_class",
    area: Callables,
    params: [object: Mixed = DefaultSpec::Null],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::GetClass,
    ),
    summary: "Returns the name of the class of an object.",
    php_manual: "function.get-class",
}
