//! Purpose:
//! Home of the PHP `ob_implicit_flush` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The stored flag is semantically inert in elephc: terminal writes are
//!   unbuffered syscalls, so implicit flushing is always effectively on.
//! - Returns `true` like PHP 8.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "ob_implicit_flush",
    area: Io,
    params: [enable: Bool = DefaultSpec::Bool(true)],
    returns: Bool,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ObImplicitFlush,
    ),
    summary: "Turns implicit flush on/off.",
    php_manual: "function.ob-implicit-flush",
}
