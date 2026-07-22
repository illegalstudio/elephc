//! Purpose:
//! Home of the PHP `substr_replace` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts required `string`, `replace`, and `offset` params, plus an optional
//!   `length` param defaulting to null.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "substr_replace",
    area: String,
    params: [string: Str, replace: Str, offset: Int, length: Mixed = DefaultSpec::Null],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::SubstrReplace,
    ),
    summary: "Replaces text within a portion of a string.",
    php_manual: "https://www.php.net/manual/en/function.substr-replace.php",
}
