//! Purpose:
//! Resolves descriptor invoker parameter defaults and materializes them at runtime.
//! Carries the already-folded constant tree into the invoker so no AST reaches assembly.
//!
//! Called from:
//! - `crate::codegen::runtime_callable_invoker` argument staging.
//! - Every descriptor invoker emission site, through `resolve_invoker_defaults()`.
//!
//! Key details:
//! - Resolution is the SHARED folder in `crate::codegen::const_default_values`, the same one the
//!   eval registration path uses, so a default eval reports through Reflection and a default this
//!   invoker materializes can never disagree.
//! - A default is kept only when it is materializable end to end. Anything else stays `None` and
//!   the invoker keeps its existing fatal diagnostic; nothing is ever replaced by null.
//! - Every refcounted default is FRESHLY allocated per invocation and hands the caller exactly one
//!   owner, which `InvokerArgumentOwners` adopts and releases on the normal and escape paths alike.
//! - Object defaults are constructed through the same pair of runtime entry points eval already
//!   uses to build an AOT class: `__rt_new_by_name` allocates and
//!   `__elephc_eval_value_construct_object` runs the real constructor.
//! - Object constructor arguments are normalized from their source call shape to the physical
//!   constructor signature before assembly emission. The eval constructor bridge therefore sees
//!   the same regular, variadic, and hidden slots that Magician's native binder would produce.

use crate::codegen::const_default_values::{
    resolve_const_default, ConstDefaultArrayKey, ConstDefaultContext, ConstDefaultObjectArg,
    ConstDefaultValue, CONST_DEFAULT_BOOL, CONST_DEFAULT_EMPTY_ARRAY, CONST_DEFAULT_FLOAT,
    CONST_DEFAULT_INT, CONST_DEFAULT_NULL,
};
use crate::codegen::platform::Arch;
use crate::codegen::runtime_value_tag;
use crate::codegen::{emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed};
use crate::ir::Module;
use crate::types::{FunctionSig, PhpType};

use super::{abi, DataSection, Emitter, InvokerEmitContext, InvokerParamShape};

/// One parameter's resolved default, aligned with `FunctionSig::params` by index.
pub(in crate::codegen) type InvokerDefaults = Vec<Option<InvokerDefaultValue>>;

/// One descriptor-invoker default after nested object calls have been physically normalized.
#[derive(Clone, PartialEq)]
pub(in crate::codegen) enum InvokerDefaultValue {
    Scalar { kind: i64, payload: i64 },
    String(String),
    Array(Vec<InvokerDefaultArrayElement>),
    Object {
        class_name: String,
        args: Vec<InvokerDefaultObjectArg>,
    },
}

/// One array element whose nested value is ready for invoker materialization.
#[derive(Clone, PartialEq)]
pub(in crate::codegen) struct InvokerDefaultArrayElement {
    key: Option<ConstDefaultArrayKey>,
    default: InvokerDefaultValue,
}

/// One physical constructor slot plus the storage type the raw constructor expects.
#[derive(Clone, PartialEq)]
pub(in crate::codegen) struct InvokerDefaultObjectArg {
    default: InvokerDefaultValue,
    target_ty: PhpType,
}

/// Resolves every parameter default of one signature into a materializable constant value.
///
/// `current_class` is the class that DECLARES the callee, which is what `self::`, `static::` and
/// `parent::` in a default expression resolve against. A free function passes `None`.
pub(in crate::codegen) fn resolve_invoker_defaults(
    module: &Module,
    current_class: Option<&str>,
    sig: &FunctionSig,
) -> InvokerDefaults {
    let context = ConstDefaultContext {
        module,
        current_class,
    };
    sig.defaults
        .iter()
        .map(|default| {
            let value = resolve_const_default(default.as_ref()?, &context)?;
            let value = normalize_invoker_default(module, value)?;
            materializable(module, &value).then_some(value)
        })
        .collect()
}

