//! Purpose:
//! Declarative eval registry entry for `in_array`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-search hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "in_array",
    area: Array,
    params: [needle, haystack, strict = EvalBuiltinDefaultValue::Bool(false)],
    direct: ArraySearch,
    values: ArraySearch,
}
