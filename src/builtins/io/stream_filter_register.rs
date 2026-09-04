//! Purpose:
//! Home of the PHP `stream_filter_register` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool` and validates nothing: php REGISTERS a class it cannot find.
//! - Arguments are pre-inferred by the registry before the hook runs; the hook does NOT
//!   re-infer them.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "stream_filter_register",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::StreamFilterRegister,
    ),
}

/// Returns `Bool`, without asking whether the class exists.
///
/// A class nobody declared is NOT a registration error in php: MEASURED on `php -n` 8.5.6,
/// `stream_filter_register("ghost", "NoSuchClass")` answers `true`, and the failure surfaces at
/// the ATTACH, which warns `User-filter "ghost" requires class "NoSuchClass", but that class is
/// not defined` and returns false. Refusing the program here made a php script that RUNS
/// uncompilable — and a filter registered but never attached is ordinary defensive code.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Bool)
}
