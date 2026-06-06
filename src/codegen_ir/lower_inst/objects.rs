//! Purpose:
//! Lowers object metadata opcodes for the Phase 04 EIR backend.
//! Supports simple object allocation, declared property access, and named or dynamic `instanceof` checks.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Object payload layout must match the legacy backend and runtime helpers:
//!   heap kind word before payload, class id at payload offset 0, then 16 bytes
//!   per declared property slot.
//! - This slice intentionally rejects dynamic properties, references, interface
//!   method entries that need missing EIR symbols, and non-literal default
//!   property expressions until their runtime paths land.

use std::collections::HashSet;

use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::{method_symbol, php_symbol_key};
use crate::types::{ClassInfo, InterfaceInfo, PhpType};

use super::super::context::FunctionContext;
use super::{
    cast_loaded_mixed_pointer_to_result, direct_call_stack_pad_bytes, expect_data, expect_operand,
    materialize_direct_call_args, store_if_result,
};
use crate::codegen_ir::fibers;
use crate::codegen_ir::literal_defaults::{
    emit_array_literal_default_to_result, literal_default_value, LiteralDefaultValue,
};
use crate::codegen_ir::{CodegenIrError, Result};

mod reflection;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;
const RUNTIME_NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;

/// Resolved declared-property storage metadata for a known object receiver.
struct PropertySlot {
    class_name: String,
    property: String,
    php_type: PhpType,
    offset: usize,
    is_declared: bool,
}

/// Resolved object property default metadata for fixed-offset initialization.
struct PropertyDefault {
    offset: usize,
    value: LiteralDefaultValue,
}

/// Concrete class that a dynamic factory can instantiate in this EIR module.
struct DynamicNewCandidate {
    class_name: String,
    class_id: u64,
    property_count: usize,
    uninitialized_marker_offsets: Vec<usize>,
    property_defaults: Vec<PropertyDefault>,
    constructor_impl: Option<(String, Vec<PhpType>)>,
}

/// Lowers fixed-class object allocation and optional constructor invocation.
pub(super) fn lower_object_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let class_name = class_name_immediate(ctx, inst)?.to_string();
    if is_fiber_class(&class_name) {
        return lower_fiber_new(ctx, inst);
    }
    if reflection::is_reflection_owner_class(&class_name) {
        return reflection::lower_reflection_owner_new(ctx, inst, &class_name);
    }
    let constructor_key = php_symbol_key("__construct");
    let (
        class_id,
        property_count,
        uninitialized_marker_offsets,
        property_defaults,
        constructor_impl,
    ) = {
        let class_info = ctx
            .module
            .class_infos
            .get(&class_name)
            .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", class_name)))?;
        if class_info.allow_dynamic_properties {
            return Err(CodegenIrError::unsupported(format!(
                "object allocation requiring dynamic properties for {}",
                class_name
            )));
        }
        if class_interfaces_require_missing_method_symbols(ctx, &class_name, class_info) {
            return Err(CodegenIrError::unsupported(format!(
                "object allocation requiring interface method symbols not emitted by EIR for {}",
                class_name
            )));
        }
        let property_defaults = collect_property_defaults(class_info, inst)?;
        let constructor_impl = if let Some(constructor) = class_info.methods.get(&constructor_key) {
            if constructor.params.len() != inst.operands.len() {
                return Err(CodegenIrError::unsupported(format!(
                    "constructor call to {}::__construct with {} args for {} params",
                    class_name,
                    inst.operands.len(),
                    constructor.params.len()
                )));
            }
            let impl_class = class_info
                .method_impl_classes
                .get(&constructor_key)
                .cloned()
                .unwrap_or_else(|| class_name.clone());
            if !class_method_already_emitted(ctx, &impl_class, &constructor_key, false) {
                return Err(CodegenIrError::unsupported(format!(
                    "constructor call to {}::__construct without an emitted EIR method body",
                    impl_class
                )));
            }
            let param_types = constructor
                .params
                .iter()
                .map(|(_, ty)| ty.codegen_repr())
                .collect::<Vec<_>>();
            Some((impl_class, param_types))
        } else if !inst.operands.is_empty() {
            return Err(CodegenIrError::unsupported(format!(
                "constructor arguments for class {} without __construct",
                class_name
            )));
        } else {
            None
        };
        let marker_offsets = uninitialized_property_marker_offsets(class_info);
        (
            class_info.class_id,
            class_info.properties.len(),
            marker_offsets,
            property_defaults,
            constructor_impl,
        )
    };
    emit_object_allocation(
        ctx,
        class_id,
        property_count,
        &uninitialized_marker_offsets,
    )?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("object_new missing result value"))?;
    ctx.store_result_value(result)?;
    emit_property_defaults(ctx, result, &property_defaults)?;
    if let Some((impl_class, param_types)) = constructor_impl {
        emit_constructor_call(
            ctx,
            result,
            &inst.operands,
            &class_name,
            &impl_class,
            &constructor_key,
            &param_types,
        )?;
    }
    Ok(())
}

/// Lowers `new Fiber($callable)` through the runtime-managed Fiber constructor.
fn lower_fiber_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let class_id = ctx
        .module
        .class_infos
        .get("Fiber")
        .map(|class| class.class_id)
        .unwrap_or(0);
    let callable_arg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    if let Some(callable) = inst.operands.first().copied() {
        ctx.load_value_to_reg(callable, callable_arg)?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, callable_arg, 0);
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        class_id as i64,
    );
    let wrapper_arg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    if let Some(wrapper) = fibers::wrapper_for_fiber_new(ctx.module, ctx.function, inst) {
        abi::emit_symbol_address(ctx.emitter, wrapper_arg, &wrapper.label);
    } else {
        abi::emit_load_int_immediate(ctx.emitter, wrapper_arg, 0);
    }
    abi::emit_call_label(ctx.emitter, "__rt_fiber_construct");
    store_if_result(ctx, inst)
}

