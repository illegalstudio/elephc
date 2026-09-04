//! Purpose:
//! Home of the PHP `stream_wrapper_register` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool` and validates nothing: php THROWS on a class it cannot find, and a
//!   throw is catchable, so the refusal belongs to run time. The lowering emits it.
//! - Arguments are pre-inferred by the registry before the hook runs; the hook does NOT
//!   re-infer them.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "stream_wrapper_register",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamWrapperRegister,
    ),
}

/// Returns `Bool`, without asking whether the class exists.
///
/// A class nobody declared is a run-time `TypeError` in php, not a compile error: MEASURED on
/// `php -n` 8.5.6, `stream_wrapper_register("gw", "NoSuchClass")` throws
/// `Argument #2 ($class) must be a valid class name, NoSuchClass given`. A throw is CATCHABLE,
/// so a program that wraps the call in `try`/`catch` is valid php — and refusing it here made
/// that program uncompilable. `lower_stream_wrapper_register` raises it instead.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Bool)
}
