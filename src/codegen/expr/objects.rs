mod access;
mod allocation;
mod dispatch;
mod instanceof;
mod nullsafe;
mod static_properties;

use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::scalars;
use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::names::Name;
use crate::parser::ast::{Expr, StaticReceiver};
use crate::types::PhpType;

pub(super) fn emit_new_object(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    allocation::emit_new_object(class_name, args, emitter, ctx, data)
}

/// Resolve a non-late-bound `StaticReceiver` to a concrete class FQN at codegen time.
/// Returns `None` if the receiver cannot be resolved (no enclosing class for
/// `Self_`, or no parent for `Parent`). `Static` is intentionally not handled
/// here because it must use the forwarded called-class id at runtime.
fn resolve_scoped_receiver_to_class(receiver: &StaticReceiver, ctx: &Context) -> Option<String> {
    match receiver {
        StaticReceiver::Self_ => ctx.current_class.clone(),
        StaticReceiver::Parent => ctx
            .current_class
            .as_ref()
            .and_then(|c| ctx.classes.get(c))
            .and_then(|info| info.parent.clone()),
        StaticReceiver::Named(name) => Some(name.as_canonical()),
        StaticReceiver::Static => None,
    }
}

pub(super) fn emit_class_constant(
    receiver: &StaticReceiver,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if matches!(receiver, StaticReceiver::Static) {
        return emit_late_bound_class_constant(emitter, ctx, data);
    }

    let name = resolve_scoped_receiver_to_class(receiver, ctx).unwrap_or_default();
    scalars::emit_string_literal(&name, emitter, data)
}

pub(super) fn emit_new_scoped_object(
    receiver: &StaticReceiver,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if matches!(receiver, StaticReceiver::Static) {
        return emit_late_bound_new_static(args, emitter, ctx, data);
    }

    let class_name = resolve_scoped_receiver_to_class(receiver, ctx)
        .expect("new self/parent/static used outside class context — should be a type error");
    allocation::emit_new_object(&class_name, args, emitter, ctx, data)
}

fn sorted_late_bound_classes_by_id(ctx: &Context) -> Vec<(String, u64)> {
    let Some(base_class) = ctx.current_class.as_deref() else {
        return Vec::new();
    };
    let mut classes: Vec<(String, u64)> = ctx
        .classes
        .iter()
        .filter(|(name, _)| class_is_same_or_descends_from(name, base_class, ctx))
        .map(|(name, info)| (name.clone(), info.class_id))
        .collect();
    classes.sort_by_key(|(_, class_id)| *class_id);
    classes
}

fn class_is_same_or_descends_from(class_name: &str, base_class: &str, ctx: &Context) -> bool {
    let mut current = Some(class_name);
    while let Some(name) = current {
        if name == base_class {
            return true;
        }
        current = ctx.classes.get(name).and_then(|info| info.parent.as_deref());
    }
    false
}

fn emit_late_bound_class_id_or_lexical_fallback(emitter: &mut Emitter, ctx: &Context) {
    if !dispatch::emit_forwarded_called_class_id(emitter, ctx) {
        let class_id = ctx
            .current_class
            .as_ref()
            .and_then(|name| ctx.classes.get(name))
            .map(|info| info.class_id)
            .unwrap_or(0);
        dispatch::emit_immediate_class_id(emitter, class_id);
    }
}

fn emit_compare_current_class_id(emitter: &mut Emitter, class_id: u64, matched_label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{}", class_id));             // compare the forwarded called-class id against this concrete class id
            emitter.instruction(&format!("b.eq {}", matched_label));            // branch to the matching late-static-binding case
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {}", class_id));             // compare the forwarded called-class id against this concrete class id
            emitter.instruction(&format!("je {}", matched_label));              // branch to the matching late-static-binding case
        }
    }
}

