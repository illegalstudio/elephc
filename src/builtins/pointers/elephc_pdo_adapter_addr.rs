//! Purpose:
//! Home of the internal `__elephc_pdo_adapter_addr` builtin: it materializes the
//! address of a shared codegen PDO callback adapter (`__rt_pdo_*`) selected by a
//! constant kind. This is the second half of the PDO Tier-D "decompose-at-PHP"
//! design — the prelude hands the bridge (descriptor pointer, adapter address) as
//! two plain `ptr` arguments, and the bridge calls the adapter back with the
//! database-provided values without ever referencing a `__rt_*` symbol itself.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//! - The PDO prelude driver methods (`Pdo\Sqlite::createCollation`, and later
//!   `createFunction` / `createAggregate`).
//!
//! Key details:
//! - `internal: true` keeps it out of PHP-visible catalogs and the parity gate.
//! - `check` returns `PhpType::Pointer(None)`; lowering reads the constant kind and
//!   emits the GOT address of the corresponding `__rt_pdo_*` adapter (kind 0 =
//!   collation).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "__elephc_pdo_adapter_addr",
    area: Internal,
    params: [kind: Int],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns the address of the shared __rt_pdo_* callback adapter for a kind.",
    internal: true
}

/// Validates that the kind argument is integer-compatible and returns the pointer type.
///
/// The registry's `check_arity` enforces the single-argument arity; the kind must be
/// a constant integer literal, which the lowering hook re-validates.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let kind_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(kind_ty, PhpType::Int | PhpType::Mixed | PhpType::Union(_)) {
        return Err(CompileError::new(
            cx.span,
            "__elephc_pdo_adapter_addr() argument must be an integer kind",
        ));
    }
    Ok(PhpType::Pointer(None))
}

/// Lowers a `__elephc_pdo_adapter_addr` call by dispatching to the shared pointer emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::pointers::lower_elephc_pdo_adapter_addr(ctx, inst)
}
