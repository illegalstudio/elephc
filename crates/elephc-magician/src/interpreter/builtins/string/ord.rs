//! Purpose:
//! Declarative eval registry entry for `ord`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing byte introspection hook.

eval_builtin! {
    name: "ord",
    area: String,
    params: [character],
    direct: Ord,
    values: Ord,
}
