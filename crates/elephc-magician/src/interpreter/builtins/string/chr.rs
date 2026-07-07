//! Purpose:
//! Declarative eval registry entry for `chr`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing byte-string hook.

eval_builtin! {
    name: "chr",
    area: String,
    params: [codepoint],
    direct: Chr,
    values: Chr,
}
