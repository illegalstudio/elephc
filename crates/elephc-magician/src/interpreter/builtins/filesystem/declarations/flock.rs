//! Purpose:
//! Declarative eval registry entry for `flock`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference path.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "flock",
    area: Filesystem,
    params: [stream, operation, would_block: by_ref = EvalBuiltinDefaultValue::Null],
    by_ref: [would_block],
    direct: none,
    values: Filesystem,
}
