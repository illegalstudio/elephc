//! Purpose:
//! Declarative eval registry entry for `strlen`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing string-length hook.

eval_builtin! {
    name: "strlen",
    area: String,
    params: [string],
    direct: Strlen,
    values: Strlen,
}
