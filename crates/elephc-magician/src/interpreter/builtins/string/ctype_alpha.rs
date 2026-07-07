//! Purpose:
//! Declarative eval registry entry for `ctype_alpha`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing ASCII ctype hook.

eval_builtin! {
    name: "ctype_alpha",
    area: String,
    params: [text],
    direct: Ctype,
    values: Ctype,
}
