//! Purpose:
//! Home of the PHP `vsprintf` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts a required `format` string and a `values` array.


builtin! {
    name: "vsprintf",
    area: String,
    params: [format: Str, values: Mixed],
    returns: Str,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Vsprintf,
    ),
    summary: "Returns a formatted string using an array of values.",
    php_manual: "https://www.php.net/manual/en/function.vsprintf.php",
}
