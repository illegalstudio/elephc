//! Purpose:
//! Home of the internal `__elephc_callable_ptr` builtin: it reinterprets a
//! closure / first-class callable value as the raw pointer to its 64-byte callable
//! descriptor. This is the PHP-prelude half of the PDO Tier-D "decompose-at-PHP"
//! callback design: a `callable` is broken into (descriptor pointer, adapter
//! address) so that no bridge extern ever declares a `callable` parameter.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//! - The PDO prelude driver methods (`Pdo\Sqlite::createCollation`, and later
//!   `createFunction` / `createAggregate`).
//!
//! Key details:
//! - `internal: true` keeps it out of PHP-visible catalogs and the parity gate while
//!   remaining callable through `registry::is_supported`.
//! - `check` returns `PhpType::Pointer(None)`; the runtime value of a closure /
//!   first-class callable already IS its descriptor pointer, so lowering is a bare
//!   identity load guarded against string / array callables (whose value is a PHP
//!   string, not a descriptor).
//! - `returns: Ptr` says the same thing in the DECLARATION, and has to. A check hook makes
//!   the declared type non-authoritative, not unused: every path that cannot name this call
//!   falls back to it. While `TypeSpec` had no pointer variant this declared `Mixed`, and the
//!   fallback handed codegen a boxed cell for a raw address.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinResultOwnership,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Op};
use crate::types::PhpType;

builtin! {
    name: "__elephc_callable_ptr",
    area: Pointers,
    params: [value: Mixed],
    returns: Ptr,
    check: check,
    semantics: internal_eir_semantics(lower, Effects::PURE, BuiltinResultOwnership::NonHeap),
    summary: "Reinterprets a closure / first-class callable as its raw descriptor pointer.",
    internal: true
}

/// Infers the argument type and returns `PhpType::Pointer(None)`.
///
/// The static callable kind (closure / first-class vs string / array) is not carried
/// by `PhpType::Callable`, so the string / array rejection happens at lowering where
/// the value's codegen type is available. The registry's `check_arity` enforces the
/// single-argument arity.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Pointer(None))
}

/// Lowers a normalized callable value to the dedicated descriptor-pointer EIR primitive.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::CallablePtr,
        vec![call.operand(0)?],
        None,
        call.result_type.clone(),
        Op::CallablePtr.default_effects(),
        Some(call.span),
    ))
}
