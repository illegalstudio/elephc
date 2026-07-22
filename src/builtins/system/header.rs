//! Purpose:
//! Home of the PHP `header` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Void`) is fully determined by the declaration.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "header",
    area: System,
    params: [header: Str, replace: Bool = DefaultSpec::Bool(true), response_code: Int = DefaultSpec::Int(0)],
    returns: Void,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Header,
    ),
    summary: "Sends a raw HTTP header.",
}
