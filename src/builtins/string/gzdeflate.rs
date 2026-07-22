//! Purpose:
//! Home of the PHP `gzdeflate` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Returns a raw string; unlike the inflate variant it never fails with false.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "gzdeflate",
    area: String,
    params: [data: Str, level: Int = DefaultSpec::Int(-1)],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Gzdeflate,
    ),
    summary: "Deflate a string using the DEFLATE data format.",
    php_manual: "https://www.php.net/manual/en/function.gzdeflate.php",
}
