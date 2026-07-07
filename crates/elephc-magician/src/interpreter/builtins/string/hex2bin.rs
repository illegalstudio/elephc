//! Purpose:
//! Declarative eval registry entry for `hex2bin`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing hex decode hook.

eval_builtin! {
    name: "hex2bin",
    area: String,
    params: [string],
    direct: Hex2Bin,
    values: Hex2Bin,
}
