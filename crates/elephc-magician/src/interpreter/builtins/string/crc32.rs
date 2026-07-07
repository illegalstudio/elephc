//! Purpose:
//! Declarative eval registry entry for `crc32`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing checksum hook.

eval_builtin! {
    name: "crc32",
    area: String,
    params: [string],
    direct: Crc32,
    values: Crc32,
}
