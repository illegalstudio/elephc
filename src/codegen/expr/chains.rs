//! Purpose:
//! Lowers chained property, method, and array accesses that share a left-to-right receiver path.
//! Maintains intermediate receiver values while walking nested PHP access expressions.
//!
//! Called from:
//! - `crate::codegen::expr::emit_expr()`
//!
//! Key details:
//! - Each chain step must preserve nullability, ownership, and side-effect order for subsequent steps.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::functions;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::{arrays, calls, objects};

struct Chain<'a> {
    base: &'a Expr,
    segments: Vec<Segment<'a>>,
}

enum Segment<'a> {
    Property {
        expr: &'a Expr,
        receiver: &'a Expr,
        property: &'a str,
        nullsafe: bool,
    },
    Method {
        expr: &'a Expr,
        receiver: &'a Expr,
        method: &'a str,
        args: &'a [Expr],
        nullsafe: bool,
    },
    Array {
        expr: &'a Expr,
        array: &'a Expr,
        index: &'a Expr,
    },
    ExprCall {
        expr: &'a Expr,
        callee: &'a Expr,
        args: &'a [Expr],
    },
}

pub(super) fn emit_nullsafe_postfix_chain(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let chain = flatten_postfix_chain(expr)?;
    if !chain.segments.iter().any(Segment::is_nullsafe_member) {
        return None;
    }

    emitter.comment("nullsafe postfix chain");
    let null_label = ctx.next_label("nullsafe_chain_null");
    let done_label = ctx.next_label("nullsafe_chain_done");
    let mut current_ty = super::emit_expr(chain.base, emitter, ctx, data);
    let mut chain_can_short_circuit = false;
    let mut normal_path_terminated = false;

    for segment in chain.segments {
        if normal_path_terminated {
            break;
        }
        match segment {
            Segment::Property {
                expr,
                receiver,
                property,
                nullsafe,
            } => {
                let outcome = emit_property_segment(
                    expr,
                    receiver,
                    property,
                    nullsafe,
                    &current_ty,
                    &null_label,
                    emitter,
                    ctx,
                    data,
                );
                current_ty = outcome.ty;
                chain_can_short_circuit |= outcome.can_short_circuit;
                normal_path_terminated |= outcome.terminated_normal_path;
            }
            Segment::Method {
                expr,
                receiver,
                method,
                args,
                nullsafe,
            } => {
                let outcome = emit_method_segment(
                    expr,
                    receiver,
                    method,
                    args,
                    nullsafe,
                    &current_ty,
                    &null_label,
                    emitter,
                    ctx,
                    data,
                );
                current_ty = outcome.ty;
                chain_can_short_circuit |= outcome.can_short_circuit;
                normal_path_terminated |= outcome.terminated_normal_path;
            }
            Segment::Array { expr, array, index } => {
                let _ = (expr, array);
                current_ty = arrays::emit_array_access_with_loaded_base(
                    &current_ty,
                    index,
                    emitter,
                    ctx,
                    data,
                    true,
                );
            }
            Segment::ExprCall { expr, callee, args } => {
                let _ = expr;
                current_ty =
                    calls::emit_loaded_expr_call(callee, args, &current_ty, emitter, ctx, data);
            }
        }
    }

    if chain_can_short_circuit {
        objects::box_nullable_result(&current_ty, emitter);
        abi::emit_jump(emitter, &done_label);
        emitter.label(&null_label);
        objects::emit_boxed_null(emitter);
        emitter.label(&done_label);
        Some(PhpType::Mixed)
    } else {
        Some(current_ty)
    }
}

struct SegmentOutcome {
    ty: PhpType,
    can_short_circuit: bool,
    terminated_normal_path: bool,
}

impl SegmentOutcome {
    fn normal(ty: PhpType, can_short_circuit: bool) -> Self {
        Self {
            ty,
            can_short_circuit,
            terminated_normal_path: false,
        }
    }

    fn always_null() -> Self {
        Self {
            ty: PhpType::Void,
            can_short_circuit: true,
            terminated_normal_path: true,
        }
    }
}

impl Segment<'_> {
    fn is_nullsafe_member(&self) -> bool {
        matches!(
            self,
            Segment::Property {
                nullsafe: true,
                ..
            } | Segment::Method {
                nullsafe: true,
                ..
            }
        )
    }
}

fn flatten_postfix_chain(expr: &Expr) -> Option<Chain<'_>> {
    let mut base = expr;
    let mut segments = Vec::new();

    loop {
        match &base.kind {
            ExprKind::PropertyAccess { object, property } => {
                segments.push(Segment::Property {
                    expr: base,
                    receiver: object,
                    property,
                    nullsafe: false,
                });
                base = object;
            }
            ExprKind::NullsafePropertyAccess { object, property } => {
                segments.push(Segment::Property {
                    expr: base,
                    receiver: object,
                    property,
                    nullsafe: true,
                });
                base = object;
            }
            ExprKind::MethodCall {
                object,
                method,
                args,
            } => {
                segments.push(Segment::Method {
                    expr: base,
                    receiver: object,
                    method,
                    args,
                    nullsafe: false,
                });
                base = object;
            }
            ExprKind::NullsafeMethodCall {
                object,
                method,
                args,
            } => {
                segments.push(Segment::Method {
                    expr: base,
                    receiver: object,
                    method,
                    args,
                    nullsafe: true,
                });
                base = object;
            }
            ExprKind::ArrayAccess { array, index } => {
                segments.push(Segment::Array {
                    expr: base,
                    array,
                    index,
                });
                base = array;
            }
            ExprKind::ExprCall { callee, args } => {
                segments.push(Segment::ExprCall {
                    expr: base,
                    callee,
                    args,
                });
                base = callee;
            }
            _ => break,
        }
    }

    if segments.is_empty() {
        return None;
    }

    segments.reverse();
    Some(Chain { base, segments })
}

