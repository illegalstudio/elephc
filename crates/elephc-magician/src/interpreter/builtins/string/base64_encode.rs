//! Purpose:
//! Declarative eval registry entry for `base64_encode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing Base64 encode hook.

eval_builtin! {
    name: "base64_encode",
    area: String,
    params: [string],
    direct: Base64Encode,
    values: Base64Encode,
}
