//! Purpose:
//! Declarative eval registry entry for `boolval`.
//!
//! Called from:
//! - `crate::interpreter::builtins::types`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing scalar-cast hook.

eval_builtin! {
    name: "boolval",
    area: Types,
    params: [value],
    direct: Boolval,
    values: Boolval,
}
