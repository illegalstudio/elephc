//! Purpose:
//! Declarative eval registry entry for `base64_decode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing Base64 decode hook.

eval_builtin! {
    name: "base64_decode",
    area: String,
    params: [string],
    direct: Base64Decode,
    values: Base64Decode,
}
