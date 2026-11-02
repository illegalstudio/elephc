//! Purpose:
//! Home of the internal `__elephc_fd_to_stream` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - This is an internal builtin (`internal: true`), not a PHP-visible function. The
//!   `proc_open()` prelude (`crate::proc_prelude`) wraps the parent end of each pipe, and the
//!   process handle's token descriptor, as stream resources through it.
//! - `check` types the result as the stream resource type (or `false` for a negative
//!   descriptor), the same union `fopen()` answers, so stream builtins accept it directly.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "__elephc_fd_to_stream",
    check: check,
    semantics: crate::builtins::semantics::host_only_runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcFdToStream,
    ),
}

/// Returns `Union(stream_resource, Bool)`: a negative descriptor boxes `false`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::stream_resource(),
        PhpType::Bool,
    ]))
}
