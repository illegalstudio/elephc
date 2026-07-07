//! Purpose:
//! Declarative eval registry entry for `is_scalar`.
//!
//! Called from:
//! - `crate::interpreter::builtins::types`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing type-predicate hook.

eval_builtin! {
    name: "is_scalar",
    area: Types,
    params: [value],
    direct: TypePredicate,
    values: TypePredicate,
}
