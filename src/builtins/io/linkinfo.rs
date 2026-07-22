//! Purpose:
//! Home of the PHP `linkinfo` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `linkinfo` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.


builtin! {
    name: "linkinfo",
    area: Io,
    params: [path: Str],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Linkinfo,
    ),
    summary: "Gets information about a link.",
    php_manual: "function.linkinfo",
}
