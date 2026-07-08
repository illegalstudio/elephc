//! Purpose:
//! Declarative eval registry entry for `fseek`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream seek helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "fseek",
    area: Filesystem,
    params: [stream, offset, whence = EvalBuiltinDefaultValue::Int(0)],
    direct: Filesystem,
    values: Filesystem,
}