/// Recursively converts shared source defaults into descriptor-invoker materialization values.
fn normalize_invoker_default(
    module: &Module,
    value: ConstDefaultValue,
) -> Option<InvokerDefaultValue> {
    match value {
        ConstDefaultValue::Scalar { kind, payload } => {
            Some(InvokerDefaultValue::Scalar { kind, payload })
        }
        ConstDefaultValue::String(value) => Some(InvokerDefaultValue::String(value)),
        ConstDefaultValue::Array(elements) => Some(InvokerDefaultValue::Array(
            elements
                .into_iter()
                .map(|element| {
                    Some(InvokerDefaultArrayElement {
                        key: element.key,
                        default: normalize_invoker_default(module, element.default)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        )),
        ConstDefaultValue::Object { class_name, args } => {
            let args = normalize_object_constructor_args(module, &class_name, args)?;
            Some(InvokerDefaultValue::Object { class_name, args })
        }
    }
}

/// Binds source object-default arguments to the constructor's complete physical parameter list.
fn normalize_object_constructor_args(
    module: &Module,
    class_name: &str,
    args: Vec<ConstDefaultObjectArg>,
) -> Option<Vec<InvokerDefaultObjectArg>> {
    let (resolved_class, _) =
        crate::codegen::const_default_values::resolve_const_default_class(module, class_name)?;
    let constructor_key = crate::names::php_symbol_key("__construct");
    let Some((owner_class, owner_info)) =
        crate::types::constructor_owner(&module.class_infos, resolved_class)
    else {
        return args.is_empty().then(Vec::new);
    };
    let sig = owner_info.methods.get(&constructor_key)?.clone();
    let declaring_class = owner_info
        .method_impl_classes
        .get(&constructor_key)
        .map(String::as_str)
        .unwrap_or(owner_class)
        .to_string();
    normalize_object_args_for_signature(module, &declaring_class, &sig, args)
}

/// Applies PHP positional/named/default/variadic binding to one physical constructor signature.
fn normalize_object_args_for_signature(
    module: &Module,
    declaring_class: &str,
    sig: &FunctionSig,
    args: Vec<ConstDefaultObjectArg>,
) -> Option<Vec<InvokerDefaultObjectArg>> {
    let shape = InvokerParamShape::of(sig);
    let variadic_index = crate::types::signatures::variadic_param_index(sig);
    let source_variadic = sig
        .variadic
        .as_deref()
        .is_some_and(|name| name != crate::func_args::HIDDEN_ARGS_PARAM);
    let mut regular = vec![None; shape.visible_regular];
    let mut tail = Vec::new();
    let mut next_positional = 0usize;
    let mut highest_regular = 0usize;
    let mut positional_surplus = 0usize;
    let mut saw_named = false;

    for arg in args {
        let value = normalize_invoker_default(module, arg.default)?;
        if let Some(name) = arg.name {
            saw_named = true;
            if let Some(index) = crate::types::call_args::named_param_index(
                sig,
                shape.visible_regular,
                &name,
            ) {
                if regular[index].replace(value).is_some() {
                    return None;
                }
                highest_regular = highest_regular.max(index + 1);
                continue;
            }
            if !source_variadic
                || !crate::types::signatures::variadic_storage_accepts_named_entries(sig)
            {
                return None;
            }
            tail.push(InvokerDefaultArrayElement {
                key: Some(ConstDefaultArrayKey::String(name)),
                default: value,
            });
            continue;
        }
        if saw_named {
            return None;
        }
        if next_positional < shape.visible_regular {
            regular[next_positional] = Some(value);
            next_positional += 1;
            highest_regular = highest_regular.max(next_positional);
        } else if variadic_index.is_some() {
            tail.push(InvokerDefaultArrayElement {
                key: None,
                default: value,
            });
            positional_surplus += 1;
        }
    }

    let default_context = ConstDefaultContext::for_class(module, declaring_class);
    for (index, slot) in regular.iter_mut().enumerate() {
        if slot.is_some() {
            continue;
        }
        let default = resolve_const_default(sig.defaults.get(index)?.as_ref()?, &default_context)?;
        *slot = Some(normalize_invoker_default(module, default)?);
    }

    let actual_count = highest_regular + positional_surplus;
    if shape.collector_needs_count {
        tail.insert(
            0,
            InvokerDefaultArrayElement {
                key: None,
                default: InvokerDefaultValue::Scalar {
                    kind: CONST_DEFAULT_INT,
                    payload: actual_count as i64,
                },
            },
        );
    }
    let mut physical = Vec::with_capacity(sig.params.len());
    for (index, (name, target_ty)) in sig.params.iter().enumerate() {
        let default = if index < shape.visible_regular {
            regular[index].take()?
        } else if name == crate::func_args::HIDDEN_ARGC_PARAM {
            InvokerDefaultValue::Scalar {
                kind: CONST_DEFAULT_INT,
                payload: actual_count as i64,
            }
        } else if Some(index) == variadic_index {
            if tail.iter().any(|element| {
                matches!(element.key, Some(ConstDefaultArrayKey::String(_)))
            }) && matches!(target_ty.codegen_repr(), PhpType::Array(_))
            {
                // The callee can use `Array<Mixed>` as dynamic collector storage, but the eval
                // constructor bridge still validates an Array parameter as indexed tag 4. Do
                // not emit a hash that the bridge would reject after allocation.
                return None;
            }
            InvokerDefaultValue::Array(std::mem::take(&mut tail))
        } else {
            return None;
        };
        physical.push(InvokerDefaultObjectArg {
            default,
            target_ty: target_ty.clone(),
        });
    }
    Some(physical)
}

/// Reports whether one resolved default can be built by the runtime materializer below.
///
/// Declining here is what keeps the invoker honest: an unrepresentable default keeps the existing
/// fatal diagnostic instead of quietly becoming null or a half-built value.
fn materializable(module: &Module, value: &InvokerDefaultValue) -> bool {
    match value {
        InvokerDefaultValue::Scalar { .. } | InvokerDefaultValue::String(_) => true,
        InvokerDefaultValue::Array(elements) => elements
            .iter()
            .all(|element| materializable(module, &element.default)),
        InvokerDefaultValue::Object { class_name, args } => {
            // The normalized bridge container is positional and physical. It exists only in a
            // module that uses eval, which is also the only module whose constructors it knows.
            crate::codegen::eval_constructor_helpers::module_emits_eval_constructor_bridge(module)
                && constructible_class(module, class_name)
                && args
                    .iter()
                    .all(|arg| materializable(module, &arg.default))
        }
    }
}

/// Reports whether `__rt_new_by_name` alone produces a correctly initialized instance.
///
/// The by-name allocator stamps the class id and ZEROES the property region; it does not run
/// property initializers, which eval applies from its own registered property-default metadata.
/// A class whose properties carry initializers therefore stays off this path rather than being
/// constructed with silently blank defaults.
fn constructible_class(module: &Module, class_name: &str) -> bool {
    let Some((_, class_info)) =
        crate::codegen::const_default_values::resolve_const_default_class(module, class_name)
    else {
        return false;
    };
    class_info.defaults.iter().all(Option::is_none)
}

/// Emits one resolved default into the canonical result registers, returning its produced type.
pub(super) fn emit_const_default_to_result(
    default: &InvokerDefaultValue,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    match default {
        InvokerDefaultValue::Scalar {
            kind: CONST_DEFAULT_NULL,
            ..
        } => super::emit_null_default_to_result(emitter, target_ty),
        InvokerDefaultValue::Scalar {
            kind: CONST_DEFAULT_BOOL,
            payload,
        } => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), *payload);
            PhpType::Bool
        }
        InvokerDefaultValue::Scalar {
            kind: CONST_DEFAULT_INT,
            payload,
        } => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), *payload);
            PhpType::Int
        }
        InvokerDefaultValue::Scalar {
            kind: CONST_DEFAULT_FLOAT,
            payload,
        } => {
            super::emit_float_literal_to_result(emitter, data, f64::from_bits(*payload as u64));
            PhpType::Float
        }
        InvokerDefaultValue::Scalar {
            kind: CONST_DEFAULT_EMPTY_ARRAY,
            ..
        } => {
            let elem_ty = target_indexed_elem_ty(target_ty).unwrap_or(PhpType::Mixed);
            super::emit_empty_indexed_array(emitter, &elem_ty);
            PhpType::Array(Box::new(elem_ty))
        }
        InvokerDefaultValue::String(value) => {
            let (label, len) = data.add_string(value.as_bytes());
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_symbol_address(emitter, ptr_reg, &label);
            abi::emit_load_int_immediate(emitter, len_reg, len as i64);
            PhpType::Str
        }
        InvokerDefaultValue::Array(elements) => {
            emit_array_default(elements, target_ty, emitter, ctx, data)
        }
        InvokerDefaultValue::Object { class_name, args } => {
            emit_object_default(class_name, args, emitter, ctx, data)
        }
        InvokerDefaultValue::Scalar { .. } => {
            super::emit_unsupported_default_abort(emitter, data, ctx);
            PhpType::Void
        }
    }
}

