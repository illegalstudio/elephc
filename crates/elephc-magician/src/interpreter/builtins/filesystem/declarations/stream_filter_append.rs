//! Purpose:
//! Declarative eval registry entry for `stream_filter_append`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream filter attachment helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_filter_append",
    area: Filesystem,
    params: [
        stream,
        filtername,
        read_write = EvalBuiltinDefaultValue::Int(3),
        params = EvalBuiltinDefaultValue::Null
    ],
    direct: Filesystem,
    values: Filesystem,
}
