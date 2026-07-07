//! Purpose:
//! Declarative eval registry entry for `array_slice`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-slice hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "array_slice",
    area: Array,
    params: [array, offset, length = EvalBuiltinDefaultValue::Null],
    direct: ArraySlice,
    values: ArraySlice,
}
