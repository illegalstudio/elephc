//! Purpose:
//! Declarative eval registry entry for `range`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the integer range hook.

eval_builtin! {
    name: "range",
    area: Array,
    params: [start, end],
    direct: Range,
    values: Range,
}
