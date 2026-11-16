//! Purpose:
//! Joins `memory_get_usage()` to the shared boxed runtime dispatcher.
//!
//! Called from:
//! - Magician builtin registry inventory assembly.
//!
//! Key details:
//! - Scalar samples use `RuntimeBuiltinId`; invalid flags use the Core TypeError adapter.

eval_builtin! {
    contract: "memory_get_usage",
    area: Core,
    direct: Core,
    values: Core,
}
