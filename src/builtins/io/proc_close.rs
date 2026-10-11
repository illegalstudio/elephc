//! Purpose:
//! Declares PHP's `proc_close` builtin for the typed EIR runtime-call path.
//!
//! Called from:
//! - The builtin registry, type checker, and EIR runtime-call dispatcher.
//!
//! Key details:
//! - The target runtime closes registered parent pipes before waiting, avoiding
//!   deadlock when an unread child pipe reaches capacity.
//! - The lowerer consumes the kind-5 process resource and stamps its release
//!   sentinel so scope cleanup cannot wait or close the process twice.

builtin! {
    contract: "proc_close",
    semantics: crate::builtins::semantics::host_only_runtime_fn_semantics(
        crate::ir::RuntimeFnId::ProcClose,
    ),
}
