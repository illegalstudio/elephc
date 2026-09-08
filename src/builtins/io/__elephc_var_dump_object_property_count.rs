//! Purpose:
//! Home of the internal `__elephc_var_dump_object_property_count` builtin used
//! by synthetic ext/date renderers to compute their visible property count.
//!
//! Called from:
//! - Synthetic date/time `__elephc_debug_dump()` method bodies.
//!
//! Key details:
//! - Uninitialized typed properties are excluded exactly like the runtime walker.
//! - The builtin stays internal because it is compiler runtime plumbing.

builtin! {
    contract: "__elephc_var_dump_object_property_count",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcVarDumpObjectPropertyCount,
    ),
}
