//! Purpose:
//! Declarative eval registry entry for `header`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the time/system hook.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "header",
    area: Time,
    params: [
        header,
        replace = EvalBuiltinDefaultValue::Bool(true),
        response_code = EvalBuiltinDefaultValue::Int(0),
    ],
    direct: Time,
    values: Time,
}
