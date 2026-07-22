//! Purpose:
//! Home of the PHP `printf` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts a required `format` string plus a variadic `values` list.


builtin! {
    name: "printf",
    area: String,
    params: [format: Str],
    variadic: "values",
    returns: Int,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Printf,
    ),
    summary: "Outputs a formatted string.",
    php_manual: "https://www.php.net/manual/en/function.printf.php",
}
