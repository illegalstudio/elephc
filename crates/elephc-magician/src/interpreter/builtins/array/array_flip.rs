//! Purpose:
//! Declarative eval registry entry for `array_flip`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-flip hook.

eval_builtin! {
    name: "array_flip",
    area: Array,
    params: [array],
    direct: ArrayFlip,
    values: ArrayFlip,
}
