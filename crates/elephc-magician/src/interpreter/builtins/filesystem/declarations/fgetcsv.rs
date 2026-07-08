//! Purpose:
//! Declarative eval registry entry for `fgetcsv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the CSV stream read helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "fgetcsv",
    area: Filesystem,
    params: [
        stream,
        length = EvalBuiltinDefaultValue::Null,
        separator = EvalBuiltinDefaultValue::String(",")
    ],
    direct: Filesystem,
    values: Filesystem,
}
