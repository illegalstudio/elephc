//! Purpose:
//! Declarative eval registry entry for `is_nan`.
//!
//! Called from:
//! - `crate::interpreter::builtins::types`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing type-predicate hook.

eval_builtin! {
    name: "is_nan",
    area: Types,
    params: [num],
    direct: TypePredicate,
    values: TypePredicate,
}
