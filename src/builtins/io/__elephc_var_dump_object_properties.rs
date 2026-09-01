//! Purpose:
//! Home of the internal `__elephc_var_dump_object_properties` builtin used by
//! synthetic ext/date renderers to expose user-declared subclass properties.
//!
//! Called from:
//! - Synthetic date/time `__elephc_debug_dump()` method bodies.
//!
//! Key details:
//! - The runtime walker consumes the program's filtered var_dump descriptor.
//! - The builtin stays internal because it is compiler runtime plumbing.

builtin! {
    contract: "__elephc_var_dump_object_properties",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcVarDumpObjectProperties,
    ),
}
