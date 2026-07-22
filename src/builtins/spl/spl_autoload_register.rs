//! Purpose:
//! Home of the PHP `spl_autoload_register` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - The autoload registration is an AOT stub: all three parameters are optional
//!   and any combination of 0–3 arguments is accepted. Returns `true` always.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "spl_autoload_register",
    area: Spl,
    params: [
        callback: Mixed = DefaultSpec::Null,
        throw: Bool = DefaultSpec::Bool(true),
        prepend: Bool = DefaultSpec::Bool(false),
    ],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::SplAutoloadRegister,
    ),
    summary: "Register given function as __autoload() implementation.",
    php_manual: "https://www.php.net/manual/en/function.spl-autoload-register.php",
}
