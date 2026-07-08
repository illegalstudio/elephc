//! Purpose:
//! Declarative eval registry entry for `stream_context_create`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream context helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_context_create",
    area: Filesystem,
    params: [
        options = EvalBuiltinDefaultValue::Null,
        params = EvalBuiltinDefaultValue::Null
    ],
    direct: Filesystem,
    values: Filesystem,
}
