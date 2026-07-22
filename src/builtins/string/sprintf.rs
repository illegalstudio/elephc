//! Purpose:
//! Home of the PHP `sprintf` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts a required `format` string plus a variadic `values` list.


builtin! {
    name: "sprintf",
    area: String,
    params: [format: Str],
    variadic: "values",
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Sprintf,
    ),
    summary: "Returns a formatted string.",
    php_manual: "https://www.php.net/manual/en/function.sprintf.php",
}
