//! Purpose:
//! Declarative eval registry entry for `array_rand`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-rand hook.

eval_builtin! {
    name: "array_rand",
    area: Array,
    params: [array],
    direct: ArrayRand,
    values: ArrayRand,
}