/// Returns the element type of a strictly indexed target slot.
fn target_indexed_elem_ty(target_ty: Option<&PhpType>) -> Option<PhpType> {
    match target_ty?.codegen_repr() {
        PhpType::Array(elem) => Some(*elem),
        _ => None,
    }
}

/// Returns each element's final PHP key, applying PHP's auto-index rule to unkeyed entries.
fn resolved_array_keys(elements: &[InvokerDefaultArrayElement]) -> Vec<ConstDefaultArrayKey> {
    let mut next_index = 0i64;
    let mut keys = Vec::with_capacity(elements.len());
    for element in elements {
        let key = match &element.key {
            Some(ConstDefaultArrayKey::Int(value)) => ConstDefaultArrayKey::Int(*value),
            Some(ConstDefaultArrayKey::String(value)) => {
                ConstDefaultArrayKey::String(value.clone())
            }
            None => ConstDefaultArrayKey::Int(next_index),
        };
        if let ConstDefaultArrayKey::Int(value) = &key {
            next_index = value.saturating_add(1).max(next_index);
        }
        keys.push(key);
    }
    keys
}

/// Emits a nonempty array default, choosing the storage shape the target slot declares.
fn emit_array_default(
    elements: &[InvokerDefaultArrayElement],
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let keys = resolved_array_keys(elements);
    let is_list = keys
        .iter()
        .enumerate()
        .all(|(index, key)| matches!(key, ConstDefaultArrayKey::Int(value) if *value == index as i64));
    if let Some(elem_ty) = target_indexed_elem_ty(target_ty) {
        // A strictly indexed slot cannot hold a sparse or string-keyed literal, and silently
        // reshaping it would change what the callee reads back.
        if !is_list {
            super::emit_unsupported_default_abort(emitter, data, ctx);
            return PhpType::Void;
        }
        emit_indexed_array_default(elements, &elem_ty, emitter, ctx, data);
        return PhpType::Array(Box::new(elem_ty));
    }
    emit_hash_array_default(elements, &keys, emitter, ctx, data);
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    }
}

