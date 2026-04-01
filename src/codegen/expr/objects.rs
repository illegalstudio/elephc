mod access;
mod allocation;
mod dispatch;

use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
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

pub(super) fn emit_property_access(
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    access::emit_property_access(object, property, emitter, ctx, data)
}

pub(super) fn emit_enum_case(
    enum_name: &str,
    case_name: &str,
    emitter: &mut Emitter,
    _ctx: &mut Context,
) -> PhpType {
    let label = crate::names::enum_case_symbol(enum_name, case_name);
    emitter.comment(&format!("load enum case {}::{}", enum_name, case_name));
    emitter.instruction(&format!("adrp x9, {}@PAGE", label));                   // load page of the enum singleton slot
    emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", label));             // resolve the enum singleton slot address
    emitter.instruction("ldr x0, [x9]");                                        // load the enum singleton pointer from its global slot
    PhpType::Object(enum_name.to_string())
}

pub(super) fn push_magic_property_name_arg(
    property: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (label, len) = data.add_string(property.as_bytes());
    emitter.instruction(&format!("adrp x1, {}@PAGE", label));                   // load page of the magic-property name string
    emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label));             // resolve the magic-property name string address
    emitter.instruction(&format!("mov x2, #{}", len));                          // pass the magic-property name length
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // push the magic-property name argument
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
