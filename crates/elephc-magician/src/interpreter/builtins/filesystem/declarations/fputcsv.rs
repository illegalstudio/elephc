//! Purpose:
//! Declarative eval registry entry for `fputcsv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the CSV stream write helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "fputcsv",
    area: Filesystem,
    params: [
        stream,
        fields,
        separator = EvalBuiltinDefaultValue::String(","),
        enclosure = EvalBuiltinDefaultValue::String("\"")
    ],
    direct: Filesystem,
    values: Filesystem,
}