fn emit_property_segment(
    expr: &Expr,
    receiver: &Expr,
    property: &str,
    nullsafe: bool,
    current_ty: &PhpType,
    null_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> SegmentOutcome {
    let receiver_ty = functions::infer_contextual_type(receiver, ctx);
    if nullsafe {
        if receiver_is_static_null(current_ty, &receiver_ty) {
            abi::emit_jump(emitter, null_label);
            return SegmentOutcome::always_null();
        }
        let Some(class_name) = singular_receiver_class(&receiver_ty, current_ty) else {
            emitter.comment("WARNING: nullsafe property access on non-object");
            abi::emit_jump(emitter, null_label);
            return SegmentOutcome::always_null();
        };
        let can_short_circuit =
            emit_nullsafe_receiver_check(current_ty, null_label, emitter);
        let property_ty = objects::emit_loaded_object_property_access(
            &class_name,
            property,
            emitter,
            ctx,
            data,
        );
        return SegmentOutcome::normal(property_ty, can_short_circuit);
    }

    let Some(class_name) = singular_receiver_class(&receiver_ty, current_ty) else {
        emitter.comment("WARNING: property access on non-object");
        return SegmentOutcome::normal(functions::infer_contextual_type(expr, ctx), false);
    };
    if matches!(current_ty.codegen_repr(), PhpType::Mixed) {
        let ty = objects::emit_nullable_object_property_access(
            &class_name,
            property,
            emitter,
            ctx,
            data,
        );
        SegmentOutcome::normal(ty, false)
    } else {
        let ty = objects::emit_loaded_object_property_access(
            &class_name,
            property,
            emitter,
            ctx,
            data,
        );
        SegmentOutcome::normal(ty, false)
    }
}

fn emit_method_segment(
    expr: &Expr,
    receiver: &Expr,
    method: &str,
    args: &[Expr],
    nullsafe: bool,
    current_ty: &PhpType,
    null_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> SegmentOutcome {
    let receiver_ty = functions::infer_contextual_type(receiver, ctx);
    if nullsafe {
        if receiver_is_static_null(current_ty, &receiver_ty) {
            abi::emit_jump(emitter, null_label);
            return SegmentOutcome::always_null();
        }
        let Some(class_name) = singular_receiver_class(&receiver_ty, current_ty) else {
            emitter.comment("WARNING: nullsafe method call on non-object");
            abi::emit_jump(emitter, null_label);
            return SegmentOutcome::always_null();
        };
        let can_short_circuit =
            emit_nullsafe_receiver_check(current_ty, null_label, emitter);
        let return_ty =
            emit_loaded_method_call(&class_name, method, args, emitter, ctx, data);
        return SegmentOutcome::normal(return_ty, can_short_circuit);
    }

    let Some(class_name) = singular_receiver_class(&receiver_ty, current_ty) else {
        emitter.comment("WARNING: method call on non-object");
        return SegmentOutcome::normal(functions::infer_contextual_type(expr, ctx), false);
    };
    if matches!(current_ty.codegen_repr(), PhpType::Mixed) {
        let message = format!(
            "Fatal error: Call to a member function {}() on null\n",
            method
        );
        objects::emit_unbox_mixed_object_or_fatal(message.as_bytes(), emitter, ctx, data);
    }
    let return_ty = emit_loaded_method_call(&class_name, method, args, emitter, ctx, data);
    SegmentOutcome::normal(return_ty, false)
}

fn emit_loaded_method_call(
    class_name: &str,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // save the loaded receiver below later method arguments
    let sig = ctx
        .classes
        .get(class_name)
        .and_then(|class_info| class_info.methods.get(method))
        .cloned();
    let emitted_args = objects::emit_pushed_method_args(args, sig.as_ref(), emitter, ctx, data);
    objects::emit_method_call_with_saved_receiver_below_args(
        class_name,
        method,
        &emitted_args.arg_types,
        emitted_args.source_temp_bytes,
        emitter,
        ctx,
    )
}

fn emit_nullsafe_receiver_check(
    current_ty: &PhpType,
    null_label: &str,
    emitter: &mut Emitter,
) -> bool {
    match current_ty.codegen_repr() {
        PhpType::Void => {
            abi::emit_jump(emitter, null_label);
            true
        }
        PhpType::Mixed => {
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // unwrap nullable receiver before the nullsafe chain segment
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #8");                          // runtime tag 8 means the receiver is null
                    emitter.instruction(&format!("b.eq {}", null_label));       // short-circuit the remaining postfix chain on null
                    emitter.instruction("mov x0, x1");                          // move the unboxed object pointer into the normal result register
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 8");                          // runtime tag 8 means the receiver is null
                    emitter.instruction(&format!("je {}", null_label));         // short-circuit the remaining postfix chain on null
                    emitter.instruction("mov rax, rdi");                        // move the unboxed object pointer into the normal result register
                }
            }
            true
        }
        _ => false,
    }
}

fn receiver_is_static_null(current_ty: &PhpType, receiver_ty: &PhpType) -> bool {
    matches!(current_ty.codegen_repr(), PhpType::Void) || matches!(receiver_ty, PhpType::Void)
}

fn singular_receiver_class(receiver_ty: &PhpType, current_ty: &PhpType) -> Option<String> {
    functions::singular_object_class(receiver_ty)
        .or_else(|| functions::singular_object_class(current_ty))
        .map(str::to_string)
}