/// Emits an indexed array default whose elements land in declared element storage.
fn emit_indexed_array_default(
    elements: &[InvokerDefaultArrayElement],
    elem_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) {
    let capacity = elements.len().max(4);
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 0),
        capacity as i64,
    );
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 1),
        elem_ty.stack_size() as i64,
    );
    abi::emit_call_label(emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(emitter, abi::int_result_reg(emitter), elem_ty);
    // The array pointer owns exactly ONE temporary stack slot for the whole fill: every append
    // helper pops that slot and pushes the pointer the runtime returned after a possible regrow.
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    for element in elements {
        let value_ty = emit_const_default_to_result(&element.default, None, emitter, ctx, data);
        append_indexed_array_element(emitter, elem_ty, &element.default, &value_ty);
    }
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
}

/// Appends the current result to the indexed array whose pointer is the top pushed word.
fn append_indexed_array_element(
    emitter: &mut Emitter,
    elem_ty: &PhpType,
    default: &InvokerDefaultValue,
    value_ty: &PhpType,
) {
    match elem_ty.codegen_repr() {
        PhpType::Str => append_indexed_string_element(emitter),
        PhpType::Float => append_indexed_float_element(emitter),
        PhpType::Int | PhpType::Bool => append_indexed_scalar_element(emitter),
        _ => {
            box_const_default_for_container(emitter, default, value_ty);
            append_indexed_refcounted_element(emitter);
        }
    }
}

/// Boxes one constant for a container without transferring borrowed static string storage.
fn box_const_default_for_container(
    emitter: &mut Emitter,
    default: &InvokerDefaultValue,
    value_ty: &PhpType,
) {
    if value_ty.is_refcounted() && !matches!(default, InvokerDefaultValue::String(_)) {
        emit_box_current_owned_value_as_mixed(emitter, value_ty);
    } else {
        emit_box_current_value_as_mixed(emitter, &value_ty.codegen_repr());
    }
}

