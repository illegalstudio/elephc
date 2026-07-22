//! Purpose:
//! Home of the PHP `gzcompress` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Returns a raw string; unlike the decompress variants it never fails.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "gzcompress",
    area: String,
    params: [data: Str, level: Int = DefaultSpec::Int(-1)],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Gzcompress,
    ),
    summary: "Compress a string using the ZLIB data format.",
    php_manual: "https://www.php.net/manual/en/function.gzcompress.php",
}
