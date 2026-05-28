//! Purpose:
//! Recognizes and lowers null-coalescing assignment forms that need specialized read-before-write behavior.
//! Avoids duplicating complex lvalue evaluation for locals, properties, arrays, and static properties.
//!
//! Called from:
//! - `crate::codegen::stmt` and assignment emitters
//!
//! Key details:
//! - The left-hand side must be evaluated once and only assigned when the observed value is null.

use super::super::abi;
use super::super::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::types::PhpType;

/// Matches NullCoalesce on `$array[$index]` where the variable and index match.
/// Returns `(current, default)` when `value` is `$array[$index] ?? <default>`,
/// otherwise returns `None`. The `current` expression is the read-before-write form.
pub(crate) fn null_coalesce_array_target<'a>(
    array: &str,
    index: &Expr,
    value: &'a Expr,
) -> Option<(&'a Expr, &'a Expr)> {
    let ExprKind::NullCoalesce {
        value: current,
        default,
    } = &value.kind
    else {
        return None;
    };
    let ExprKind::ArrayAccess {
        array: current_array,
        index: current_index,
    } = &current.kind
    else {
        return None;
    };
    if matches!(&current_array.kind, ExprKind::Variable(name) if name == array)
        && expr_equivalent(current_index, index)
    {
        Some((current, default))
    } else {
        None
    }
}

/// Matches NullCoalesce on `$object->property` where the object and property match.
/// Returns `(current, default)` when `value` is `$object->property ?? <default>`,
/// otherwise returns `None`. The `current` expression is the read-before-write form.
pub(crate) fn null_coalesce_property_target<'a>(
    object: &Expr,
    property: &str,
    value: &'a Expr,
) -> Option<(&'a Expr, &'a Expr)> {
    let ExprKind::NullCoalesce {
        value: current,
        default,
    } = &value.kind
    else {
        return None;
    };
    let ExprKind::PropertyAccess {
        object: current_object,
        property: current_property,
    } = &current.kind
    else {
        return None;
    };
    if current_property == property && expr_equivalent(current_object, object) {
        Some((current, default))
    } else {
        None
    }
}

/// Matches NullCoalesce on `StaticProperty` where the receiver and property match.
/// Returns `(current, default)` when `value` is `<Receiver>::$property ?? <default>`,
/// otherwise returns `None`. The `current` expression is the read-before-write form.
pub(crate) fn null_coalesce_static_property_target<'a>(
    receiver: &StaticReceiver,
    property: &str,
    value: &'a Expr,
) -> Option<(&'a Expr, &'a Expr)> {
    let ExprKind::NullCoalesce {
        value: current,
        default,
    } = &value.kind
    else {
        return None;
    };
    let ExprKind::StaticPropertyAccess {
        receiver: current_receiver,
        property: current_property,
    } = &current.kind
    else {
        return None;
    };
    if current_receiver == receiver && current_property == property {
        Some((current, default))
    } else {
        None
    }
}

/// Matches NullCoalesce on `$object->property[$index]` where object, property, and index match.
/// Returns `(current, default)` when `value` is `$object->property[$index] ?? <default>`,
/// otherwise returns `None`. The `current` expression is the read-before-write form.
pub(crate) fn null_coalesce_property_array_target<'a>(
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &'a Expr,
) -> Option<(&'a Expr, &'a Expr)> {
    let ExprKind::NullCoalesce {
        value: current,
        default,
    } = &value.kind
    else {
        return None;
    };
    let ExprKind::ArrayAccess {
        array: current_array,
        index: current_index,
    } = &current.kind
    else {
        return None;
    };
    let ExprKind::PropertyAccess {
        object: current_object,
        property: current_property,
    } = &current_array.kind
    else {
        return None;
    };
    if current_property == property
        && expr_equivalent(current_object, object)
        && expr_equivalent(current_index, index)
    {
        Some((current, default))
    } else {
        None
    }
}

