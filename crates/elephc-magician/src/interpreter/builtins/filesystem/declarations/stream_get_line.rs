//! Purpose:
//! Declarative eval registry entry for `stream_get_line`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the delimiter-aware stream line helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_get_line",
    area: Filesystem,
    params: [stream, length, ending = EvalBuiltinDefaultValue::String("")],
    direct: Filesystem,
    values: Filesystem,
}