/// Lowers constrained runtime class-string object construction.
pub(super) fn lower_dynamic_object_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let (_fallback_class, required_parent) = dynamic_object_new_metadata(ctx, inst)?;
    let class_name_value = expect_operand(inst, 0)?;
    let constructor_args = inst
        .operands
        .get(1..)
        .ok_or_else(|| CodegenIrError::invalid_module("dynamic_object_new missing class operand"))?;
    let candidates = dynamic_new_candidates(ctx, &required_parent, constructor_args.len(), inst)?;
    if candidates.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "dynamic object construction for {} without EIR-lowered candidates",
            required_parent
        )));
    }
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("dynamic_object_new missing result value"))?;
    emit_dynamic_new_class_lookup(ctx, class_name_value, &required_parent)?;
    let invalid_label = ctx.next_label("dynamic_new_invalid");
    let unmatched_label = ctx.next_label("dynamic_new_unmatched");
    let done_label = ctx.next_label("dynamic_new_done");
    emit_branch_if_dynamic_new_lookup_invalid(ctx, &invalid_label);
    emit_push_dynamic_new_class_id(ctx);
    let case_labels = candidates
        .iter()
        .map(|candidate| {
            let label = ctx.next_label("dynamic_new_case");
            emit_compare_dynamic_new_class_id(ctx, candidate.class_id, &label);
            label
        })
        .collect::<Vec<_>>();
    abi::emit_jump(ctx.emitter, &unmatched_label);

    ctx.emitter.label(&unmatched_label);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    emit_dynamic_new_fatal(ctx, &required_parent);

    ctx.emitter.label(&invalid_label);
    emit_dynamic_new_fatal(ctx, &required_parent);

    for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        emit_dynamic_new_candidate(ctx, candidate, constructor_args, result)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Parses the fallback and required-parent class names from the EIR data pool.
fn dynamic_object_new_metadata(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
) -> Result<(String, String)> {
    let data = expect_data(inst)?;
    let value = ctx
        .module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("class data", data.as_raw()))?;
    let Some((fallback_class, required_parent)) = value.split_once('|') else {
        return Err(CodegenIrError::invalid_module(format!(
            "dynamic_object_new metadata '{}' missing fallback|required separator",
            value
        )));
    };
    Ok((
        fallback_class.trim_start_matches('\\').to_string(),
        required_parent.trim_start_matches('\\').to_string(),
    ))
}

/// Collects runtime-instantiable classes for this dynamic factory.
fn dynamic_new_candidates(
    ctx: &FunctionContext<'_>,
    required_parent: &str,
    arg_count: usize,
    inst: &Instruction,
) -> Result<Vec<DynamicNewCandidate>> {
    let mut candidates = Vec::new();
    let mut sorted_classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in sorted_classes {
        if !class_is_same_or_descends_from(ctx, class_name, required_parent) {
            continue;
        }
        if let Some(candidate) =
            dynamic_new_candidate(ctx, class_name, class_info, arg_count, inst)?
        {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

/// Returns true when `class_name` is the requested parent or one of its descendants.
fn class_is_same_or_descends_from(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    required_parent: &str,
) -> bool {
    let mut current = Some(class_name);
    while let Some(name) = current {
        if php_symbol_key(name.trim_start_matches('\\'))
            == php_symbol_key(required_parent.trim_start_matches('\\'))
        {
            return true;
        }
        current = ctx
            .module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Builds one dynamic factory candidate when its constructor path is emitted.
fn dynamic_new_candidate(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    class_info: &ClassInfo,
    arg_count: usize,
    inst: &Instruction,
) -> Result<Option<DynamicNewCandidate>> {
    if class_info.allow_dynamic_properties
        || class_interfaces_require_missing_method_symbols(ctx, class_name, class_info)
    {
        return Ok(None);
    }
    let constructor_key = php_symbol_key("__construct");
    let constructor_impl = if let Some(constructor) = class_info.methods.get(&constructor_key) {
        if constructor.params.len() != arg_count {
            return Ok(None);
        }
        let impl_class = class_info
            .method_impl_classes
            .get(&constructor_key)
            .cloned()
            .unwrap_or_else(|| class_name.to_string());
        if !class_method_already_emitted(ctx, &impl_class, &constructor_key, false) {
            return Ok(None);
        }
        let param_types = constructor
            .params
            .iter()
            .map(|(_, ty)| ty.codegen_repr())
            .collect::<Vec<_>>();
        Some((impl_class, param_types))
    } else if arg_count == 0 {
        None
    } else {
        return Ok(None);
    };
    let property_defaults = collect_property_defaults(class_info, inst)?;
    Ok(Some(DynamicNewCandidate {
        class_name: class_name.to_string(),
        class_id: class_info.class_id,
        property_count: class_info.properties.len(),
        uninitialized_marker_offsets: uninitialized_property_marker_offsets(class_info),
        property_defaults,
        constructor_impl,
    }))
}

/// Emits the class-string lookup input and calls the shared target-name resolver.
fn emit_dynamic_new_class_lookup(
    ctx: &mut FunctionContext<'_>,
    class_name_value: ValueId,
    required_parent: &str,
) -> Result<()> {
    let class_ty = ctx.value_php_type(class_name_value)?.codegen_repr();
    match class_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(class_name_value, ptr_reg, len_reg)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(class_name_value, abi::int_result_reg(ctx.emitter))?;
            emit_dynamic_new_mixed_class_string(ctx, required_parent);
        }
        _ => {
            emit_dynamic_new_fatal(ctx, required_parent);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_instanceof_lookup");
    Ok(())
}

/// Unboxes a mixed class-string or emits the dynamic-factory fatal.
fn emit_dynamic_new_mixed_class_string(
    ctx: &mut FunctionContext<'_>,
    required_parent: &str,
) {
    let string_label = ctx.next_label("dynamic_new_class_string");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // runtime tag 1 means the dynamic factory argument is a string
            ctx.emitter.instruction(&format!("b.eq {}", string_label));         // continue only when the boxed factory argument is a class-string
            emit_dynamic_new_fatal(ctx, required_parent);
            ctx.emitter.label(&string_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // runtime tag 1 means the dynamic factory argument is a string
            ctx.emitter.instruction(&format!("je {}", string_label));           // continue only when the boxed factory argument is a class-string
            emit_dynamic_new_fatal(ctx, required_parent);
            ctx.emitter.label(&string_label);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed string pointer into the lookup input register
        }
    }
}

/// Branches when the dynamic factory lookup failed or named an interface.
fn emit_branch_if_dynamic_new_lookup_invalid(
    ctx: &mut FunctionContext<'_>,
    invalid_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // did the dynamic factory class-string resolve to metadata?
            ctx.emitter.instruction(&format!("b.eq {}", invalid_label));        // abort unresolved factory classes before construction
            ctx.emitter.instruction("cmp x2, #0");                              // target kind 0 means a concrete class, not an interface
            ctx.emitter.instruction(&format!("b.ne {}", invalid_label));        // abort interface targets because factories instantiate objects
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the dynamic factory class-string resolve to metadata?
            ctx.emitter.instruction(&format!("je {}", invalid_label));          // abort unresolved factory classes before construction
            ctx.emitter.instruction("test rdx, rdx");                           // target kind 0 means a concrete class, not an interface
            ctx.emitter.instruction(&format!("jne {}", invalid_label));         // abort interface targets because factories instantiate objects
        }
    }
}

/// Preserves the resolved dynamic factory class id for candidate dispatch.
fn emit_push_dynamic_new_class_id(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg(ctx.emitter, "x1"),
        Arch::X86_64 => abi::emit_push_reg(ctx.emitter, "rdi"),
    }
}

/// Branches to `matched_label` when the saved factory class id matches `class_id`.
fn emit_compare_dynamic_new_class_id(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    matched_label: &str,
) {
    let scratch = abi::temp_int_reg(ctx.emitter.target);
    abi::emit_load_temporary_stack_slot(ctx.emitter, scratch, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #{}", scratch, class_id)); // compare the requested factory class with this candidate class id
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // branch when the runtime class-string selected this constructor
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", scratch, class_id)); // compare the requested factory class with this candidate class id
            ctx.emitter.instruction(&format!("je {}", matched_label));          // branch when the runtime class-string selected this constructor
        }
    }
}

