//! Purpose:
//! Declarative eval registry entry for `is_finite`.
//!
//! Called from:
//! - `crate::interpreter::builtins::types`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing type-predicate hook.

eval_builtin! {
    name: "is_finite",
    area: Types,
    params: [num],
    direct: TypePredicate,
    values: TypePredicate,
}
