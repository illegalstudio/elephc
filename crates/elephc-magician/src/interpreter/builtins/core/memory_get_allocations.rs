//! Purpose:
//! Joins `memory_get_allocations()` to the shared boxed runtime dispatcher.
//!
//! Called from:
//! - Magician builtin registry inventory assembly.
//!
//! Key details:
//! - Reads the same native heap as compiled PHP through `RuntimeBuiltinId`.

eval_builtin! {
    contract: "memory_get_allocations",
    area: Core,
    direct: none,
    values: none,
}
