//! Purpose:
//! Home of the PHP `get_resource_type` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - The parameter is named `resource` (matching the PHP golden signature).


builtin! {
    name: "get_resource_type",
    area: Types,
    params: [resource: Mixed],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::GetResourceType,
    ),
    summary: "Returns the type of a resource.",
    php_manual: "function.get-resource-type",
}
