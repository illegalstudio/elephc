//! Purpose:
//! Home of the PHP `vprintf` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts a required `format` string and a `values` array.


builtin! {
    name: "vprintf",
    area: String,
    params: [format: Str, values: Mixed],
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Vprintf,
    ),
    summary: "Outputs a formatted string using an array of values.",
    php_manual: "https://www.php.net/manual/en/function.vprintf.php",
}
