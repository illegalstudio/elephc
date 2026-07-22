//! Purpose:
//! Home of the PHP `spl_autoload_unregister` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The AOT stub accepts exactly one callable argument and returns `true`.


builtin! {
    name: "spl_autoload_unregister",
    area: Spl,
    params: [callback: Mixed],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::SplAutoloadUnregister,
    ),
    summary: "Unregister given function as __autoload() implementation.",
    php_manual: "https://www.php.net/manual/en/function.spl-autoload-unregister.php",
}
