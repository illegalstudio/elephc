//! Purpose:
//! Registers PHP's `method_exists` metadata lookup as a typed builtin operation.
//!
//! Called from:
//! - The builtin registry through `crate::builtins::callables`.
//!
//! Key details:
//! - Static class metadata and eval-aware lookup remain backend implementation details.

builtin! {
    name: "method_exists",
    area: Callables,
    params: [object_or_class: Mixed, method: Str],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::MethodExists,
    ),
    summary: "Checks whether a class method exists.",
    php_manual: "function.method-exists",
}
