use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::functions;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::{access, dispatch};
use crate::codegen::expr::emit_expr;

const NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;

pub(super) fn emit_nullsafe_property_access(
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let Some((class_name, nullable)) = nullsafe_receiver_class(object, ctx) else {
        emit_expr(object, emitter, ctx, data);
        emit_plain_null(emitter);
        return PhpType::Void;
    };
    if !nullable {
        return access::emit_property_access(object, property, emitter, ctx, data);
    }

    emitter.comment(&format!("?->{}", property));
    let null_label = ctx.next_label("nullsafe_prop_null");
    let done_label = ctx.next_label("nullsafe_prop_done");
    let receiver_ty = emit_expr(object, emitter, ctx, data);
    if !emit_nullable_receiver_to_object(&receiver_ty, &null_label, emitter) {
        emit_boxed_null(emitter);
        return PhpType::Mixed;
    }

    let property_ty =
        access::emit_loaded_object_property_access(&class_name, property, emitter, ctx, data);
    box_nullable_result(&property_ty, emitter);
    abi::emit_jump(emitter, &done_label);
    emitter.label(&null_label);
    emit_boxed_null(emitter);
    emitter.label(&done_label);
    PhpType::Mixed
}

pub(super) fn emit_nullsafe_method_call(
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let Some((class_name, nullable)) = nullsafe_receiver_class(object, ctx) else {
        emit_expr(object, emitter, ctx, data);
        emit_plain_null(emitter);
        return PhpType::Void;
    };
    if !nullable {
        return dispatch::emit_method_call(object, method, args, emitter, ctx, data);
    }

    emitter.comment(&format!("?->{}()", method));
    let null_label = ctx.next_label("nullsafe_method_null");
    let done_label = ctx.next_label("nullsafe_method_done");
    let receiver_ty = emit_expr(object, emitter, ctx, data);
    if !emit_nullable_receiver_to_object(&receiver_ty, &null_label, emitter) {
        emit_boxed_null(emitter);
        return PhpType::Mixed;
    }

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // save the receiver below later argument temporaries until the nullsafe branch commits to the call
    let sig = ctx
        .classes
        .get(&class_name)
        .and_then(|class_info| class_info.methods.get(method))
        .cloned();
    let arg_types = dispatch::emit_pushed_method_args(args, sig.as_ref(), emitter, ctx, data);
    let return_ty = dispatch::emit_method_call_with_saved_receiver_below_args(
        &class_name,
        method,
        &arg_types,
        emitter,
        ctx,
    );
    box_nullable_result(&return_ty, emitter);
    abi::emit_jump(emitter, &done_label);
    emitter.label(&null_label);
    emit_boxed_null(emitter);
    emitter.label(&done_label);
    PhpType::Mixed
}

fn nullsafe_receiver_class(object: &Expr, ctx: &Context) -> Option<(String, bool)> {
    match functions::infer_contextual_type(object, ctx) {
        PhpType::Object(class_name) => Some((class_name, false)),
        PhpType::Void => None,
        PhpType::Union(members) => {
            let mut class_name = None;
            let mut nullable = false;
            for member in members {
                match member {
                    PhpType::Void => nullable = true,
                    PhpType::Object(candidate) => class_name = Some(candidate),
                    _ => return None,
                }
            }
            class_name.map(|name| (name, nullable))
        }
        _ => None,
    }
}

fn emit_nullable_receiver_to_object(
    receiver_ty: &PhpType,
    null_label: &str,
    emitter: &mut Emitter,
) -> bool {
    match receiver_ty.codegen_repr() {
        PhpType::Void => false,
        PhpType::Object(_) => true,
        PhpType::Mixed => {
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // inspect a nullable receiver box before following the object member access
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #8");                          // runtime tag 8 means the nullsafe receiver is null
                    emitter.instruction(&format!("b.eq {}", null_label));       // skip member evaluation when the receiver is null
                    emitter.instruction("mov x0, x1");                          // move the unboxed object pointer into the normal result register
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 8");                          // runtime tag 8 means the nullsafe receiver is null
                    emitter.instruction(&format!("je {}", null_label));         // skip member evaluation when the receiver is null
                    emitter.instruction("mov rax, rdi");                        // move the unboxed object pointer into the normal result register
                }
            }
            true
        }
        _ => true,
    }
}

fn box_nullable_result(result_ty: &PhpType, emitter: &mut Emitter) {
    if !matches!(result_ty.codegen_repr(), PhpType::Mixed) {
        crate::codegen::emit_box_current_value_as_mixed(emitter, result_ty);
    }
}

fn emit_boxed_null(emitter: &mut Emitter) {
    emit_plain_null(emitter);
    crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Void);
}

fn emit_plain_null(emitter: &mut Emitter) {
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), NULL_SENTINEL);
}
