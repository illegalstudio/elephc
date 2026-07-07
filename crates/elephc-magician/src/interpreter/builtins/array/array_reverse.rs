//! Purpose:
//! Declarative eval registry entry for `array_reverse`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-reverse hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "array_reverse",
    area: Array,
    params: [array, preserve_keys = EvalBuiltinDefaultValue::Bool(false)],
    direct: ArrayReverse,
    values: ArrayReverse,
}
