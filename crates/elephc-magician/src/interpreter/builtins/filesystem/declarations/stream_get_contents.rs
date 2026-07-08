//! Purpose:
//! Declarative eval registry entry for `stream_get_contents`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the bounded stream read helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_get_contents",
    area: Filesystem,
    params: [
        stream,
        length = EvalBuiltinDefaultValue::Null,
        offset = EvalBuiltinDefaultValue::Int(-1)
    ],
    direct: Filesystem,
    values: Filesystem,
}
