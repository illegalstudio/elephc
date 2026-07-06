//! Purpose:
//! Provides shared expression lowering utilities for strings, arrays, nullable values, and runtime checks.
//! Keeps repeated assembly snippets out of individual expression feature emitters.
//!
//! Called from:
//! - `crate::codegen_support::expr` submodules
//!
//! Key details:
//! - Helpers must document and preserve the result registers and scratch registers they clobber.

use super::super::context::{Context, HeapOwnership};
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::{expr_result_heap_ownership, Expr, PhpType};

/// Increments the refcount of a borrowed heap argument if the expression result is not already owned.
pub(super) fn retain_borrowed_heap_arg(emitter: &mut Emitter, expr: &Expr, ty: &PhpType) {
    if expr_result_heap_ownership(expr) == HeapOwnership::Owned {
        return;
    }
    if matches!(ty, PhpType::Callable) {
        crate::codegen_support::callable_descriptor::emit_retain_current_descriptor(emitter);
    } else if ty.is_refcounted() {
        crate::codegen_support::abi::emit_incref_if_refcounted(emitter, ty);
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
    if matches!(a, PhpType::TaggedScalar) || matches!(b, PhpType::TaggedScalar) {
        let other = if matches!(a, PhpType::TaggedScalar) { b } else { a };
        return match other {
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::TaggedScalar => {
                PhpType::TaggedScalar
            }
            _ => PhpType::Mixed,
        };
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
                crate::codegen_support::abi::emit_call_label(emitter, "__rt_mixed_cast_int");
            }
            PhpType::Pointer(_) => {
                crate::codegen_support::abi::emit_call_label(emitter, "__rt_mixed_cast_int");
            }
            PhpType::Bool => {
                crate::codegen_support::abi::emit_call_label(emitter, "__rt_mixed_cast_bool");
            }
            PhpType::Float => {
                crate::codegen_support::abi::emit_call_label(emitter, "__rt_mixed_cast_float");
            }
            PhpType::Str => {
                super::coerce_to_string(emitter, ctx, data, source_ty);
            }
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => match emitter.target.arch {
                crate::codegen_support::platform::Arch::AArch64 => {
                    crate::codegen_support::abi::emit_call_label(emitter, "__rt_mixed_unbox");
                    emitter.instruction("mov x0, x1");                          // use the unboxed heap payload word as the coerced pointer
                }
                crate::codegen_support::platform::Arch::X86_64 => {
                    crate::codegen_support::abi::emit_call_label(emitter, "__rt_mixed_unbox");
                    emitter.instruction("mov rax, rdi");                        // use the unboxed heap payload word as the coerced pointer
                }
            },
            PhpType::Mixed | PhpType::Union(_) => {}
            PhpType::TaggedScalar => match emitter.target.arch {
                crate::codegen_support::platform::Arch::AArch64 => {
                    crate::codegen_support::abi::emit_call_label(emitter, "__rt_mixed_unbox");
                    emitter.instruction("mov x9, x1");                          // stage the unboxed payload while the tag moves into the tag register
                    emitter.instruction("mov x1, x0");                          // place the unboxed runtime tag in the tagged scalar tag register
                    emitter.instruction("mov x0, x9");                          // place the unboxed payload in the tagged scalar payload register
                }
                crate::codegen_support::platform::Arch::X86_64 => {
                    crate::codegen_support::abi::emit_call_label(emitter, "__rt_mixed_unbox");
                    emitter.instruction("mov rdx, rax");                        // place the unboxed runtime tag in the tagged scalar tag register
                    emitter.instruction("mov rax, rdi");                        // place the unboxed payload in the tagged scalar payload register
                }
            },
            _ => {}
        }
    } else if matches!(source_ty, PhpType::TaggedScalar) {
        match target_ty.codegen_repr() {
            PhpType::Int | PhpType::Bool | PhpType::Resource(_) | PhpType::Pointer(_) => {
                crate::codegen_support::sentinels::emit_tagged_scalar_to_int_null_as_zero(emitter);
            }
            PhpType::Float => {
                crate::codegen_support::sentinels::emit_tagged_scalar_to_int_null_as_zero(emitter);
                crate::codegen_support::abi::emit_int_result_to_float_result(emitter);  // widen the narrowed payload into the float result register
            }
            PhpType::Str => {
                super::coerce_to_string(emitter, ctx, data, source_ty);
            }
            PhpType::Mixed | PhpType::Union(_) => {
                crate::codegen_support::emit_box_current_value_as_mixed(emitter, source_ty);
            }
            _ => {}
        }
    } else if matches!(target_ty, PhpType::TaggedScalar) {
        match source_ty {
            PhpType::Int | PhpType::Bool => {
                crate::codegen_support::sentinels::emit_tagged_scalar_from_int_result(emitter);
            }
            PhpType::Void | PhpType::Never => {
                crate::codegen_support::sentinels::emit_tagged_scalar_null(emitter);
            }
            _ => {}
        }
    } else if matches!(target_ty, PhpType::Mixed | PhpType::Union(_)) {
        crate::codegen_support::emit_box_current_value_as_mixed(emitter, source_ty);
    } else if *target_ty == PhpType::Str {
        super::coerce_to_string(emitter, ctx, data, source_ty);
    } else if *target_ty == PhpType::Float
        && matches!(source_ty, PhpType::Int | PhpType::Bool | PhpType::Void)
    {
        if *source_ty == PhpType::Void {
            emitter.instruction("mov x0, #0");                                  // null widens to numeric zero before float coercion
        }
        crate::codegen_support::abi::emit_int_result_to_float_result(emitter);          // convert the integer-like result into the active target float-result register
    } else if *target_ty == PhpType::Int && *source_ty == PhpType::Float {
        match emitter.target.arch {
            crate::codegen_support::platform::Arch::AArch64 => {
                emitter.instruction("fcvtzs x0, d0");                           // truncate the float result to an integer for PHP coercion
            }
            crate::codegen_support::platform::Arch::X86_64 => {
                emitter.instruction("cvttsd2si rax, xmm0");                     // truncate the float result to an integer for PHP coercion
            }
        }
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
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
                | PhpType::Mixed
                | PhpType::Union(_)
        );
    }
    if matches!(source_ty, PhpType::TaggedScalar) {
        return matches!(
            target_ty.codegen_repr(),
            PhpType::Int
                | PhpType::Bool
                | PhpType::Resource(_)
                | PhpType::Pointer(_)
                | PhpType::Float
                | PhpType::Str
                | PhpType::Mixed
                | PhpType::Union(_)
        );
    }
    if matches!(target_ty, PhpType::TaggedScalar) {
        return matches!(
            source_ty,
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never
        );
    }
    matches!(target_ty, PhpType::Mixed | PhpType::Union(_))
        || *target_ty == PhpType::Str
        || (*target_ty == PhpType::Float
            && matches!(source_ty, PhpType::Int | PhpType::Bool | PhpType::Void))
}