/// Allocates and initializes one selected dynamic factory candidate.
fn emit_dynamic_new_candidate(
    ctx: &mut FunctionContext<'_>,
    candidate: &DynamicNewCandidate,
    constructor_args: &[ValueId],
    result: ValueId,
) -> Result<()> {
    emit_object_allocation(
        ctx,
        candidate.class_id,
        candidate.property_count,
        &candidate.uninitialized_marker_offsets,
    )?;
    ctx.store_result_value(result)?;
    emit_property_defaults(ctx, result, &candidate.property_defaults)?;
    if let Some((impl_class, param_types)) = &candidate.constructor_impl {
        emit_constructor_call(
            ctx,
            result,
            constructor_args,
            &candidate.class_name,
            impl_class,
            &php_symbol_key("__construct"),
            param_types,
        )?;
    }
    Ok(())
}

/// Emits the runtime fatal diagnostic for invalid dynamic factory class names.
fn emit_dynamic_new_fatal(ctx: &mut FunctionContext<'_>, required_parent: &str) {
    let message = format!(
        "Fatal error: Dynamic factory class must extend {}\n",
        required_parent
    );
    emit_fatal_message(ctx, message.as_bytes());
}

/// Writes a fatal diagnostic to stderr and exits.
fn emit_fatal_message(ctx: &mut FunctionContext<'_>, message: &[u8]) {
    let (message_label, message_len) = ctx.data.add_string(message);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the fatal diagnostic
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));      // pass the fatal diagnostic byte length to write()
            ctx.emitter.syscall(4);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter.instruction(&format!("mov edx, {}", message_len));      // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the fatal diagnostic
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall
            ctx.emitter.instruction("syscall");                                 // write the fatal diagnostic bytes
        }
    }
    abi::emit_exit(ctx.emitter, 1);
}

/// Collects literal defaults that can be copied directly into object property slots.
fn collect_property_defaults(
    class_info: &ClassInfo,
    inst: &Instruction,
) -> Result<Vec<PropertyDefault>> {
    let mut defaults = Vec::new();
    for (index, (property, php_type)) in class_info.properties.iter().enumerate() {
        let Some(default_expr) = class_info.defaults.get(index).and_then(Option::as_ref) else {
            continue;
        };
        let offset = class_info
            .property_offsets
            .get(property)
            .copied()
            .unwrap_or(8 + index * 16);
        defaults.push(PropertyDefault {
            offset,
            value: literal_default_value(
                &format!("property ${}", property),
                php_type,
                &default_expr.kind,
                inst.op.name(),
            )?,
        });
    }
    Ok(defaults)
}

/// Writes all supported property defaults into the newly allocated object.
fn emit_property_defaults(
    ctx: &mut FunctionContext<'_>,
    object: crate::ir::ValueId,
    defaults: &[PropertyDefault],
) -> Result<()> {
    for default in defaults {
        let object_reg = abi::secondary_scratch_reg(ctx.emitter);
        ctx.load_value_to_reg(object, object_reg)?;
        emit_property_default(ctx, object_reg, default)?;
    }
    Ok(())
}

/// Writes one literal property default into its object slot.
fn emit_property_default(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    default: &PropertyDefault,
) -> Result<()> {
    match &default.value {
        LiteralDefaultValue::Int(value) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, *value);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Bool(value) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, i64::from(*value));
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Float(value) => {
            let label = ctx.data.add_float(*value);
            let scratch = abi::symbol_scratch_reg(ctx.emitter);
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_symbol_address(ctx.emitter, scratch, &label);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("ldr {}, [{}]", float_reg, scratch)); // load the property default float literal through the symbol scratch register
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!("movsd {}, QWORD PTR [{}]", float_reg, scratch)); // load the property default float literal through the symbol scratch register
                }
            }
            abi::emit_store_to_address(ctx.emitter, float_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Str(value) => {
            let (label, len) = ctx.data.add_string(value.as_bytes());
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
            abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, object_reg, default.offset);
            abi::emit_store_to_address(ctx.emitter, len_reg, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Array {
            elem_type,
            elements,
        } => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_array_literal_default_to_result(ctx, elem_type, elements)?;
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
    }
    Ok(())
}

/// Calls the resolved `__construct` method with the newly allocated object as `$this`.
fn emit_constructor_call(
    ctx: &mut FunctionContext<'_>,
    object: crate::ir::ValueId,
    constructor_args: &[crate::ir::ValueId],
    class_name: &str,
    impl_class: &str,
    constructor_key: &str,
    constructor_param_types: &[PhpType],
) -> Result<()> {
    let mut args = Vec::with_capacity(constructor_args.len() + 1);
    args.push(object);
    args.extend(constructor_args.iter().copied());
    let mut param_types = Vec::with_capacity(constructor_param_types.len() + 1);
    param_types.push(PhpType::Object(class_name.to_string()));
    param_types.extend_from_slice(constructor_param_types);
    let overflow_bytes = materialize_direct_call_args(ctx, &args, &param_types)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &method_symbol(impl_class, constructor_key));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    Ok(())
}

/// Lowers a declared object property read for statically known object receivers.
pub(super) fn lower_prop_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return lower_mixed_prop_get(ctx, inst, object, &property);
    }
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    emit_property_load(ctx, &slot, base_reg)?;
    store_if_result(ctx, inst)
}

