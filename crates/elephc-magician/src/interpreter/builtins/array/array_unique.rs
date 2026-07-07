//! Purpose:
//! Declarative eval registry entry for `array_unique`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-unique hook.

eval_builtin! {
    name: "array_unique",
    area: Array,
    params: [array],
    direct: ArrayUnique,
    values: ArrayUnique,
}
