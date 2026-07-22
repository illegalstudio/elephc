//! Purpose:
//! Home of the PHP `spl_autoload` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts 1 required argument (`class`) and 1 optional argument (`file_extensions`).
//! - The AOT stub evaluates arguments for side effects and returns void.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "spl_autoload",
    area: Spl,
    params: [class: Mixed, file_extensions: Mixed = DefaultSpec::Null],
    returns: Void,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::SplAutoload,
    ),
    summary: "Default implementation for __autoload().",
    php_manual: "https://www.php.net/manual/en/function.spl-autoload.php",
}
