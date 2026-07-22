//! Purpose:
//! Home of the PHP `http_response_code` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin: return type (`Int`) is fully determined by the declaration.
//! - `arity_error` overrides the default "takes at most 1 argument" message to match
//!   the legacy phrasing "takes 0 or 1 arguments".

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "http_response_code",
    area: System,
    params: [response_code: Int = DefaultSpec::Int(0)],
    arity_error: "http_response_code() takes 0 or 1 arguments",
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HttpResponseCode,
    ),
    summary: "Gets or sets the HTTP response code.",
}