/// Lowers `$mixed->property` through the shared stdClass-aware runtime helper.
fn lower_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_property_get");
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Lowers a nullsafe declared-property read for nullable object receivers.
pub(super) fn lower_nullsafe_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    let Some((class_name, nullable)) = nullable_object_receiver_class(ctx, object)? else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for receiver PHP type {:?}",
            inst.op.name(),
            raw_value_php_type(ctx, object)?
        )));
    };
    if !nullable {
        return lower_prop_get(ctx, inst);
    }
    let slot = resolve_property_slot_for_class(ctx, &class_name, &property, inst)?;
    let null_label = ctx.next_label("nullsafe_prop_null");
    let done_label = ctx.next_label("nullsafe_prop_done");
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    emit_nullable_receiver_object_payload(ctx, object, &null_label, base_reg)?;
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    emit_property_load(ctx, &slot, base_reg)?;
    emit_box_current_value_as_mixed(ctx.emitter, &slot.php_type.codegen_repr());
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    emit_boxed_null(ctx);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers a dynamic property read against declared slots on statically known objects.
pub(super) fn lower_dynamic_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property_value = expect_operand(inst, 1)?;
    if let Some(property) = const_string_operand(ctx, property_value)? {
        return lower_const_dynamic_prop_get(ctx, object, property, inst);
    }
    lower_runtime_dynamic_declared_prop_get(ctx, object, property_value, inst)
}

/// Lowers a dynamic property read when the property expression is a literal string.
fn lower_const_dynamic_prop_get(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<()> {
    let slot = resolve_property_slot(ctx, object, property, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    emit_property_load(ctx, &slot, base_reg)?;
    if inst.result_php_type.codegen_repr() == PhpType::Mixed
        && slot.php_type.codegen_repr() != PhpType::Mixed
    {
        emit_box_current_value_as_mixed(ctx.emitter, &slot.php_type.codegen_repr());
    }
    store_if_result(ctx, inst)
}

/// Lowers a runtime string dynamic property read by dispatching across declared slots.
fn lower_runtime_dynamic_declared_prop_get(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    let class_name = dynamic_property_object_class(ctx, object, inst)?;
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    ensure_dynamic_property_miss_supported(inst)?;
    let slots = declared_dynamic_property_slots(ctx, &class_name, inst)?;
    ensure_dynamic_property_slot_results_supported(&slots, inst)?;
    let match_labels = slots
        .iter()
        .map(|slot| ctx.next_label(&format!("dyn_prop_{}", label_fragment(&slot.property))))
        .collect::<Vec<_>>();
    let miss_label = ctx.next_label("dyn_prop_miss");
    let done_label = ctx.next_label("dyn_prop_done");

    let object_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, object_reg)?;
    abi::emit_push_reg(ctx.emitter, object_reg);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    for (slot, label) in slots.iter().zip(match_labels.iter()) {
        emit_branch_if_dynamic_name_matches(ctx, &slot.property, label);
    }
    abi::emit_jump(ctx.emitter, &miss_label);

    for (slot, label) in slots.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, 16);
        if slot.is_declared {
            emit_uninitialized_typed_property_guard(ctx, slot, base_reg);
        }
        emit_property_load(ctx, slot, base_reg)?;
        if dynamic_property_result_needs_box(inst, &slot.php_type) {
            emit_box_current_value_as_mixed(ctx.emitter, &slot.php_type.codegen_repr());
        }
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    emit_dynamic_property_miss_result(ctx, inst);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Returns the normalized class name for object receivers supported by dynamic property dispatch.
fn dynamic_property_object_class(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    inst: &Instruction,
) -> Result<String> {
    let object_ty = ctx.value_php_type(object)?;
    let PhpType::Object(class_name) = object_ty else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for runtime dynamic receiver PHP type {:?}",
            inst.op.name(),
            object_ty
        )));
    };
    Ok(class_name.trim_start_matches('\\').to_string())
}

/// Verifies that the dynamic property name is already materialized as a string.
fn ensure_runtime_dynamic_property_name(
    ctx: &FunctionContext<'_>,
    property_value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    let property_ty = ctx.value_php_type(property_value)?;
    if property_ty == PhpType::Str {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} with runtime property name PHP type {:?}",
        inst.op.name(),
        property_ty
    )))
}

/// Resolves all declared property slots that a runtime dynamic property read may match.
fn declared_dynamic_property_slots(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    inst: &Instruction,
) -> Result<Vec<PropertySlot>> {
    let normalized = class_name.trim_start_matches('\\');
    let property_names = {
        let class_info = ctx
            .module
            .class_infos
            .get(normalized)
            .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", normalized)))?;
        class_info
            .properties
            .iter()
            .map(|(property, _)| property.clone())
            .collect::<Vec<_>>()
    };
    property_names
        .iter()
        .map(|property| resolve_property_slot_for_class(ctx, normalized, property, inst))
        .collect()
}

/// Verifies that the EIR result type can receive every declared property candidate.
fn ensure_dynamic_property_slot_results_supported(
    slots: &[PropertySlot],
    inst: &Instruction,
) -> Result<()> {
    let result_ty = inst.result_php_type.codegen_repr();
    if result_ty == PhpType::Mixed {
        return Ok(());
    }
    for slot in slots {
        if slot.php_type.codegen_repr() != result_ty {
            return Err(CodegenIrError::unsupported(format!(
                "{} with declared property {}::${} PHP type {:?} and result PHP type {:?}",
                inst.op.name(),
                slot.class_name,
                slot.property,
                slot.php_type,
                result_ty
            )));
        }
    }
    Ok(())
}

/// Verifies that a runtime miss can be materialized in the EIR result register shape.
fn ensure_dynamic_property_miss_supported(inst: &Instruction) -> Result<()> {
    match inst.result_php_type.codegen_repr() {
        PhpType::Mixed | PhpType::Bool | PhpType::Int => Ok(()),
        ty => Err(CodegenIrError::unsupported(format!(
            "{} runtime miss for result PHP type {:?}",
            inst.op.name(),
            ty
        ))),
    }
}

/// Returns true when a loaded property value must be boxed for a `Mixed` EIR result.
fn dynamic_property_result_needs_box(inst: &Instruction, source_ty: &PhpType) -> bool {
    inst.result_php_type.codegen_repr() == PhpType::Mixed
        && source_ty.codegen_repr() != PhpType::Mixed
}

/// Emits a PHP null value for a dynamic property lookup that matched no declared slot.
fn emit_dynamic_property_miss_result(ctx: &mut FunctionContext<'_>, inst: &Instruction) {
    if inst.result_php_type.codegen_repr() == PhpType::Mixed {
        emit_boxed_null(ctx);
        return;
    }
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        RUNTIME_NULL_SENTINEL,
    );
}

