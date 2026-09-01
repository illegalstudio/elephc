//! Purpose:
//! Home of the internal `__elephc_print_r_object_properties` builtin used by
//! synthetic ext/date renderers to expose user-declared subclass properties.
//!
//! Called from:
//! - Synthetic date/time `__elephc_print_r_dump()` method bodies.
//!
//! Key details:
//! - The runtime walker uses the program's class-property descriptor table.
//! - The builtin stays internal because it is compiler runtime plumbing rather
//!   than part of PHP's public function surface.

builtin! {
    contract: "__elephc_print_r_object_properties",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcPrintRObjectProperties,
    ),
}
