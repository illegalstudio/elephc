//! Purpose:
//! Home of the PHP `gethostbyname` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers the hostname argument and returns `Str`.


builtin! {
    name: "gethostbyname",
    area: Io,
    params: [hostname: Str],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Gethostbyname,
    ),
    summary: "Gets the IPv4 address corresponding to the given Internet host name.",
    php_manual: "function.gethostbyname",
}