/// Emits a runtime string comparison branch against one declared property name.
fn emit_branch_if_dynamic_name_matches(
    ctx: &mut FunctionContext<'_>,
    property: &str,
    target_label: &str,
) {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            abi::emit_symbol_address(ctx.emitter, "x3", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", len as i64);
            ctx.emitter.instruction("bl __rt_str_eq");                          // compare the runtime property name against this declared property
            ctx.emitter.instruction(&format!("cbnz x0, {}", target_label));     // dispatch to the declared property slot when the names match
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
            abi::emit_symbol_address(ctx.emitter, "rdx", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", len as i64);
            ctx.emitter.instruction("call __rt_str_eq");                        // compare the runtime property name against this declared property
            ctx.emitter.instruction("test rax, rax");                           // check whether the runtime string comparison matched
            ctx.emitter.instruction(&format!("jne {}", target_label));          // dispatch to the declared property slot when the names match
        }
    }
}

/// Converts arbitrary names into assembly-label-safe fragments.
fn label_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

/// Lowers a declared object property write for statically known object receivers.
pub(super) fn lower_prop_set(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    emit_property_store(ctx, value, &slot, base_reg)
}

/// Lowers named `instanceof` using runtime class/interface metadata.
pub(super) fn lower_instanceof(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let value_ty = ctx.value_php_type(value)?;
    if !matches!(value_ty, PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)) {
        emit_false(ctx);
        return store_if_result(ctx, inst);
    }
    let class_name = class_name_immediate(ctx, inst)?;
    let Some((target_id, target_kind)) = classify_named_target(ctx, class_name) else {
        emit_false(ctx);
        return store_if_result(ctx, inst);
    };
    match value_ty {
        PhpType::Object(_) => {
            ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
            emit_match_call(ctx, target_id, target_kind, "__rt_exception_matches");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(value, abi::int_arg_reg_name(ctx.emitter.target, 0))?;
            emit_match_call(ctx, target_id, target_kind, "__rt_mixed_instanceof");
        }
        _ => emit_false(ctx),
    }
    store_if_result(ctx, inst)
}

/// Lowers dynamic `instanceof` where the target is resolved from a runtime string or object.
pub(super) fn lower_instanceof_dynamic(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let target = expect_operand(inst, 1)?;
    let value_ty = ctx.value_php_type(value)?;
    let target_ty = ctx.value_php_type(target)?;
    let target_false = ctx.next_label("instanceof_dynamic_target_false");
    let done = ctx.next_label("instanceof_dynamic_done");
    emit_normalized_dynamic_instanceof_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_dynamic_target_metadata(ctx, target, &target_ty, &target_false)?;
    emit_dynamic_match_call(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&target_false);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_false(ctx);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Emits allocation, class-id stamping, and declared-property slot initialization.
fn emit_object_allocation(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    property_count: usize,
    uninitialized_marker_offsets: &[usize],
) -> Result<()> {
    let payload_size = 8 + property_count * 16;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x0, #{}", payload_size));     // request object payload storage for the class id and property slots
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #4");                              // heap kind 4 marks object instances for ownership helpers
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the heap header before the object payload
            ctx.emitter.instruction(&format!("mov x10, #{}", class_id));        // materialize the compile-time class id
            ctx.emitter.instruction("str x10, [x0]");                           // store the class id at object payload offset zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rax, {}", payload_size));     // request object payload storage for the class id and property slots
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the x86_64 object heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the heap header before the object payload
            ctx.emitter.instruction(&format!("mov r10, {}", class_id));         // materialize the compile-time class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the class id at object payload offset zero
        }
    }
    let object_reg = abi::int_result_reg(ctx.emitter);
    for index in 0..property_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset);
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset + 8);
    }
    if !uninitialized_marker_offsets.is_empty() {
        let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
        abi::emit_load_int_immediate(ctx.emitter, marker_reg, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
        for offset in uninitialized_marker_offsets {
            abi::emit_store_to_address(ctx.emitter, marker_reg, object_reg, *offset);
        }
    }
    Ok(())
}

/// Returns true when implemented interfaces require method-wrapper metadata not emitted here.
fn class_interfaces_require_missing_method_symbols(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    class_info: &ClassInfo,
) -> bool {
    let emitted_methods = emitted_instance_method_keys(ctx);
    let mut seen = HashSet::new();
    let mut stack = class_info.interfaces.iter().map(String::as_str).collect::<Vec<_>>();
    while let Some(interface_name) = stack.pop() {
        if !seen.insert(interface_name.to_string()) {
            continue;
        }
        let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
            return true;
        };
        if interface_requires_missing_method_symbol(
            ctx,
            class_name,
            class_info,
            interface_info,
            &emitted_methods,
        ) {
            return true;
        }
        stack.extend(interface_info.parents.iter().map(String::as_str));
    }
    false
}

/// Returns true when one interface table entry would point at an unavailable symbol.
fn interface_requires_missing_method_symbol(
    ctx: &FunctionContext<'_>,
    fallback_class: &str,
    class_info: &ClassInfo,
    interface_info: &InterfaceInfo,
    emitted_methods: &HashSet<(String, String)>,
) -> bool {
    for method_name in &interface_info.method_order {
        let impl_class = class_info
            .method_impl_classes
            .get(method_name)
            .map(String::as_str)
            .unwrap_or(fallback_class);
        if interface_method_needs_return_wrapper(ctx, interface_info, method_name, impl_class) {
            return true;
        }
        if !emitted_methods.contains(&(impl_class.to_string(), method_name.clone())) {
            return true;
        }
    }
    false
}

/// Returns true when an interface entry would need a return boxing wrapper.
fn interface_method_needs_return_wrapper(
    ctx: &FunctionContext<'_>,
    interface_info: &InterfaceInfo,
    method_name: &str,
    impl_class: &str,
) -> bool {
    let Some(interface_sig) = interface_info.methods.get(method_name) else {
        return false;
    };
    let Some(actual_sig) = ctx
        .module
        .class_infos
        .get(impl_class)
        .and_then(|class_info| class_info.methods.get(method_name))
    else {
        return true;
    };
    matches!(interface_sig.return_type.codegen_repr(), PhpType::Mixed)
        && !matches!(actual_sig.return_type.codegen_repr(), PhpType::Mixed)
}

/// Returns instance-method keys emitted by the EIR backend.
fn emitted_instance_method_keys(ctx: &FunctionContext<'_>) -> HashSet<(String, String)> {
    ctx.module
        .class_methods
        .iter()
        .filter(|function| !function.flags.is_static)
        .filter_map(|function| {
            let (class_name, method_name) = function.name.rsplit_once("::")?;
            Some((class_name.to_string(), php_symbol_key(method_name)))
        })
        .collect()
}

