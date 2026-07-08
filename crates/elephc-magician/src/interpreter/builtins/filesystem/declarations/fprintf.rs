//! Purpose:
//! Declarative eval registry entry for `fprintf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Variadic values are formatted by the existing printf-family helper.

eval_builtin! {
    name: "fprintf",
    area: Filesystem,
    params: [stream, format],
    variadic: values,
    direct: Filesystem,
    values: Filesystem,
}