/// Appends the current integer-like result to the pushed indexed array pointer.
fn append_indexed_scalar_element(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // pass the scalar default payload to the indexed-array append helper
            abi::emit_pop_reg(emitter, "x0");
            abi::emit_call_label(emitter, "__rt_array_push_int");
            abi::emit_push_reg(emitter, "x0");
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // pass the scalar default payload to the indexed-array append helper
            abi::emit_pop_reg(emitter, "rdi");
            abi::emit_call_label(emitter, "__rt_array_push_int");
            abi::emit_push_reg(emitter, "rax");
        }
    }
}

/// Appends the current floating-point result to the pushed indexed array pointer.
fn append_indexed_float_element(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fmov x1, d0");                                 // pass the float default bits to the indexed-array append helper
            abi::emit_pop_reg(emitter, "x0");
            abi::emit_call_label(emitter, "__rt_array_push_int");
            abi::emit_push_reg(emitter, "x0");
        }
        Arch::X86_64 => {
            emitter.instruction("movq rsi, xmm0");                              // pass the float default bits to the indexed-array append helper
            abi::emit_pop_reg(emitter, "rdi");
            abi::emit_call_label(emitter, "__rt_array_push_int");
            abi::emit_push_reg(emitter, "rax");
        }
    }
}

/// Appends the current string result to the pushed indexed array pointer.
fn append_indexed_string_element(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(emitter, "x0");
            abi::emit_call_label(emitter, "__rt_array_push_str");
            abi::emit_push_reg(emitter, "x0");
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // pass the string default pointer to the indexed-array append helper
            abi::emit_pop_reg(emitter, "rdi");
            abi::emit_call_label(emitter, "__rt_array_push_str");
            abi::emit_push_reg(emitter, "rax");
        }
    }
}

/// Appends the current boxed Mixed result and retires the temporary owner after insertion.
fn append_indexed_refcounted_element(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(emitter, "x9");
            abi::emit_push_reg(emitter, "x0");
            emitter.instruction("mov x1, x0");                                  // pass the boxed default payload to the refcounted append helper
            emitter.instruction("mov x0, x9");                                  // pass the saved indexed-array pointer to the refcounted append helper
            abi::emit_call_label(emitter, "__rt_array_push_refcounted");
            crate::codegen::emit_release_pushed_refcounted_temp_after_array_push(
                emitter,
                &PhpType::Mixed,
            );
            abi::emit_push_reg(emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(emitter, "r11");
            abi::emit_push_reg(emitter, "rax");
            emitter.instruction("mov rsi, rax");                                // pass the boxed default payload to the refcounted append helper
            emitter.instruction("mov rdi, r11");                                // pass the saved indexed-array pointer to the refcounted append helper
            abi::emit_call_label(emitter, "__rt_array_push_refcounted");
            crate::codegen::emit_release_pushed_refcounted_temp_after_array_push(
                emitter,
                &PhpType::Mixed,
            );
            abi::emit_push_reg(emitter, "rax");
        }
    }
}

/// Emits a hash-backed array default holding Mixed values under their resolved PHP keys.
///
/// The hash pointer travels through the result register across insertions because the runtime may
/// reallocate it on growth, and each key is staged on the temporary stack while its value is
/// materialized so value-staging calls cannot clobber it.
fn emit_hash_array_default(
    elements: &[InvokerDefaultArrayElement],
    keys: &[ConstDefaultArrayKey],
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) {
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 16);
    abi::emit_load_int_immediate(
        emitter,
        abi::int_arg_reg_name(emitter.target, 1),
        runtime_value_tag(&PhpType::Mixed) as i64,
    );
    abi::emit_call_label(emitter, "__rt_hash_new");
    for (element, key) in elements.iter().zip(keys) {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
        match emitter.target.arch {
            Arch::AArch64 => {
                emit_hash_key_aarch64(emitter, data, key);
                abi::emit_push_reg_pair(emitter, "x1", "x2");
                let value_ty =
                    emit_const_default_to_result(&element.default, None, emitter, ctx, data);
                emit_hash_value_aarch64(emitter, &value_ty);
                abi::emit_pop_reg_pair(emitter, "x1", "x2");
                abi::emit_pop_reg(emitter, "x0");
                abi::emit_load_int_immediate(
                    emitter,
                    "x5",
                    runtime_value_tag(&value_ty.codegen_repr()) as i64,
                );
            }
            Arch::X86_64 => {
                emit_hash_key_x86_64(emitter, data, key);
                abi::emit_push_reg_pair(emitter, "rsi", "rdx");
                let value_ty =
                    emit_const_default_to_result(&element.default, None, emitter, ctx, data);
                emit_hash_value_x86_64(emitter, &value_ty);
                abi::emit_pop_reg_pair(emitter, "rsi", "rdx");
                abi::emit_pop_reg(emitter, "rdi");
                abi::emit_load_int_immediate(
                    emitter,
                    "r9",
                    runtime_value_tag(&value_ty.codegen_repr()) as i64,
                );
            }
        }
        abi::emit_call_label(emitter, "__rt_hash_set");
    }
}

