//! Purpose:
//! Declarative eval registry entry for `stream_wrapper_register`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream wrapper registry helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_wrapper_register",
    area: Filesystem,
    params: [
        protocol,
        r#class,
        flags = EvalBuiltinDefaultValue::Int(0)
    ],
    direct: Filesystem,
    values: Filesystem,
}
