//! Purpose:
//! Declarative eval registry entry for `addslashes`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing slash escaping hook.

eval_builtin! {
    name: "addslashes",
    area: String,
    params: [string],
    direct: Slashes,
    values: Slashes,
}