/// Returns true when the current EIR module includes a class method body.
fn class_method_already_emitted(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    ctx.module.class_methods.iter().any(|function| {
        function.flags.is_static == is_static
            && function
                .name
                .rsplit_once("::")
                .is_some_and(|(candidate_class, candidate_method)| {
                    candidate_class == class_name
                        && php_symbol_key(candidate_method) == method_key
                })
    })
}

/// Collects property high-word offsets that should start with the typed-property sentinel.
fn uninitialized_property_marker_offsets(class_info: &ClassInfo) -> Vec<usize> {
    class_info
        .properties
        .iter()
        .enumerate()
        .filter_map(|(index, (property, _))| {
            let starts_uninitialized = class_info.declared_properties.contains(property)
                && class_info.defaults.get(index).is_some_and(|default| default.is_none());
            if starts_uninitialized {
                Some(
                    class_info
                        .property_offsets
                        .get(property)
                        .copied()
                        .unwrap_or(8 + index * 16)
                        + 8,
                )
            } else {
                None
            }
        })
        .collect()
}

/// Resolves the property slot for a concrete object receiver and declared property name.
fn resolve_property_slot(
    ctx: &FunctionContext<'_>,
    object: crate::ir::ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let object_ty = ctx.value_php_type(object)?;
    let PhpType::Object(class_name) = object_ty else {
        if let PhpType::Packed(class_name) = object_ty {
            return resolve_packed_field_slot(ctx, &class_name, property, inst);
        }
        return Err(CodegenIrError::unsupported(format!(
            "{} for receiver PHP type {:?}",
            inst.op.name(),
            object_ty
        )));
    };
    resolve_property_slot_for_class(ctx, &class_name, property, inst)
}

/// Resolves a property slot for a known class name.
fn resolve_property_slot_for_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", normalized)))?;
    if class_info.reference_properties.contains(property) {
        return Err(CodegenIrError::unsupported(format!(
            "{} for reference property {}::${}",
            inst.op.name(),
            normalized,
            property
        )));
    }
    let Some((index, (_, php_type))) = class_info
        .properties
        .iter()
        .enumerate()
        .find(|(_, (name, _))| name == property)
    else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for dynamic or missing property {}::${}",
            inst.op.name(),
            normalized,
            property
        )));
    };
    ensure_property_type_supported(php_type, inst)?;
    let offset = class_info
        .property_offsets
        .get(property)
        .copied()
        .unwrap_or(8 + index * 16);
    Ok(PropertySlot {
        class_name: normalized.to_string(),
        property: property.to_string(),
        php_type: php_type.clone(),
        offset,
        is_declared: class_info.declared_properties.contains(property),
    })
}

/// Returns the source PHP type for an SSA value before codegen representation erasure.
pub(super) fn raw_value_php_type(ctx: &FunctionContext<'_>, value: ValueId) -> Result<PhpType> {
    ctx.function
        .value(value)
        .map(|metadata| metadata.php_type.clone())
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))
}

/// Returns the literal string payload for a value produced by `ConstStr`, when statically known.
fn const_string_operand<'a>(ctx: &FunctionContext<'a>, value: ValueId) -> Result<Option<&'a str>> {
    let metadata = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = metadata.def else {
        return Ok(None);
    };
    let instruction = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if instruction.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = instruction.immediate else {
        return Err(CodegenIrError::invalid_module("const_str missing data immediate"));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .map(Some)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Resolves an object or object|null source type for a nullsafe receiver.
pub(super) fn nullable_object_receiver_class(
    ctx: &FunctionContext<'_>,
    object: ValueId,
) -> Result<Option<(String, bool)>> {
    match raw_value_php_type(ctx, object)? {
        PhpType::Object(class_name) => Ok(Some((class_name, false))),
        PhpType::Union(members) => {
            let mut class_name = None;
            let mut nullable = false;
            for member in members {
                match member {
                    PhpType::Void => nullable = true,
                    PhpType::Object(candidate) => {
                        if class_name
                            .as_ref()
                            .is_some_and(|existing: &String| existing != &candidate)
                        {
                            return Ok(None);
                        }
                        class_name = Some(candidate);
                    }
                    _ => return Ok(None),
                }
            }
            Ok(class_name.map(|name| (name, nullable)))
        }
        _ => Ok(None),
    }
}

/// Unboxes a nullable object receiver and branches when it holds PHP null.
pub(super) fn emit_nullable_receiver_object_payload(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    null_label: &str,
    object_reg: &str,
) -> Result<()> {
    let ty = ctx.load_value_to_result(object)?;
    if ty != PhpType::Mixed {
        return Err(CodegenIrError::unsupported(format!(
            "nullsafe property receiver storage {:?}",
            ty
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #8");                              // check whether the nullable receiver holds PHP null
            ctx.emitter.instruction(&format!("b.eq {}", null_label));           // short-circuit property access for nullsafe null receivers
            ctx.emitter.instruction(&format!("mov {}, x1", object_reg));        // promote the unboxed object payload into the property base register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 8");                              // check whether the nullable receiver holds PHP null
            ctx.emitter.instruction(&format!("je {}", null_label));             // short-circuit property access for nullsafe null receivers
            ctx.emitter.instruction(&format!("mov {}, rdi", object_reg));       // promote the unboxed object payload into the property base register
        }
    }
    Ok(())
}

/// Boxes a PHP null sentinel as a runtime Mixed cell.
pub(super) fn emit_boxed_null(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), RUNTIME_NULL_SENTINEL);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}

/// Resolves a field slot on an embedded packed-class receiver.
fn resolve_packed_field_slot(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
    inst: &Instruction,
) -> Result<PropertySlot> {
    let normalized = class_name.trim_start_matches('\\');
    let class_info = ctx
        .module
        .packed_class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown packed class {}", normalized)))?;
    let Some(field) = class_info.fields.iter().find(|field| field.name == property) else {
        return Err(CodegenIrError::unsupported(format!(
            "{} for missing packed field {}::${}",
            inst.op.name(),
            normalized,
            property
        )));
    };
    ensure_property_type_supported(&field.php_type, inst)?;
    Ok(PropertySlot {
        class_name: normalized.to_string(),
        property: property.to_string(),
        php_type: field.php_type.clone(),
        offset: field.offset,
        is_declared: false,
    })
}

/// Verifies that this slice knows how to represent the property type in an object slot.
fn ensure_property_type_supported(php_type: &PhpType, inst: &Instruction) -> Result<()> {
    match php_type {
        PhpType::Bool | PhpType::Int | PhpType::Float | PhpType::Str => Ok(()),
        ty if is_pointer_sized_property_type(ty) => Ok(()),
        _ => Err(CodegenIrError::unsupported(format!(
            "{} for property PHP type {:?}",
            inst.op.name(),
            php_type
        ))),
    }
}

/// Verifies the assigned value already has the property storage representation.
fn ensure_property_value_supported(
    ctx: &FunctionContext<'_>,
    slot: &PropertySlot,
    value: ValueId,
    value_ty: &PhpType,
    inst: &Instruction,
) -> Result<()> {
    if value_ty == &slot.php_type {
        return Ok(());
    }
    if is_pointer_sized_property_type(&slot.php_type)
        && is_pointer_slot_null_sentinel(ctx, value, value_ty)?
    {
        return Ok(());
    }
    if is_empty_array_for_array_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} assigning PHP type {:?} to {}::${} with PHP type {:?}",
        inst.op.name(),
        value_ty,
        slot.class_name,
        slot.property,
        slot.php_type
    )))
}

