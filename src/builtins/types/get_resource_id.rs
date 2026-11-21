//! Purpose:
//! Home of the PHP `get_resource_id` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - The parameter is named `resource` (matching the PHP golden signature).


builtin! {
    contract: "get_resource_id",
    semantics: crate::builtins::mixed_inputs::with_domains(
        crate::builtins::semantics::runtime_fn_semantics(
            crate::ir::RuntimeFnId::GetResourceId,
        ),
        crate::builtins::mixed_inputs::first_resource,
    ),
}
