//! Purpose:
//! Declarative eval registry entry for `strval`.
//!
//! Called from:
//! - `crate::interpreter::builtins::types`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing scalar-cast hook.

eval_builtin! {
    name: "strval",
    area: Types,
    params: [value],
    direct: Cast,
    values: Cast,
}
