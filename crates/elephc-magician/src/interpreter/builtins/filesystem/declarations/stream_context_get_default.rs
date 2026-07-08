//! Purpose:
//! Declarative eval registry entry for `stream_context_get_default`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the default stream context helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_context_get_default",
    area: Filesystem,
    params: [options = EvalBuiltinDefaultValue::Null],
    direct: Filesystem,
    values: Filesystem,
}
