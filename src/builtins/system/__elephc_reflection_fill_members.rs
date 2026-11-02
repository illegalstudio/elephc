//! Purpose:
//! Declares the internal builtin that fills a compiled reflector's member reflectors -- its
//! methods, properties, constants, constructor, parent and enum cases -- the first time a
//! getter needs them.
//!
//! Called from:
//! - The synthesized `ReflectionClass` / `ReflectionObject` / `ReflectionEnum` getters that
//!   read a member-reflector slot (see `reflection_member_fill_preamble`).
//!
//! Key details:
//! - A compiled class reflector records its class's shared filler ADDRESS in
//!   `__members_filler` instead of building every member reflector up front. Building them
//!   eagerly allocated some twenty objects for `new ReflectionClass(P::class)`, which php never
//!   builds unless asked, so every later object handle (`#23` for php's `#2`) drifted.
//! - The getter clears the slot before calling, so the filler runs at most once per reflector.
//! - `internal: true` keeps this compiler primitive out of PHP-visible builtin catalogs.

use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinResultOwnership,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, Op};
use crate::types::PhpType;

builtin! {
    contract: "__elephc_reflection_fill_members",
    check: check,
    semantics: internal_eir_semantics(
        lower,
        Effects::READS_HEAP.union(Effects::WRITES_HEAP).union(Effects::ALLOC_HEAP),
        BuiltinResultOwnership::NonHeap,
    ),
}

/// Checks the filler address and the reflector it fills; the call has no value.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    Ok(PhpType::Void)
}

/// Lowers the call to the dedicated EIR primitive that runs the recorded filler on the reflector.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    Ok(ctx.emit_value(
        Op::ReflectionFillMembers,
        vec![call.operand(0)?, call.operand(1)?],
        None,
        PhpType::Void,
        Op::ReflectionFillMembers.default_effects(),
        Some(call.span),
    ))
}
