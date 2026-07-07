//! Purpose:
//! Declarative eval registry entry for `abs`.
//!
//! Called from:
//! - `crate::interpreter::builtins::math`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing numeric hook.

eval_builtin! {
    name: "abs",
    area: Math,
    params: [num],
    direct: Abs,
    values: Abs,
}
