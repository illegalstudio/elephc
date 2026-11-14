//! Purpose:
//! Home of the internal `__elephc_resource_close` builtin.
//! Marks a resource cell closed (`Unknown`) without touching its native handle.
//!
//! Called from:
//! - The injected proc prelude (`crate::proc_prelude`).
//!
//! Key details:
//! - Lowered by `crate::codegen::lower_inst::builtins::io::proc_resources` on every target.

builtin! {
    contract: "__elephc_resource_close",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcResourceClose,
    ),
}
