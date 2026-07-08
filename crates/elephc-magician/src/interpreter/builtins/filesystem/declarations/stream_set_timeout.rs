//! Purpose:
//! Declarative eval registry entry for `stream_set_timeout`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream timeout-setting helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_set_timeout",
    area: Filesystem,
    params: [stream, seconds, microseconds = EvalBuiltinDefaultValue::Int(0)],
    direct: Filesystem,
    values: Filesystem,
}
