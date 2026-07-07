//! Purpose:
//! Declarative eval registry entry for `gettype`.
//!
//! Called from:
//! - `crate::interpreter::builtins::types`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing type-name hook.

eval_builtin! {
    name: "gettype",
    area: Types,
    params: [value],
    direct: Gettype,
    values: Gettype,
}
