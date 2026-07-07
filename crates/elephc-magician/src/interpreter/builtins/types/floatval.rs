//! Purpose:
//! Declarative eval registry entry for `floatval`.
//!
//! Called from:
//! - `crate::interpreter::builtins::types`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing scalar-cast hook.

eval_builtin! {
    name: "floatval",
    area: Types,
    params: [value],
    direct: Cast,
    values: Cast,
}
