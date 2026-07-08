//! Purpose:
//! Declarative eval registry entry for `http_response_code`.
//!
//! Called from:
//! - `crate::interpreter::builtins::time::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the time/system hook.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "http_response_code",
    area: Time,
    params: [response_code = EvalBuiltinDefaultValue::Int(0)],
    direct: Time,
    values: Time,
}