/// Materializes one resolved hash key into the AArch64 key registers `x1`/`x2`.
fn emit_hash_key_aarch64(emitter: &mut Emitter, data: &mut DataSection, key: &ConstDefaultArrayKey) {
    match key {
        ConstDefaultArrayKey::Int(value) => {
            abi::emit_load_int_immediate(emitter, "x1", *value);
            abi::emit_load_int_immediate(emitter, "x2", -1);
        }
        ConstDefaultArrayKey::String(value) => {
            let (label, len) = data.add_string(value.as_bytes());
            abi::emit_symbol_address(emitter, "x1", &label);
            abi::emit_load_int_immediate(emitter, "x2", len as i64);
            abi::emit_call_label(emitter, "__rt_hash_normalize_key");
        }
    }
}

/// Materializes one resolved hash key into the x86_64 key registers `rsi`/`rdx`.
fn emit_hash_key_x86_64(emitter: &mut Emitter, data: &mut DataSection, key: &ConstDefaultArrayKey) {
    match key {
        ConstDefaultArrayKey::Int(value) => {
            abi::emit_load_int_immediate(emitter, "rsi", *value);
            abi::emit_load_int_immediate(emitter, "rdx", -1);
        }
        ConstDefaultArrayKey::String(value) => {
            let (label, len) = data.add_string(value.as_bytes());
            abi::emit_symbol_address(emitter, "rax", &label);
            abi::emit_load_int_immediate(emitter, "rdx", len as i64);
            abi::emit_call_label(emitter, "__rt_hash_normalize_key");
            emitter.instruction("mov rsi, rax");                                // move the normalized key low word into the hash ABI key register
        }
    }
}

/// Materializes the current result as the AArch64 `__rt_hash_set` value payload `x3`/`x4`.
fn emit_hash_value_aarch64(emitter: &mut Emitter, value_ty: &PhpType) {
    match value_ty.codegen_repr() {
        PhpType::Float => {
            emitter.instruction("fmov x3, d0");                                 // pass the float default bits as the hash value low word
            emitter.instruction("mov x4, xzr");                                 // float hash values do not use the high payload word
        }
        PhpType::Str => {
            abi::emit_call_label(emitter, "__rt_str_persist");
            emitter.instruction("mov x3, x1");                                  // transfer the persistent string pointer as the hash value low word
            emitter.instruction("mov x4, x2");                                  // pass the string length as the hash value high word
        }
        PhpType::Void | PhpType::Never => {
            emitter.instruction("mov x3, xzr");                                 // null hash values use a zero low payload word
            emitter.instruction("mov x4, xzr");                                 // null hash values use a zero high payload word
        }
        _ => {
            emitter.instruction("mov x3, x0");                                  // transfer the default payload as the hash value low word
            emitter.instruction("mov x4, xzr");                                 // single-word hash values do not use the high payload word
        }
    }
}

/// Materializes the current result as the x86_64 `__rt_hash_set` value payload `rcx`/`r8`.
fn emit_hash_value_x86_64(emitter: &mut Emitter, value_ty: &PhpType) {
    match value_ty.codegen_repr() {
        PhpType::Float => {
            emitter.instruction("movq rcx, xmm0");                              // pass the float default bits as the hash value low word
            emitter.instruction("xor r8, r8");                                  // float hash values do not use the high payload word
        }
        PhpType::Str => {
            abi::emit_call_label(emitter, "__rt_str_persist");
            emitter.instruction("mov rcx, rax");                                // transfer the persistent string pointer as the hash value low word
            emitter.instruction("mov r8, rdx");                                 // pass the string length as the hash value high word
        }
        PhpType::Void | PhpType::Never => {
            emitter.instruction("xor rcx, rcx");                                // null hash values use a zero low payload word
            emitter.instruction("xor r8, r8");                                  // null hash values use a zero high payload word
        }
        _ => {
            emitter.instruction("mov rcx, rax");                                // transfer the default payload as the hash value low word
            emitter.instruction("xor r8, r8");                                  // single-word hash values do not use the high payload word
        }
    }
}

