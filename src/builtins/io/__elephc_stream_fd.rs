//! Purpose:
//! Home of the internal `__elephc_stream_fd` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - This is an internal builtin (`internal: true`), not a PHP-visible function. The
//!   `proc_open()` prelude (`crate::proc_prelude`) reads the native descriptor behind a stream
//!   resource through it, to hand that descriptor to the child process.
//! - Every member of the prelude calls it, so its `check` hook is where the `proc_*` family is
//!   refused on targets whose sandbox forbids spawning a process.
//! - The result is a plain integer, so nothing it returns keeps the stream alive.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "__elephc_stream_fd",
    check: check,
    semantics: crate::builtins::semantics::host_only_runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcStreamFd,
    ),
}

/// Refuses the `proc_*` family where spawning is forbidden; the result is otherwise an `int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if cx.checker.target.forbids_process_spawn() {
        return Err(CompileError::new(
            cx.span,
            &format!(
                "proc_open() and its family cannot be compiled for {}: that sandbox forbids \
                 spawning a process, so the call would always fail at run time",
                cx.checker.target.as_str()
            ),
        ));
    }
    Ok(PhpType::Int)
}
