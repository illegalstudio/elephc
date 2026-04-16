use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::{expr_result_heap_ownership, Expr, HeapOwnership, PhpType};

pub(super) fn retain_borrowed_heap_arg(emitter: &mut Emitter, expr: &Expr, ty: &PhpType) {
    if ty.is_refcounted() && expr_result_heap_ownership(expr) != HeapOwnership::Owned {
        crate::codegen::abi::emit_incref_if_refcounted(emitter, ty);
    }
}

pub(super) fn widen_codegen_type(a: &PhpType, b: &PhpType) -> PhpType {
    if a == b {
        return a.clone();
    }
    if matches!(a, PhpType::Mixed | PhpType::Union(_))
        || matches!(b, PhpType::Mixed | PhpType::Union(_))
    {
        return PhpType::Mixed;
    }
    if *a == PhpType::Str || *b == PhpType::Str {
        return PhpType::Str;
    }
    if *a == PhpType::Float || *b == PhpType::Float {
        return PhpType::Float;
    }
    if *a == PhpType::Void {
        return b.clone();
    }
    if *b == PhpType::Void {
        return a.clone();
    }
    a.clone()
}

pub(super) fn coerce_result_to_type(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    source_ty: &PhpType,
    target_ty: &PhpType,
) {
    if source_ty == target_ty {
        return;
    }
    if matches!(target_ty, PhpType::Mixed | PhpType::Union(_)) {
        crate::codegen::emit_box_current_value_as_mixed(emitter, source_ty);
    } else if *target_ty == PhpType::Str {
        super::coerce_to_string(emitter, ctx, data, source_ty);
    } else if *target_ty == PhpType::Float
        && matches!(source_ty, PhpType::Int | PhpType::Bool | PhpType::Void)
    {
        if *source_ty == PhpType::Void {
            emitter.instruction("mov x0, #0");                                  // null widens to numeric zero before float coercion
        }
        crate::codegen::abi::emit_int_result_to_float_result(emitter);          // convert the integer-like result into the active target float-result register
    }
}