/// Returns true when an empty array literal initializes a typed array property.
fn is_empty_array_for_array_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    matches!(
        (value_ty, slot_ty),
        (PhpType::Array(elem_ty), PhpType::Array(_))
            if matches!(elem_ty.as_ref(), PhpType::Never | PhpType::Void)
    )
}

/// Returns true when a value can initialize a pointer-sized slot as null.
fn is_pointer_slot_null_sentinel(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<bool> {
    if matches!(value_ty, PhpType::Void) {
        return Ok(true);
    }
    if !matches!(value_ty, PhpType::Int) {
        return Ok(false);
    }
    let metadata = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = metadata.def else {
        return Ok(false);
    };
    let instruction = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    Ok(instruction.op == Op::ConstI64 && instruction.immediate == Some(Immediate::I64(0)))
}

/// Emits the declared-property load into the canonical result register(s).
fn emit_property_load(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    match &slot.php_type {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, ptr_reg, base_reg, slot.offset);
            abi::emit_load_from_address(ctx.emitter, len_reg, base_reg, slot.offset + 8);
        }
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, float_reg, base_reg, slot.offset);
        }
        PhpType::Bool | PhpType::Int => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset);
        }
        ty if is_pointer_sized_property_type(ty) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset);
        }
        _ => return Err(CodegenIrError::unsupported(format!(
            "property load for PHP type {:?}",
            slot.php_type
        ))),
    }
    Ok(())
}

/// Emits a declared-property store from an SSA value into the object slot.
fn emit_property_store(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    match &slot.php_type {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, base_reg, slot.offset);
            abi::emit_store_to_address(ctx.emitter, len_reg, base_reg, slot.offset + 8);
        }
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            ctx.load_value_to_reg(value, float_reg)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, float_reg, base_reg, slot.offset);
            abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        }
        PhpType::Bool | PhpType::Int => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            ctx.load_value_to_reg(value, int_reg)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, int_reg, base_reg, slot.offset);
            abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        }
        ty if is_pointer_sized_property_type(ty) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            ctx.load_value_to_reg(value, int_reg)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, int_reg, base_reg, slot.offset);
            abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        }
        _ => return Err(CodegenIrError::unsupported(format!(
            "property store for PHP type {:?}",
            slot.php_type
        ))),
    }
    Ok(())
}

/// Returns true for property values represented as a single pointer-sized word.
fn is_pointer_sized_property_type(php_type: &PhpType) -> bool {
    matches!(
        php_type.codegen_repr(),
        PhpType::Iterable
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Buffer(_)
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_)
            | PhpType::Resource(_)
    )
}

/// Emits a fatal guard for reads from uninitialized typed properties.
fn emit_uninitialized_typed_property_guard(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    object_reg: &str,
) {
    let initialized_label = ctx.next_label("typed_prop_initialized");
    let marker_reg = abi::secondary_scratch_reg(ctx.emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(ctx.emitter);
    abi::emit_load_from_address(ctx.emitter, marker_reg, object_reg, slot.offset + 8);
    abi::emit_load_int_immediate(ctx.emitter, sentinel_reg, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the property marker against the uninitialized sentinel
            ctx.emitter.instruction(&format!("b.ne {}", initialized_label));    // continue the property read once the slot has been initialized
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // compare the property marker against the uninitialized sentinel
            ctx.emitter.instruction(&format!("jne {}", initialized_label));     // continue the property read once the slot has been initialized
        }
    }
    emit_uninitialized_typed_property_fatal(ctx, slot);
    ctx.emitter.label(&initialized_label);
}

/// Emits the runtime fatal diagnostic for an uninitialized typed-property read.
fn emit_uninitialized_typed_property_fatal(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
) {
    let message = format!(
        "Fatal error: Typed property {}::${} must not be accessed before initialization\n",
        slot.class_name, slot.property
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the uninitialized typed-property fatal
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));      // pass the fatal diagnostic byte length to write()
            ctx.emitter.syscall(4);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter.instruction(&format!("mov edx, {}", message_len));      // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the uninitialized typed-property fatal
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall
            ctx.emitter.instruction("syscall");                                 // write the uninitialized typed-property fatal diagnostic
        }
    }
    abi::emit_exit(ctx.emitter, 1);
}

/// Normalizes the tested value into an object pointer or null for dynamic `instanceof`.
fn emit_normalized_dynamic_instanceof_value(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    match value_ty {
        PhpType::Object(_) => {
            ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(value, abi::int_result_reg(ctx.emitter))?;
            emit_mixed_instanceof_value_normalization(ctx);
        }
        _ => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
    }
    Ok(())
}

/// Unboxes a Mixed/Union tested value and leaves only object payloads as matchable.
fn emit_mixed_instanceof_value_normalization(ctx: &mut FunctionContext<'_>) {
    let object_label = ctx.next_label("instanceof_dynamic_value_object");
    let done = ctx.next_label("instanceof_dynamic_value_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the tested mixed payload is an object
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // object payloads can be matched after dynamic target resolution
            ctx.emitter.instruction("mov x0, #0");                              // scalar mixed payloads become null so the matcher returns false
            ctx.emitter.instruction(&format!("b {}", done));                    // skip object-payload promotion for scalar payloads
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov x0, x1");                              // promote the unboxed object pointer into the normal result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the tested mixed payload is an object
            ctx.emitter.instruction(&format!("je {}", object_label));           // object payloads can be matched after dynamic target resolution
            ctx.emitter.instruction("xor eax, eax");                            // scalar mixed payloads become null so the matcher returns false
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip object-payload promotion for scalar payloads
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov rax, rdi");                            // promote the unboxed object pointer into the normal result register
        }
    }
    ctx.emitter.label(&done);
}

