//! Purpose:
//! Declarative eval registry entry for `fscanf`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - The current eval implementation returns parsed values and ignores output vars.

eval_builtin! {
    name: "fscanf",
    area: Filesystem,
    params: [stream, format],
    variadic: vars,
    direct: Filesystem,
    values: Filesystem,
}
