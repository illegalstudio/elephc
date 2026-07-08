//! Purpose:
//! Declarative eval registry entry for `array_splice`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "array_splice",
    area: Array,
    params: [
        array: by_ref,
        offset,
        length = EvalBuiltinDefaultValue::Null,
        replacement = EvalBuiltinDefaultValue::EmptyArray,
    ],
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