/// Resolves the dynamic `instanceof` target into matcher id/kind registers.
fn emit_dynamic_target_metadata(
    ctx: &mut FunctionContext<'_>,
    target: crate::ir::ValueId,
    target_ty: &PhpType,
    false_label: &str,
) -> Result<()> {
    match target_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(target, ptr_reg, len_reg)?;
            emit_lookup_string_target(ctx, false_label);
        }
        PhpType::Object(_) => {
            ctx.load_value_to_reg(target, abi::int_result_reg(ctx.emitter))?;
            emit_object_target_metadata(ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_reg(target, abi::int_result_reg(ctx.emitter))?;
            emit_mixed_target_metadata(ctx, false_label);
        }
        _ => emit_invalid_dynamic_target_fatal(ctx),
    }
    Ok(())
}

/// Looks up a string dynamic target in the runtime class/interface name table.
fn emit_lookup_string_target(ctx: &mut FunctionContext<'_>, false_label: &str) {
    abi::emit_call_label(ctx.emitter, "__rt_instanceof_lookup");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // did the dynamic string resolve to a known class or interface?
            ctx.emitter.instruction(&format!("b.eq {}", false_label));          // unresolved class-string targets make instanceof false
            ctx.emitter.instruction("mov x0, x1");                              // move the resolved target id into the matcher target-id register
            ctx.emitter.instruction("mov x1, x2");                              // move the resolved target kind into the matcher target-kind register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the dynamic string resolve to a known class or interface?
            ctx.emitter.instruction(&format!("je {}", false_label));            // unresolved class-string targets make instanceof false
            ctx.emitter.instruction("mov rax, rdi");                            // move the resolved target id into the matcher target-id register
        }
    }
}

/// Extracts matcher metadata from an object-typed dynamic target.
fn emit_object_target_metadata(ctx: &mut FunctionContext<'_>) {
    let ok_label = ctx.next_label("instanceof_dynamic_object_target_ok");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", ok_label));         // non-null object targets can provide runtime class metadata
            emit_invalid_dynamic_target_fatal(ctx);
            ctx.emitter.label(&ok_label);
            ctx.emitter.instruction("ldr x0, [x0]");                            // load the runtime class id from the target object header
            ctx.emitter.instruction("mov x1, #0");                              // object targets always resolve to class target kind
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // null object targets are not valid dynamic instanceof targets
            ctx.emitter.instruction(&format!("jne {}", ok_label));              // non-null object targets can provide runtime class metadata
            emit_invalid_dynamic_target_fatal(ctx);
            ctx.emitter.label(&ok_label);
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // load the runtime class id from the target object header
            ctx.emitter.instruction("xor edx, edx");                            // object targets always resolve to class target kind
        }
    }
}

/// Unboxes a Mixed/Union target and routes strings or objects to the matching target resolver.
fn emit_mixed_target_metadata(ctx: &mut FunctionContext<'_>, false_label: &str) {
    let string_label = ctx.next_label("instanceof_dynamic_target_string");
    let object_label = ctx.next_label("instanceof_dynamic_target_object");
    let done = ctx.next_label("instanceof_dynamic_target_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // runtime tag 1 means the dynamic target is a string
            ctx.emitter.instruction(&format!("b.eq {}", string_label));         // resolve boxed string targets through class-string lookup
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the dynamic target is an object
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // resolve boxed object targets through their runtime class id
            emit_invalid_dynamic_target_fatal(ctx);
            ctx.emitter.label(&string_label);
            emit_lookup_string_target(ctx, false_label);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov x0, x1");                              // move the unboxed target object pointer into the result register
            emit_object_target_metadata(ctx);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // runtime tag 1 means the dynamic target is a string
            ctx.emitter.instruction(&format!("je {}", string_label));           // resolve boxed string targets through class-string lookup
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the dynamic target is an object
            ctx.emitter.instruction(&format!("je {}", object_label));           // resolve boxed object targets through their runtime class id
            emit_invalid_dynamic_target_fatal(ctx);
            ctx.emitter.label(&string_label);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed target string pointer into the lookup input register
            emit_lookup_string_target(ctx, false_label);
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed target object pointer into the result register
            emit_object_target_metadata(ctx);
        }
    }
    ctx.emitter.label(&done);
}

/// Emits a dynamic `instanceof` matcher call after target id/kind have been resolved.
fn emit_dynamic_match_call(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_push_reg(ctx.emitter, "x1");
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_push_reg(ctx.emitter, "rdx");
        }
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2));
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1));
    abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0));
    abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
}

/// Emits the runtime fatal for invalid dynamic `instanceof` targets.
fn emit_invalid_dynamic_target_fatal(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_instanceof_invalid_target");
}

/// Emits the metadata matcher call with object-or-mixed input already in argument 0.
fn emit_match_call(
    ctx: &mut FunctionContext<'_>,
    target_id: u64,
    target_kind: i64,
    helper: &str,
) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 1),
        target_id as i64,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 2),
        target_kind,
    );
    abi::emit_call_label(ctx.emitter, helper);
}

/// Classifies a named target as a class `(kind 0)` or interface `(kind 1)`.
fn classify_named_target(
    ctx: &FunctionContext<'_>,
    class_name: &str,
) -> Option<(u64, i64)> {
    let normalized = class_name.trim_start_matches('\\');
    if let Some(class_info) = ctx.module.class_infos.get(normalized) {
        return Some((class_info.class_id, 0));
    }
    ctx.module
        .interface_infos
        .get(normalized)
        .map(|interface_info| (interface_info.interface_id, 1))
}

/// Emits a boolean false result for non-object values or unresolved targets.
fn emit_false(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
}

/// Resolves an instruction property-name immediate into the module data pool.
fn property_name_immediate<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Result<&'a str> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Resolves an instruction class-name immediate into the module data pool.
fn class_name_immediate<'a>(
    ctx: &'a FunctionContext<'_>,
    inst: &Instruction,
) -> Result<&'a str> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .map(String::as_str)
        .ok_or_else(|| CodegenIrError::missing_entry("class data", data.as_raw()))
}

/// Returns true when a class name refers to PHP's built-in `Fiber` type.
fn is_fiber_class(class_name: &str) -> bool {
    php_symbol_key(class_name.trim_start_matches('\\')) == "fiber"
}
