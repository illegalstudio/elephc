//! Purpose:
//! Declarative eval registry entry for `array_search`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-search hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "array_search",
    area: Array,
    params: [needle, haystack, strict = EvalBuiltinDefaultValue::Bool(false)],
    direct: ArraySearch,
    values: ArraySearch,
}
