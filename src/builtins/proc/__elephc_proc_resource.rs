//! Purpose:
//! Home of the internal `__elephc_proc_resource` builtin.
//! Boxes a proc-bridge process id as a `process` resource (kind 11).
//!
//! Called from:
//! - The injected proc prelude (`crate::proc_prelude`).
//!
//! Key details:
//! - Lowered by `crate::codegen::lower_inst::builtins::io::proc_resources` on every target.

builtin! {
    contract: "__elephc_proc_resource",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcProcResource,
    ),
}
