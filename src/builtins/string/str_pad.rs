//! Purpose:
//! Home of the PHP `str_pad` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts required `string` and `length` params, plus optional `pad_string`
//!   and `pad_type` params with PHP-compatible defaults.

use crate::builtins::spec::DefaultSpec;

builtin! {
    name: "str_pad",
    area: String,
    params: [
        string: Str,
        length: Int,
        pad_string: Str = DefaultSpec::Str(" "),
        pad_type: Int = DefaultSpec::Int(1)
    ],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StrPad,
    ),
    summary: "Pads a string to a certain length with another string.",
    php_manual: "https://www.php.net/manual/en/function.str-pad.php",
}
