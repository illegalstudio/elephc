//! Purpose:
//! Declarative eval registry entry for `is_infinite`.
//!
//! Called from:
//! - `crate::interpreter::builtins::types`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing type-predicate hook.

eval_builtin! {
    name: "is_infinite",
    area: Types,
    params: [num],
    direct: TypePredicate,
    values: TypePredicate,
}
