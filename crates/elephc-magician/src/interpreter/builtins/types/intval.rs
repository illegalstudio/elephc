//! Purpose:
//! Declarative eval registry entry for `intval`.
//!
//! Called from:
//! - `crate::interpreter::builtins::types`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing scalar-cast hook.

eval_builtin! {
    name: "intval",
    area: Types,
    params: [value],
    direct: Cast,
    values: Cast,
}
