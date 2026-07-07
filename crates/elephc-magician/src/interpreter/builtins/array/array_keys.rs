//! Purpose:
//! Declarative eval registry entry for `array_keys`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-projection hook.

eval_builtin! {
    name: "array_keys",
    area: Array,
    params: [array],
    direct: ArrayProjection,
    values: ArrayProjection,
}
