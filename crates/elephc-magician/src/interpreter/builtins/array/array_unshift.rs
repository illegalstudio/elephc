//! Purpose:
//! Declarative eval registry entry for `array_unshift`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

eval_builtin! {
    name: "array_unshift",
    area: Array,
    params: [array: by_ref],
    variadic: values,
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
