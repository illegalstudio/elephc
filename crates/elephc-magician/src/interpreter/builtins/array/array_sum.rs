//! Purpose:
//! Declarative eval registry entry for `array_sum`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the array-aggregate hook.

eval_builtin! {
    name: "array_sum",
    area: Array,
    params: [array],
    direct: ArrayAggregate,
    values: ArrayAggregate,
}
