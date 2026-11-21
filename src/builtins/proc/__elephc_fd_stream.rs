//! Purpose:
//! Home of the internal `__elephc_fd_stream` builtin.
//! Boxes a proc-bridge pipe descriptor as an ordinary stream resource.
//!
//! Called from:
//! - The injected proc prelude (`crate::proc_prelude`).
//!
//! Key details:
//! - Lowered by `crate::codegen::lower_inst::builtins::io::proc_resources` on every target.

builtin! {
    contract: "__elephc_fd_stream",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcFdStream,
    ),
}
