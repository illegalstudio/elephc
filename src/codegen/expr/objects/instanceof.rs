use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::names::Name;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::super::emit_expr;
use super::dispatch;

pub(super) fn emit_instanceof(
    value: &Expr,
    target: &Name,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("instanceof {}", target.as_str()));
    let value_ty = emit_expr(value, emitter, ctx, data);
    let value_repr = value_ty.codegen_repr();

    let target_kind = match classify_target(target, ctx) {
        Some(kind) => kind,
        None => {
            emit_false(emitter);
            return PhpType::Bool;
        }
    };

    if !can_hold_object_or_boxed_value(&value_repr) {
        emit_false(emitter);
        return PhpType::Bool;
    }

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the tested value while materializing the target type id
    let target_kind_id = match target_kind {
        InstanceOfTarget::Class(class_id) => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), class_id as i64);
            0
        }
        InstanceOfTarget::Interface(interface_id) => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), interface_id as i64);
            1
        }
        InstanceOfTarget::LateStaticClass => {
            if !dispatch::emit_forwarded_called_class_id(emitter, ctx) {
                abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));       // discard the preserved tested value before returning false
                emit_false(emitter);
                return PhpType::Bool;
            }
            0
        }
    };
    let matcher = if matches!(value_repr, PhpType::Mixed | PhpType::Union(_)) {
        "__rt_mixed_instanceof"
    } else {
        "__rt_exception_matches"
    };
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the target id while loading runtime matcher arguments
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 1));       // pass target class/interface id as matcher argument 2
    abi::emit_pop_reg(emitter, abi::int_arg_reg_name(emitter.target, 0));       // pass the tested object pointer as matcher argument 1
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 2),
        target_kind_id,
    );
    abi::emit_call_label(emitter, matcher);                                     // run the object/class/interface matcher for plain or boxed values
    PhpType::Bool
}

enum InstanceOfTarget {
    Class(u64),
    Interface(u64),
    LateStaticClass,
}

fn classify_target(target: &Name, ctx: &Context) -> Option<InstanceOfTarget> {
    let target_name = match target.as_str() {
        "self" => ctx.current_class.as_deref()?,
        "parent" => ctx
            .current_class
            .as_ref()
            .and_then(|class_name| ctx.classes.get(class_name))
            .and_then(|class_info| class_info.parent.as_deref())?,
        "static" => return Some(InstanceOfTarget::LateStaticClass),
        other => other,
    };
    if let Some(class_info) = ctx.classes.get(target_name) {
        Some(InstanceOfTarget::Class(class_info.class_id))
    } else {
        ctx.interfaces
            .get(target_name)
            .map(|interface_info| InstanceOfTarget::Interface(interface_info.interface_id))
    }
}

fn can_hold_object_or_boxed_value(ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_) => true,
        _ => false,
    }
}

fn emit_false(emitter: &mut Emitter) {
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
}
