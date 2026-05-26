//! Purpose:
//! Provides shared expression lowering utilities for strings, arrays, nullable values, and runtime checks.
//! Keeps repeated assembly snippets out of individual expression feature emitters.
//!
//! Called from:
//! - `crate::codegen::expr` submodules
//!
//! Key details:
//! - Helpers must document and preserve the result registers and scratch registers they clobber.

use super::super::context::{Context, HeapOwnership};
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::{expr_result_heap_ownership, Expr, PhpType};

/// Increments the refcount of a borrowed heap argument if the expression result is not already owned.
pub(super) fn retain_borrowed_heap_arg(emitter: &mut Emitter, expr: &Expr, ty: &PhpType) {
    if ty.is_refcounted() && expr_result_heap_ownership(expr) != HeapOwnership::Owned {
        crate::codegen::abi::emit_incref_if_refcounted(emitter, ty);
    }
}

/// Returns the wider of two PhpType for mixed-type expression results.
///
/// The type priority is: Mixed > Union > Str > Float > (Int/Bool/Null) > Void.
/// When one operand is Void, returns the other type. Otherwise returns the
/// higher-priority type, or `a` if both are lower-priority types with no Void.
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

/// Emits runtime coercion from source_ty to target_ty using appropriate __rt_* helpers.
pub(crate) fn coerce_result_to_type(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    source_ty: &PhpType,
    target_ty: &PhpType,
) {
    if source_ty == target_ty {
        return;
    }
    if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
        match target_ty.codegen_repr() {
            PhpType::Int | PhpType::Resource(_) => {
                crate::codegen::abi::emit_call_label(emitter, "__rt_mixed_cast_int");
            }
            PhpType::Pointer(_) => {
                crate::codegen::abi::emit_call_label(emitter, "__rt_mixed_cast_int");
            }
            PhpType::Bool => {
                crate::codegen::abi::emit_call_label(emitter, "__rt_mixed_cast_bool");
            }
            PhpType::Float => {
                crate::codegen::abi::emit_call_label(emitter, "__rt_mixed_cast_float");
            }
            PhpType::Str => {
                super::coerce_to_string(emitter, ctx, data, source_ty);
            }
            PhpType::Object(_) => match emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    crate::codegen::abi::emit_call_label(emitter, "__rt_mixed_unbox");
                    emitter.instruction("mov x0, x1");                          // use the object payload word as the coerced object pointer
                }
                crate::codegen::platform::Arch::X86_64 => {
                    crate::codegen::abi::emit_call_label(emitter, "__rt_mixed_unbox");
                    emitter.instruction("mov rax, rdi");                        // use the object payload word as the coerced object pointer
                }
            },
            PhpType::Mixed | PhpType::Union(_) => {}
            _ => {}
        }
    } else if matches!(target_ty, PhpType::Mixed | PhpType::Union(_)) {
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

/// Returns true if coerce_result_to_type would succeed for the given source/target pair.
pub(crate) fn can_coerce_result_to_type(source_ty: &PhpType, target_ty: &PhpType) -> bool {
    if source_ty == target_ty {
        return true;
    }
    if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
        return matches!(
            target_ty.codegen_repr(),
            PhpType::Int
                | PhpType::Resource(_)
                | PhpType::Pointer(_)
                | PhpType::Bool
                | PhpType::Float
                | PhpType::Str
                | PhpType::Object(_)
                | PhpType::Mixed
                | PhpType::Union(_)
        );
    }
    matches!(target_ty, PhpType::Mixed | PhpType::Union(_))
        || *target_ty == PhpType::Str
        || (*target_ty == PhpType::Float
            && matches!(source_ty, PhpType::Int | PhpType::Bool | PhpType::Void))
}
