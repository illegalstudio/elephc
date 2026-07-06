//! Purpose:
//! Home of the PHP `hash_init` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `hash_init` accepts only 1 argument (the algorithm name). The `flags`/`key`
//!   parameters from the PHP golden signature are not supported: HASH_HMAC streaming
//!   mode requires passing a secret key and is blocked by `arity_error` and `max_args`.
//! - `min_args: 1, max_args: 1` enforces exactly 1 arg in `check_arity`. The custom
//!   `arity_error` message explains the HMAC streaming restriction to the caller.
//! - `check` records the elephc-crypto bridge requirement via `require_builtin_library`
//!   so the linker pulls in the hash algorithm set.
//! - Arity validation runs before the `check` hook fires.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "hash_init",
    area: String,
    params: [algo: Str, flags: Int = DefaultSpec::Int(0), key: Str = DefaultSpec::Str("")],
    min_args: 1,
    max_args: 1,
    arity_error: "hash_init() flags/HASH_HMAC streaming mode is not supported; use hash_hmac() for HMAC",
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Initialize an incremental hashing context.",
    php_manual: "https://www.php.net/manual/en/function.hash-init.php",
}

/// Records the elephc-crypto bridge requirement and returns `PhpType::Mixed`.
///
/// Arity (exactly 1 arg) is pre-validated by `check_arity`; the custom `arity_error`
/// message on the spec fires instead of the standard phrasing.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.require_builtin_library("elephc_crypto");
    Ok(PhpType::Mixed)
}

/// Lowers a `hash_init` call by delegating to the shared hash-init emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_hash_init(ctx, inst)
}
