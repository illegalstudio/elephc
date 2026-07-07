//! Purpose:
//! Declarative eval registry entry for `umask`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the process umask helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "umask",
    area: Filesystem,
    params: [mask = EvalBuiltinDefaultValue::Null],
    direct: Filesystem,
    values: Filesystem,
}