/// Matches NullCoalesce on `StaticProperty[$index]` where receiver, property, and index match.
/// Returns `(current, default)` when `value` is `<Receiver>::$property[$index] ?? <default>`,
/// otherwise returns `None`. The `current` expression is the read-before-write form.
pub(crate) fn null_coalesce_static_property_array_target<'a>(
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    value: &'a Expr,
) -> Option<(&'a Expr, &'a Expr)> {
    let ExprKind::NullCoalesce {
        value: current,
        default,
    } = &value.kind
    else {
        return None;
    };
    let ExprKind::ArrayAccess {
        array: current_array,
        index: current_index,
    } = &current.kind
    else {
        return None;
    };
    let ExprKind::StaticPropertyAccess {
        receiver: current_receiver,
        property: current_property,
    } = &current_array.kind
    else {
        return None;
    };
    if current_receiver == receiver
        && current_property == property
        && expr_equivalent(current_index, index)
    {
        Some((current, default))
    } else {
        None
    }
}

/// Implements the `expr_equivalent` operation for this module.
fn expr_equivalent(left: &Expr, right: &Expr) -> bool {
    match (&left.kind, &right.kind) {
        (ExprKind::Variable(a), ExprKind::Variable(b)) => a == b,
        (ExprKind::This, ExprKind::This) => true,
        (ExprKind::IntLiteral(a), ExprKind::IntLiteral(b)) => a == b,
        (ExprKind::StringLiteral(a), ExprKind::StringLiteral(b)) => a == b,
        (ExprKind::BoolLiteral(a), ExprKind::BoolLiteral(b)) => a == b,
        (ExprKind::Null, ExprKind::Null) => true,
        (
            ExprKind::StaticPropertyAccess {
                receiver: a_receiver,
                property: a_property,
            },
            ExprKind::StaticPropertyAccess {
                receiver: b_receiver,
                property: b_property,
            },
        ) => a_receiver == b_receiver && a_property == b_property,
        (
            ExprKind::PropertyAccess {
                object: a_object,
                property: a_property,
            },
            ExprKind::PropertyAccess {
                object: b_object,
                property: b_property,
            },
        ) => a_property == b_property && expr_equivalent(a_object, b_object),
        (
            ExprKind::ArrayAccess {
                array: a_array,
                index: a_index,
            },
            ExprKind::ArrayAccess {
                array: b_array,
                index: b_index,
            },
        ) => expr_equivalent(a_array, b_array) && expr_equivalent(a_index, b_index),
        _ => false,
    }
}

/// Emits a conditional branch to `keep_label` when the `??=` result value is non-null.
/// Handles Mixed/Union by unboxing first and comparing the runtime tag; uses a sentinel
/// value for scalar types (int, float, string). The sentinel avoids polluting the
/// codegen for the common null case.
pub(crate) fn emit_branch_if_result_non_null(
    ty: &PhpType,
    keep_label: &str,
    emitter: &mut Emitter,
) {
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // inspect the boxed value tag before deciding whether ??= should store
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction("cmp x0, #8");                              // runtime tag 8 means the boxed value is null
                emitter.instruction(&format!("b.ne {}", keep_label));           // keep the existing value when the boxed payload is non-null
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction("cmp rax, 8");                              // runtime tag 8 means the boxed value is null
                emitter.instruction(&format!("jne {}", keep_label));            // keep the existing value when the boxed payload is non-null
            }
        }
        return;
    }

    let null_reg = abi::symbol_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, null_reg, 0x7fff_ffff_ffff_fffe_u64 as i64);
    if ty == &PhpType::Float {
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction("fmov x0, d0");                             // copy the float bits into x0 for the null-sentinel comparison
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction("movq rax, xmm0");                          // copy the float bits into rax for the null-sentinel comparison
            }
        }
    }
    let cmp_reg = if ty == &PhpType::Str {
        abi::string_result_regs(emitter).0
    } else {
        abi::int_result_reg(emitter)
    };
    emitter.instruction(&format!("cmp {}, {}", cmp_reg, null_reg));             // compare the current value with the shared null sentinel
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("b.ne {}", keep_label));               // keep the existing value when it is not null
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("jne {}", keep_label));                // keep the existing value when it is not null
        }
    }
}
