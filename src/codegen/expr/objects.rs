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
