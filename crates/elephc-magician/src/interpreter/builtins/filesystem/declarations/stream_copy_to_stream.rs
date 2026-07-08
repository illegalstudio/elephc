//! Purpose:
//! Declarative eval registry entry for `stream_copy_to_stream`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream copy helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_copy_to_stream",
    area: Filesystem,
    params: [
        from,
        to,
        length = EvalBuiltinDefaultValue::Null,
        offset = EvalBuiltinDefaultValue::Int(-1)
    ],
    direct: Filesystem,
    values: Filesystem,
}
