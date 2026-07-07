//! Purpose:
//! Declarative eval registry entry for `array_pad`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-pad hook.

eval_builtin! {
    name: "array_pad",
    area: Array,
    params: [array, length, value],
    direct: ArrayPad,
    values: ArrayPad,
}
