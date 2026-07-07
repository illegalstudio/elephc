//! Purpose:
//! Declarative eval registry entry for `count`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing count hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "count",
    area: Array,
    params: [value, mode = EvalBuiltinDefaultValue::Int(0)],
    direct: Count,
    values: Count,
}