fn emit_late_bound_class_constant(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let classes = sorted_late_bound_classes_by_id(ctx);
    let done_label = ctx.next_label("static_class_done");
    let fallback_name = ctx.current_class.clone().unwrap_or_default();

    emit_late_bound_class_id_or_lexical_fallback(emitter, ctx);
    let mut cases = Vec::new();
    for (_, class_id) in &classes {
        let label = ctx.next_label("static_class_case");
        emit_compare_current_class_id(emitter, *class_id, &label);
        cases.push(label);
    }

    scalars::emit_string_literal(&fallback_name, emitter, data);
    abi::emit_jump(emitter, &done_label);                                       // skip late-static-binding class-name cases after using the lexical fallback

    for ((class_name, _), label) in classes.into_iter().zip(cases) {
        emitter.label(&label);
        scalars::emit_string_literal(&class_name, emitter, data);
        abi::emit_jump(emitter, &done_label);                                   // finish after materializing the matched late-bound class name
    }

    emitter.label(&done_label);
    PhpType::Str
}

fn emit_late_bound_new_static(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let classes = sorted_late_bound_classes_by_id(ctx);
    let done_label = ctx.next_label("new_static_done");
    let fallback_class = ctx.current_class.clone().unwrap_or_default();

    emit_late_bound_class_id_or_lexical_fallback(emitter, ctx);
    let mut cases = Vec::new();
    for (_, class_id) in &classes {
        let label = ctx.next_label("new_static_case");
        emit_compare_current_class_id(emitter, *class_id, &label);
        cases.push(label);
    }

    if !fallback_class.is_empty() {
        allocation::emit_new_object(&fallback_class, args, emitter, ctx, data);
    }
    abi::emit_jump(emitter, &done_label);                                       // skip concrete new-static cases after the lexical fallback

    for ((class_name, _), label) in classes.into_iter().zip(cases) {
        emitter.label(&label);
        allocation::emit_new_object(&class_name, args, emitter, ctx, data);
        abi::emit_jump(emitter, &done_label);                                   // finish after constructing the matched late-bound class
    }

    emitter.label(&done_label);
    PhpType::Object(fallback_class)
}

pub(super) fn emit_property_access(
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    access::emit_property_access(object, property, emitter, ctx, data)
}

pub(super) fn emit_nullsafe_property_access(
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    nullsafe::emit_nullsafe_property_access(object, property, emitter, ctx, data)
}

pub(super) fn emit_static_property_access(
    receiver: &StaticReceiver,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    static_properties::emit_static_property_access(receiver, property, emitter, ctx)
}

pub(super) fn emit_enum_case(
    enum_name: &str,
    case_name: &str,
    emitter: &mut Emitter,
    _ctx: &mut Context,
) -> PhpType {
    let label = crate::names::enum_case_symbol(enum_name, case_name);
    emitter.comment(&format!("load enum case {}::{}", enum_name, case_name));
    crate::codegen::abi::emit_load_symbol_to_reg(
        emitter,
        crate::codegen::abi::int_result_reg(emitter),
        &label,
        0,
    ); // load the enum singleton pointer from its global slot through the target-aware symbol helper
    PhpType::Object(enum_name.to_string())
}

pub(super) fn push_magic_property_name_arg(
    property: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (label, len) = data.add_string(property.as_bytes());
    let (ptr_reg, len_reg) = crate::codegen::abi::string_result_regs(emitter);
    crate::codegen::abi::emit_symbol_address(emitter, ptr_reg, &label); // materialize the magic-property name string address for the active target ABI
    crate::codegen::abi::emit_load_int_immediate(emitter, len_reg, len as i64); // materialize the magic-property name length for the active target ABI
    crate::codegen::abi::emit_push_reg_pair(emitter, ptr_reg, len_reg); // push the magic-property name argument pair onto the temporary call stack
}

pub(super) fn emit_method_call(
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    dispatch::emit_method_call(object, method, args, emitter, ctx, data)
}

pub(super) fn emit_nullsafe_method_call(
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    nullsafe::emit_nullsafe_method_call(object, method, args, emitter, ctx, data)
}

pub(super) fn emit_method_call_with_pushed_args(
    class_name: &str,
    method: &str,
    arg_types: &[PhpType],
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    dispatch::emit_method_call_with_pushed_args(class_name, method, arg_types, emitter, ctx)
}

pub(super) fn emit_static_method_call(
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    dispatch::emit_static_method_call(receiver, method, args, emitter, ctx, data)
}

pub(super) fn emit_instanceof(
    value: &Expr,
    target: &Name,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    instanceof::emit_instanceof(value, target, emitter, ctx, data)
}
