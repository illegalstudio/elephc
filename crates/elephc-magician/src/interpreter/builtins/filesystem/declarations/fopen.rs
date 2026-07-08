//! Purpose:
//! Declarative eval registry entry for `fopen`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream-opening helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "fopen",
    area: Filesystem,
    params: [
        filename,
        mode,
        use_include_path = EvalBuiltinDefaultValue::Bool(false),
        context = EvalBuiltinDefaultValue::Null
    ],
    direct: Filesystem,
    values: Filesystem,
}
