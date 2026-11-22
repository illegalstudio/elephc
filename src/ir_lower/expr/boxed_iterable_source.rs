//! Purpose:
//! Hands the SPL iterator functions the raw payload of a source that is held in a boxed slot.
//!
//! Called from:
//! - Builtin function call lowering, after shared argument planning.
//!
//! Key details:
//! - A union-typed local keeps its boxed `Mixed` slot when a guard narrows the read to
//!   `iterable` or to a `Traversable` object, while `iterator_to_array()`, `iterator_count()`
//!   and `iterator_apply()` dispatch on a raw array, hash or object pointer. The layout-neutral
//!   `Op::MixedUnbox` bridges the two on every target.
//! - `iterator_to_array()` qualifies only for the mixed-payload results its helper builds from
//!   an `iterable` source. A source the checker typed as an array with raw elements keeps its
//!   existing lowering.

use super::*;

/// Replaces the boxed source of an SPL iterator function with its raw iterable payload.
///
/// `function all(string|iterable $files): array { if ($files instanceof Traversable) { return
/// iterator_to_array($files, false); } ... }` reads `$files` out of a boxed slot. The checker
/// already typed the call from the narrowed source, so the payload is known to be an array, a
/// hash or a `Traversable` object and the helper's runtime kind dispatch picks the right one.
pub(super) fn unbox_boxed_iterator_source(
    ctx: &mut LoweringContext<'_, '_>, canonical: &str, operands: &mut [ValueId], span: Span,
) {
    use crate::ir::RuntimeFnId::{IteratorApply, IteratorCount, IteratorToArray};
    let Some(crate::builtins::semantics::BuiltinLowering::Runtime(
        crate::ir::RuntimeCallTarget::Function(function),
    )) = crate::builtins::registry::lookup(canonical).map(|def| def.spec.semantics.lowering)
    else { return; };
    let reads_raw_source = match function {
        // The iterable helper boxes every element it copies, which is the layout the checker
        // promised only when it typed the source as `iterable` or as an object.
        IteratorToArray => ctx.builtin_call_types.get(&span).is_some_and(result_has_mixed_payload),
        IteratorCount | IteratorApply => true,
        _ => false,
    };
    if !reads_raw_source { return; }
    let Some(&source) = operands.first() else { return; };
    let ir_type = ctx.builder.value_type(source);
    if ir_type != IrType::Heap(IrHeapKind::Mixed) { return; }
    let source = LoweredValue { value: source, ir_type };
    let unboxed = ctx.emit_owned_value(
        Op::MixedUnbox, vec![source.value], None, PhpType::Iterable,
        Op::mixed_unbox_effects(&PhpType::Iterable), Some(span),
    );
    release_coerced_source_if_owned(ctx, source, Some(span));
    operands[0] = root_evaluated_call_argument(ctx, unboxed, span).value;
}

/// Whether every container the call can return stores boxed `Mixed` elements.
fn result_has_mixed_payload(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(element) => **element == PhpType::Mixed,
        PhpType::AssocArray { value, .. } => **value == PhpType::Mixed,
        PhpType::Union(members) => members.iter().all(result_has_mixed_payload),
        _ => false,
    }
}
