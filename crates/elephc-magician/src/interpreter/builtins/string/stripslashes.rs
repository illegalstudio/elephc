//! Purpose:
//! Declarative eval registry entry for `stripslashes`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing slash unescaping hook.

eval_builtin! {
    name: "stripslashes",
    area: String,
    params: [string],
    direct: Slashes,
    values: Slashes,
}
