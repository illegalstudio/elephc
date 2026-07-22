//! Purpose:
//! Home of the PHP `tempnam` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `tempnam` is a pure-data builtin whose `Str` return type is
//!   fully determined by its declaration. The registry common path infers the
//!   arguments and enforces the exactly-2-argument arity before falling back to
//!   `returns`.


builtin! {
    name: "tempnam",
    area: Io,
    params: [directory: Str, prefix: Str],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Tempnam,
    ),
    summary: "Creates a file with a unique filename.",
    php_manual: "function.tempnam",
}
