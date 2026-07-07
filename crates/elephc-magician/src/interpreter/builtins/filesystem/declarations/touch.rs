//! Purpose:
//! Declarative eval registry entry for `touch`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the timestamp mutation helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "touch",
    area: Filesystem,
    params: [
        filename,
        mtime = EvalBuiltinDefaultValue::Null,
        atime = EvalBuiltinDefaultValue::Null
    ],
    direct: Filesystem,
    values: Filesystem,
}
