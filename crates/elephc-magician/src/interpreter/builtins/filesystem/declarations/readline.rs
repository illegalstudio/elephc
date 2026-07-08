//! Purpose:
//! Declarative eval registry entry for `readline`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the host stdin helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "readline",
    area: Filesystem,
    params: [prompt = EvalBuiltinDefaultValue::Null],
    direct: Filesystem,
    values: Filesystem,
}
