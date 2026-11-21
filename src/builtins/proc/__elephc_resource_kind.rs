//! Purpose:
//! Home of the internal `__elephc_resource_kind` builtin.
//! Reads a resource cell's kind word, or -1 for a non-resource.
//!
//! Called from:
//! - The injected proc prelude (`crate::proc_prelude`).
//!
//! Key details:
//! - Lowered by `crate::codegen::lower_inst::builtins::io::proc_resources` on every target.

builtin! {
    contract: "__elephc_resource_kind",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcResourceKind,
    ),
}
