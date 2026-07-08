//! Purpose:
//! Declarative eval registry entry for `explode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the string split/join hook.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "explode",
    area: String,
    params: [separator, string, limit = EvalBuiltinDefaultValue::Int(i64::MAX)],
    direct: StringSplitJoin,
    values: StringSplitJoin,
}
