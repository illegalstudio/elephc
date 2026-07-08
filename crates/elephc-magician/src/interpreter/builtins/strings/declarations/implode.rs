//! Purpose:
//! Declarative eval registry entry for `implode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the string split/join hook.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "implode",
    area: String,
    params: [separator = EvalBuiltinDefaultValue::Null, array],
    required: 1,
    direct: StringSplitJoin,
    values: StringSplitJoin,
}
