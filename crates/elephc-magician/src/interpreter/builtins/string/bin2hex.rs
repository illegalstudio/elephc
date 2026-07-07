//! Purpose:
//! Declarative eval registry entry for `bin2hex`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing hex encode hook.

eval_builtin! {
    name: "bin2hex",
    area: String,
    params: [string],
    direct: Bin2Hex,
    values: Bin2Hex,
}