/// Emits an object default: allocate by name, run the real constructor, keep exactly one owner.
///
/// The constructor bridge takes a boxed Mixed receiver and a boxed Mixed argument container in the
/// normalized indexed-of-boxed-Mixed shape, and answers 1 for success. It runs under its own
/// exception boundary, so a throwing constructor comes back as a failure with the throwable
/// pending, which this rethrows into the invoker's boundary instead of inventing a value.
fn emit_object_default(
    class_name: &str,
    args: &[InvokerDefaultObjectArg],
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) -> PhpType {
    let object_ty = PhpType::Object(class_name.to_string());
    let fail_label = ctx.next_label("invoker_object_default_fail");
    let done_label = ctx.next_label("invoker_object_default_done");
    let (name_label, name_len) = data.add_string(class_name.as_bytes());
    let bridge = emitter
        .target
        .extern_symbol("__elephc_eval_value_construct_object");

    // -- stage the constructor argument container first, so the receiver stays on top --
    emit_object_default_args(args, emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));

    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", &name_label);
            abi::emit_load_int_immediate(emitter, "x2", name_len as i64);
            abi::emit_call_label(emitter, "__rt_new_by_name");
            emitter.instruction(&format!("cbz x0, {}", fail_label));            // an unregistered class cannot be constructed
            emit_box_current_owned_value_as_mixed(emitter, &object_ty);
            abi::emit_push_reg(emitter, "x0");
            abi::emit_pop_reg(emitter, "x0");                                   // boxed receiver
            abi::emit_pop_reg(emitter, "x1");                                   // boxed argument container
            abi::emit_push_reg(emitter, "x1");
            abi::emit_push_reg(emitter, "x0");
            emitter.instruction("mov x2, xzr");                                 // default construction runs without an eval class scope
            emitter.instruction("mov x3, xzr");                                 // the empty class scope has zero length
            emitter.instruction("mov x4, xzr");                                 // no eval context is needed for a constant default
            abi::emit_call_label(emitter, &bridge);
            abi::emit_pop_reg(emitter, "x1");                                   // boxed receiver
            abi::emit_pop_reg(emitter, "x2");                                   // boxed argument container
            abi::emit_push_reg(emitter, "x1");
            abi::emit_push_reg(emitter, "x0");                                  // constructor status
            emitter.instruction("mov x0, x2");                                  // release the argument container box
            abi::emit_call_label(emitter, "__rt_decref_mixed");
            abi::emit_pop_reg(emitter, "x0");                                   // constructor status
            abi::emit_pop_reg(emitter, "x1");                                   // boxed receiver
            abi::emit_push_reg(emitter, "x1");
            emitter.instruction(&format!("cbz x0, {}", fail_label));            // a failed constructor must not yield a half-built object
            abi::emit_pop_reg(emitter, "x0");                                   // boxed receiver
            abi::emit_push_reg(emitter, "x0");
            abi::emit_call_label(emitter, "__rt_mixed_unbox");
            emitter.instruction("mov x0, x1");                                  // the unboxed object pointer is still owned by the box
            abi::emit_call_label(emitter, "__rt_incref");
            abi::emit_push_reg(emitter, "x0");                                  // the invoker's own object owner
            abi::emit_pop_reg(emitter, "x0");
            abi::emit_pop_reg(emitter, "x1");                                   // boxed receiver
            abi::emit_push_reg(emitter, "x0");
            emitter.instruction("mov x0, x1");                                  // release the receiver box, leaving one object owner
            abi::emit_call_label(emitter, "__rt_decref_mixed");
            abi::emit_pop_reg(emitter, "x0");
            abi::emit_jump(emitter, &done_label);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rax", &name_label);
            abi::emit_load_int_immediate(emitter, "rdx", name_len as i64);
            abi::emit_call_label(emitter, "__rt_new_by_name");
            emitter.instruction("test rax, rax");                               // did the class registry allocate the object?
            emitter.instruction(&format!("jz {}", fail_label));                 // an unregistered class cannot be constructed
            emit_box_current_owned_value_as_mixed(emitter, &object_ty);
            abi::emit_push_reg(emitter, "rax");
            abi::emit_pop_reg(emitter, "rdi");                                  // boxed receiver
            abi::emit_pop_reg(emitter, "rsi");                                  // boxed argument container
            abi::emit_push_reg(emitter, "rsi");
            abi::emit_push_reg(emitter, "rdi");
            emitter.instruction("xor edx, edx");                                // default construction runs without an eval class scope
            emitter.instruction("xor ecx, ecx");                                // the empty class scope has zero length
            emitter.instruction("xor r8d, r8d");                                // no eval context is needed for a constant default
            abi::emit_call_label(emitter, &bridge);
            abi::emit_pop_reg(emitter, "rdi");                                  // boxed receiver
            abi::emit_pop_reg(emitter, "rsi");                                  // boxed argument container
            abi::emit_push_reg(emitter, "rdi");
            abi::emit_push_reg(emitter, "rax");                                 // constructor status
            emitter.instruction("mov rax, rsi");                                // release the argument container box
            abi::emit_call_label(emitter, "__rt_decref_mixed");
            abi::emit_pop_reg(emitter, "rax");                                  // constructor status
            abi::emit_pop_reg(emitter, "rdi");                                  // boxed receiver
            abi::emit_push_reg(emitter, "rdi");
            emitter.instruction("test rax, rax");                               // did the constructor bridge report success?
            emitter.instruction(&format!("jz {}", fail_label));                 // a failed constructor must not yield a half-built object
            abi::emit_pop_reg(emitter, "rax");                                  // boxed receiver
            abi::emit_push_reg(emitter, "rax");
            abi::emit_call_label(emitter, "__rt_mixed_unbox");
            emitter.instruction("mov rax, rdi");                                // the unboxed object pointer is still owned by the box
            abi::emit_call_label(emitter, "__rt_incref");
            abi::emit_push_reg(emitter, "rax");                                 // the invoker's own object owner
            abi::emit_pop_reg(emitter, "rax");
            abi::emit_pop_reg(emitter, "rdi");                                  // boxed receiver
            abi::emit_push_reg(emitter, "rax");
            emitter.instruction("mov rax, rdi");                                // release the receiver box, leaving one object owner
            abi::emit_call_label(emitter, "__rt_decref_mixed");
            abi::emit_pop_reg(emitter, "rax");
            abi::emit_jump(emitter, &done_label);
        }
    }

    emitter.label(&fail_label);
    emit_object_default_failure(emitter, data, ctx);
    emitter.label(&done_label);
    object_ty
}

