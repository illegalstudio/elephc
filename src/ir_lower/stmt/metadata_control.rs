//! Purpose:
//! No-op metadata markers and shared control-flow helpers.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Preserves physical declaration identity for later native binding activation.
pub(super) fn lower_class_like_activation(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    kind: crate::parser::ast::ClassLikeKind,
    source_path: &std::path::Path,
    span: Span,
) {
    let source = ctx.source_catalog.as_ref().and_then(|catalog| catalog.id_for_path(source_path));
    let immediate = match source {
        Some(source) => {
            Immediate::class_like_activation(source, span, kind, ctx.intern_string(name))
        }
        None => Immediate::UnresolvedSource(Box::new(source_path.to_path_buf())),
    };
    ctx.emit_void(Op::ClassLikeActivate, Vec::new(), Some(immediate),
        Op::ClassLikeActivate.default_effects(), Some(span));
}

/// Emits a no-op marker for declaration-only or frontend-only statements.
pub(super) fn lower_noop(ctx: &mut LoweringContext<'_, '_>, span: Span) {
    ctx.emit_void(
        Op::Nop,
        Vec::new(),
        None,
        Op::Nop.default_effects(),
        Some(span),
    );
}

/// Records a function variant group in high-level EIR metadata form.
pub(super) fn lower_function_variant_group(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    variants: &[String],
    span: Span,
) {
    let label = format!("{}:{}", name, variants.join(","));
    let data = ctx.intern_string(&label);
    ctx.emit_void(
        Op::FunctionVariantDispatch,
        Vec::new(),
        Some(Immediate::Data(data)),
        Op::FunctionVariantDispatch.default_effects(),
        Some(span),
    );
}

/// Records one selected function variant.
pub(super) fn lower_function_variant_mark(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    variant: &str,
    span: Span,
) {
    let label = format!("{}:{}", name, variant);
    let data = ctx.intern_string(&label);
    ctx.emit_void(
        Op::FunctionVariantMark,
        Vec::new(),
        Some(Immediate::Data(data)),
        Op::FunctionVariantMark.default_effects(),
        Some(span),
    );
}

/// Emits a branch to `target` if the current block can still fall through.
pub(super) fn branch_to(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br {
            target,
            args: Vec::new(),
        });
    }
}

/// Finds the active loop target and its lexical index for a one-based control level.
pub(super) fn loop_target(
    ctx: &LoweringContext<'_, '_>,
    level: usize,
) -> Option<(usize, LoopFrame)> {
    let level = level.max(1);
    ctx.loop_stack
        .len()
        .checked_sub(level)
        .and_then(|index| ctx.loop_stack.get(index).copied().map(|frame| (index, frame)))
}

/// Selects the strongest array write opcode valid for a lowered array value.
pub(super) fn array_set_op(ir_type: IrType) -> Op {
    match ir_type {
        IrType::Heap(crate::ir::IrHeapKind::Array) => Op::ArraySet,
        IrType::Heap(crate::ir::IrHeapKind::Hash) => Op::HashSet,
        IrType::Heap(crate::ir::IrHeapKind::Buffer) => Op::BufferSet,
        _ => Op::RuntimeCall,
    }
}

/// Returns true when a lowered index value is a boxed `Mixed`/`Union` cell that
/// may hold either an integer or a string array key (e.g. a foreach loop key,
/// which `Op::IterCurrentKey` always produces as Mixed). Such writes go through
/// `Op::ArraySetMixedKey` so the key tag is dispatched at runtime instead of
/// coercing it to int (which would collapse a string key onto int 0).
pub(super) fn index_is_boxed_mixed_key(ir_type: IrType) -> bool {
    matches!(
        ir_type,
        IrType::Heap(crate::ir::IrHeapKind::Mixed)
            | IrType::Heap(crate::ir::IrHeapKind::Union)
    )
}

/// Returns true when the index expression is a foreach loop key known to hold an
/// integer at runtime (its source was a concretely-indexed array), so the
/// destination write can keep the indexed `ArraySet` path with int coercion
/// instead of promoting to a hash. See `LoweringContext::mark_foreach_int_key`.
pub(super) fn index_is_foreach_int_key(ctx: &LoweringContext<'_, '_>, index: &Expr) -> bool {
    if let ExprKind::Variable(name) = &index.kind {
        return ctx.is_foreach_int_key(name);
    }
    false
}

/// Extracts an integer switch case value from literal cases.
pub(super) fn int_case_value(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::BoolLiteral(value) => Some(i64::from(*value)),
        _ => None,
    }
}

/// Emits a boolean constant value.
pub(super) fn emit_const_bool(
    ctx: &mut LoweringContext<'_, '_>,
    value: bool,
    span: Option<Span>,
) -> LoweredValue {
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ConstBool,
            Vec::new(),
            Some(Immediate::Bool(value)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
            Op::ConstBool.default_effects(),
            span,
        )
        .expect("const_bool produces a value");
    LoweredValue {
        value,
        ir_type: IrType::I64,
    }
}

/// Emits a null sentinel value.
pub(super) fn emit_null_value(ctx: &mut LoweringContext<'_, '_>, span: Option<Span>) -> LoweredValue {
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ConstNull,
            Vec::new(),
            None,
            IrType::I64,
            PhpType::Void,
            Ownership::NonHeap,
            Op::ConstNull.default_effects(),
            span,
        )
        .expect("const_null produces a value");
    LoweredValue {
        value,
        ir_type: IrType::I64,
    }
}
