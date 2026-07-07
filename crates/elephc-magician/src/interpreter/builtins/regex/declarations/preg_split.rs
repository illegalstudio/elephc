//! Purpose:
//! Declarative eval registry entry for `preg_split`.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the split hook.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "preg_split",
    area: Regex,
    params: [
        pattern,
        subject,
        limit = EvalBuiltinDefaultValue::Int(-1),
        flags = EvalBuiltinDefaultValue::Int(0),
    ],
    direct: Regex,
    values: Regex,
}
