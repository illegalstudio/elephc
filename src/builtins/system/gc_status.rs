//! Purpose:
//! Registers PHP `gc_status()` and builds its PHP 8 status-array shape in EIR.
//!
//! Called from:
//! - The builtin registry through `crate::builtins::system`.
//!
//! Key details:
//! - Collector counters and timings come from typed GC controls backed by live runtime state.
//! - The associative array is boxed as Mixed so direct and callable wrappers share one ABI.

use crate::builtins::semantics::{
    callable_accepts_any_source, BuiltinArgumentLowering, BuiltinCallablePolicy,
    BuiltinEffects, BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError,
    BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType, BuiltinRuntimeFunctions,
    BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport, BuiltinValidation,
    LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, GcControlOp, Immediate, Op};
use crate::span::Span;
use crate::types::PhpType;

builtin! {
    contract: "gc_status",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(status_effects()),
        result_ownership: BuiltinResultOwnership::Fresh,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Dynamic(callable_accepts_any_source),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Returns the aggregate effects of status reads, array allocation, boxing, and inserts.
const fn status_effects() -> Effects {
    Effects::from_bits_retain(
        Effects::READS_GLOBAL.bits()
            | Effects::READS_HEAP.bits()
            | Effects::READS_PROCESS.bits()
            | Effects::WRITES_HEAP.bits()
            | Effects::ALLOC_HEAP.bits()
            | Effects::REFCOUNT_OP.bits(),
    )
}

/// Builds the complete PHP 8 collector status array and boxes it for the declared Mixed result.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let hash_type = PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(12)),
        hash_type,
        Op::HashNew.default_effects(),
        Some(call.span),
    );

    let running = emit_metric(ctx, GcControlOp::Running, PhpType::Bool, call.span);
    insert_status_entry(ctx, hash.value, "running", running, call.span);
    let protected = emit_metric(ctx, GcControlOp::Protected, PhpType::Bool, call.span);
    insert_status_entry(ctx, hash.value, "protected", protected, call.span);
    let full = emit_bool(ctx, false, call.span);
    insert_status_entry(ctx, hash.value, "full", full, call.span);
    let runs = emit_metric(ctx, GcControlOp::Runs, PhpType::Int, call.span);
    insert_status_entry(ctx, hash.value, "runs", runs, call.span);
    let collected = emit_metric(ctx, GcControlOp::Collected, PhpType::Int, call.span);
    insert_status_entry(ctx, hash.value, "collected", collected, call.span);
    let threshold = emit_int(ctx, 0, call.span);
    insert_status_entry(ctx, hash.value, "threshold", threshold, call.span);
    let buffer_size = emit_int(ctx, 0, call.span);
    insert_status_entry(ctx, hash.value, "buffer_size", buffer_size, call.span);
    let roots = emit_metric(ctx, GcControlOp::Roots, PhpType::Int, call.span);
    insert_status_entry(ctx, hash.value, "roots", roots, call.span);
    for (key, op) in [
        ("application_time", GcControlOp::ApplicationTime),
        ("collector_time", GcControlOp::CollectorTime),
        ("destructor_time", GcControlOp::DestructorTime),
        ("free_time", GcControlOp::FreeTime),
    ] {
        let value = emit_metric(ctx, op, PhpType::Float, call.span);
        insert_status_entry(ctx, hash.value, key, value, call.span);
    }

    Ok(ctx.emit_value(
        Op::MixedBox,
        vec![hash.value],
        None,
        call.result_type.clone(),
        Op::MixedBox.default_effects(),
        Some(call.span),
    ))
}

/// Emits one dynamic scalar collector metric.
fn emit_metric(
    ctx: &mut dyn BuiltinLoweringContext,
    op: GcControlOp,
    php_type: PhpType,
    span: Span,
) -> LoweredBuiltinValue {
    ctx.emit_value(
        Op::GcControl,
        Vec::new(),
        Some(Immediate::I64(op.as_i64())),
        php_type,
        op.effects(),
        Some(span),
    )
}

/// Emits one integer literal used by the stable GC status schema.
fn emit_int(
    ctx: &mut dyn BuiltinLoweringContext,
    value: i64,
    span: Span,
) -> LoweredBuiltinValue {
    ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(value)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(span),
    )
}

/// Emits one boolean literal used by the stable GC status schema.
fn emit_bool(
    ctx: &mut dyn BuiltinLoweringContext,
    value: bool,
    span: Span,
) -> LoweredBuiltinValue {
    ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(value)),
        PhpType::Bool,
        Op::ConstBool.default_effects(),
        Some(span),
    )
}

/// Boxes and inserts one scalar value under a persistent status key.
fn insert_status_entry(
    ctx: &mut dyn BuiltinLoweringContext,
    hash: crate::ir::ValueId,
    key: &str,
    value: LoweredBuiltinValue,
    span: Span,
) {
    let key_data = ctx.intern_string(key);
    let key = ctx.emit_value(
        Op::ConstStr,
        Vec::new(),
        Some(Immediate::Data(key_data)),
        PhpType::Str,
        Op::ConstStr.default_effects(),
        Some(span),
    );
    let value = ctx.emit_value(
        Op::MixedBox,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    );
    ctx.emit_void(
        Op::HashSet,
        vec![hash, key.value, value.value],
        None,
        Op::HashSet.default_effects(),
        Some(span),
    );
}
