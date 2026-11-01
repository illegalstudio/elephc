//! Purpose:
//! Home of PHP's `header_remove` builtin and its web response-state semantics.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - An omitted name clears all pending response headers; a supplied name removes matching headers
//!   case-insensitively without changing the response status.

builtin! {
    contract: "header_remove",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HeaderRemove,
    ),
}
