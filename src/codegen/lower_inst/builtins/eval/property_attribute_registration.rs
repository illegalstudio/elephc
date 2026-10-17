//! Purpose:
//! Emits property-default and member-attribute metadata for eval.
//!
//! Called from:
//! - The eval lowering facade and sibling eval support modules.
//!
//! Key details:
//! - Attribute records retain their compact tagged binary encoding.

use super::*;

/// Emits one native property-default metadata registration call into the eval context.
pub(super) fn register_eval_native_property_default(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativePropertyDefaultRegistration,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let property_key = format!(
        "{}::{}",
        registration.class_name, registration.property_name
    );
    let (property_key_label, property_key_len) = ctx.data.add_string(property_key.as_bytes());
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &property_key_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        property_key_len as i64,
    );
    let default = match &registration.default {
        EvalNativePropertyDefault::Value(default) => default,
        EvalNativePropertyDefault::DeferredError(message) => {
            let (label, len) = ctx.data.add_string(message.as_bytes());
            abi::emit_symbol_address(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 3), &label);
            abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 4), len as i64);
            let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_register_native_property_default_error");
            abi::emit_call_label(ctx.emitter, &symbol);
            return;
        }
    };
    let symbol = match default {
        EvalNativeCallableDefault::Scalar { kind, payload } => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 3),
                *kind,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                *payload,
            );
            ctx.emitter
                .target
                .extern_symbol("__elephc_eval_register_native_property_default_scalar")
        }
        EvalNativeCallableDefault::String(value) => {
            let (default_label, default_len) = ctx.data.add_string(value.as_bytes());
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 3),
                &default_label,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                default_len as i64,
            );
            ctx.emitter
                .target
                .extern_symbol("__elephc_eval_register_native_property_default_string")
        }
        EvalNativeCallableDefault::Array(_) => {
            let spec = encode_eval_native_array_default(default);
            let (default_label, default_len) = ctx.data.add_string(&spec);
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 3),
                &default_label,
            );
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_arg_reg_name(ctx.emitter.target, 4),
                default_len as i64,
            );
            ctx.emitter
                .target
                .extern_symbol("__elephc_eval_register_native_property_default_array")
        }
        EvalNativeCallableDefault::Object { .. } => return,
    };
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Emits one native member-attribute metadata registration call into the eval context.
pub(super) fn register_eval_native_member_attribute(
    ctx: &mut FunctionContext<'_>,
    context_offset: usize,
    registration: &EvalNativeMemberAttributeRegistration,
) {
    load_eval_context_local_to_arg(ctx, context_offset, 0);
    let record = eval_native_member_attribute_record(registration);
    let (record_label, record_len) = ctx.data.add_string(&record);
    abi::emit_symbol_address(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        &record_label,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        record_len as i64,
    );
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_register_native_member_attribute");
    abi::emit_call_label(ctx.emitter, &symbol);
}

/// Encodes one member-attribute registration record for the eval bridge ABI.
pub(super) fn eval_native_member_attribute_record(
    registration: &EvalNativeMemberAttributeRegistration,
) -> Vec<u8> {
    let mut record = Vec::new();
    record.push(registration.owner_kind);
    let member_key = if registration.owner_kind == NATIVE_MEMBER_ATTRIBUTE_CLASS {
        registration.class_name.clone()
    } else {
        format!("{}::{}", registration.class_name, registration.member_name)
    };
    eval_native_member_attribute_push_string(&mut record, &member_key);
    eval_native_member_attribute_push_string(&mut record, &registration.attribute_name);
    match &registration.attribute_args {
        Some(args) => {
            record.push(NATIVE_ATTRIBUTE_ARGS_SUPPORTED);
            eval_native_member_attribute_push_u32(&mut record, args.len());
            for arg in args {
                eval_native_member_attribute_push_entry(&mut record, arg);
            }
        }
        None => record.push(NATIVE_ATTRIBUTE_ARGS_UNSUPPORTED),
    }
    record
}

/// Returns true when an attribute argument list can be encoded for eval registration.
pub(super) fn eval_native_member_attribute_args_supported(args: &[AttrArgEntry]) -> bool {
    args.iter()
        .all(|entry| eval_native_member_attribute_value_supported(&entry.value))
}

/// Returns true when one attribute argument value can be encoded for eval registration.
pub(super) fn eval_native_member_attribute_value_supported(value: &AttrArgValue) -> bool {
    match value {
        AttrArgValue::ConstRef(_) | AttrArgValue::ScopedConst(_, _) => false,
        AttrArgValue::Array(elements) => eval_native_member_attribute_args_supported(elements),
        AttrArgValue::Null
        | AttrArgValue::Bool(_)
        | AttrArgValue::Int(_)
        | AttrArgValue::Float(_)
        | AttrArgValue::Str(_) => true,
    }
}

/// Encodes one keyed attribute argument entry into a member-attribute registration record.
pub(super) fn eval_native_member_attribute_push_entry(record: &mut Vec<u8>, entry: &AttrArgEntry) {
    match &entry.key {
        Some(AttrKey::Str(name)) => {
            record.push(NATIVE_ATTRIBUTE_ARG_NAMED);
            eval_native_member_attribute_push_string(record, name);
            eval_native_member_attribute_push_arg(record, &entry.value);
        }
        Some(AttrKey::Int(_)) | None => eval_native_member_attribute_push_arg(record, &entry.value),
    }
}

/// Encodes one attribute argument value into a member-attribute registration record.
pub(super) fn eval_native_member_attribute_push_arg(record: &mut Vec<u8>, arg: &AttrArgValue) {
    match arg {
        AttrArgValue::Null => record.push(NATIVE_ATTRIBUTE_ARG_NULL),
        AttrArgValue::Bool(value) => {
            record.push(NATIVE_ATTRIBUTE_ARG_BOOL);
            record.push(u8::from(*value));
        }
        AttrArgValue::Int(value) => {
            record.push(NATIVE_ATTRIBUTE_ARG_INT);
            record.extend_from_slice(&value.to_le_bytes());
        }
        AttrArgValue::Float(bits) => {
            record.push(NATIVE_ATTRIBUTE_ARG_FLOAT);
            record.extend_from_slice(&bits.to_le_bytes());
        }
        AttrArgValue::Str(value) => {
            record.push(NATIVE_ATTRIBUTE_ARG_STRING);
            eval_native_member_attribute_push_string(record, value);
        }
        AttrArgValue::Array(elements) => {
            record.push(NATIVE_ATTRIBUTE_ARG_ARRAY);
            eval_native_member_attribute_push_u32(record, elements.len());
            for element in elements {
                eval_native_member_attribute_push_entry(record, element);
            }
        }
        AttrArgValue::ConstRef(_) | AttrArgValue::ScopedConst(_, _) => {
            record.push(NATIVE_ATTRIBUTE_ARGS_UNSUPPORTED);
        }
    }
}

/// Encodes one length-prefixed UTF-8 string into a member-attribute registration record.
pub(super) fn eval_native_member_attribute_push_string(record: &mut Vec<u8>, value: &str) {
    eval_native_member_attribute_push_u32(record, value.len());
    record.extend_from_slice(value.as_bytes());
}

/// Encodes one little-endian u32 length into a member-attribute registration record.
pub(super) fn eval_native_member_attribute_push_u32(record: &mut Vec<u8>, value: usize) {
    let value = u32::try_from(value).unwrap_or(u32::MAX);
    record.extend_from_slice(&value.to_le_bytes());
}