/// Builds the boxed physical argument container the eval constructor bridge consumes.
fn emit_object_default_args(
    args: &[InvokerDefaultObjectArg],
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
) {
    super::emit_empty_indexed_array(emitter, &PhpType::Mixed);
    for arg in args {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
        let value_ty =
            emit_const_default_to_result(&arg.default, Some(&arg.target_ty), emitter, ctx, data);
        box_const_default_for_container(emitter, &arg.default, &value_ty);
        append_indexed_refcounted_element(emitter);
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
    }
    emit_box_current_owned_value_as_mixed(emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
}

/// Rethrows a pending constructor throwable, or reports a fatal when construction simply failed.
fn emit_object_default_failure(
    emitter: &mut Emitter,
    data: &mut DataSection,
    ctx: &mut InvokerEmitContext,
) {
    let fatal_label = ctx.next_label("invoker_object_default_fatal");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", "_exc_value", 0);
            emitter.instruction(&format!("cbz x9, {}", fatal_label));           // no pending throwable means the bridge refused the constructor outright
            abi::emit_jump(emitter, "__rt_throw_current");
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r11", "_exc_value", 0);
            emitter.instruction("test r11, r11");                               // is a constructor throwable pending?
            emitter.instruction(&format!("jz {}", fatal_label));                // no pending throwable means the bridge refused the constructor outright
            abi::emit_jump(emitter, "__rt_throw_current");
        }
    }
    emitter.label(&fatal_label);
    super::emit_unsupported_default_abort(emitter, data, ctx);
}

#[cfg(test)]
mod tests;
