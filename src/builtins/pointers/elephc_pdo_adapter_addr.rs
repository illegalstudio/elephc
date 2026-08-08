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
use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinResultOwnership,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::errors::CompileError;
use crate::ir::{Effects, Op};
use crate::types::PhpType;

builtin! {
    name: "__elephc_pdo_adapter_addr",
    area: Pointers,
    params: [kind: Int],
    returns: Ptr,
    check: check,
    semantics: internal_eir_semantics(lower, Effects::PURE, BuiltinResultOwnership::NonHeap),
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

/// Lowers one callback-adapter selector to its dedicated address-producing EIR primitive.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::PdoAdapterAddr,
        vec![call.operand(0)?],
        None,
        call.result_type.clone(),
        Op::PdoAdapterAddr.default_effects(),
        Some(call.span),
    ))
}
