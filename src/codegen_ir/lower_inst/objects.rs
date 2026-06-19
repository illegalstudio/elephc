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
//!   per declared property slot plus an optional dynamic-property hash pointer.
//! - Reference properties store a pointer to a local or heap ref-cell in the
//!   property slot, while normal declared properties store values directly.
//! - This slice intentionally rejects interface method entries that need missing
//!   EIR symbols and non-literal default property expressions until their runtime
//!   paths land.

use std::collections::HashSet;

use crate::codegen::{abi, callable_descriptor, emit_box_current_value_as_mixed, runtime_value_tag};
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::intrinsics::IntrinsicCall;
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::names::{method_symbol, php_symbol_key};
use crate::types::{ClassInfo, InterfaceInfo, PhpType};

use super::super::context::FunctionContext;
use super::{
    callables, cast_loaded_mixed_pointer_to_result, direct_call_stack_pad_bytes, expect_data,
    emit_loaded_indexed_array_to_mixed, emit_ref_arg_writebacks, expect_operand, iterators,
    load_value_to_first_int_arg, materialize_direct_call_args_with_refs,
    materialize_method_call_args_with_receiver_reg_and_refs, resolve_method_call_target,
    store_call_result, store_if_result,
};
use crate::codegen_ir::fibers;
use crate::codegen_ir::literal_defaults::{
    emit_array_literal_default_to_result, emit_assoc_array_literal_default_to_result,
    emit_boxed_bool_literal_to_result, emit_boxed_float_literal_to_result,
    emit_boxed_int_literal_to_result, emit_boxed_null_literal_to_result,
    emit_boxed_string_literal_default_to_result, emit_empty_assoc_array_literal_to_result,
    emit_string_literal_default_to_result, emit_tagged_null_literal_to_result,
    literal_default_value, LiteralDefaultValue,
};
use crate::codegen_ir::{CodegenIrError, Result};

mod reflection;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;
const RUNTIME_NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;
const ITERATOR_ITERATOR_DOWNCAST_MESSAGE: &str =
    "Class to downcast to not found or not base class or does not implement Traversable";

/// Resolved declared-property storage metadata for a known object receiver.
struct PropertySlot {
    class_name: String,
    property: String,
    php_type: PhpType,
    offset: usize,
    is_declared: bool,
    is_packed: bool,
    is_reference: bool,
}

/// Declared-property candidate reachable from a `Mixed` object receiver.
struct MixedPropertyCandidate {
    class_id: u64,
    slot: PropertySlot,
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
    allow_dynamic_properties: bool,
    uninitialized_marker_offsets: Vec<usize>,
    property_defaults: Vec<PropertyDefault>,
    constructor_impl: Option<ConstructorCallTarget>,
}

/// Constructor metadata needed after object allocation has produced `$this`.
struct ConstructorCallTarget {
    impl_class: String,
    param_types: Vec<PhpType>,
    ref_params: Vec<bool>,
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
    if class_name == "CallbackFilterIterator" || class_name == "RecursiveCallbackFilterIterator" {
        return lower_callback_filter_iterator_new(ctx, inst, &class_name);
    }
    if is_builtin_stdclass(&class_name) {
        return lower_stdclass_new(ctx, inst);
    }
    if class_name == "IteratorIterator" {
        return lower_iterator_iterator_new(ctx, inst);
    }
    if is_spl_doubly_linked_list_family(&class_name) {
        return lower_spl_doubly_linked_list_new(ctx, inst, &class_name);
    }
    if class_name == "SplFixedArray" {
        return lower_spl_fixed_array_new(ctx, inst);
    }
    if let Some(class_id) = throwable_payload_class_id(ctx, &class_name) {
        return lower_builtin_throwable_new(ctx, inst, &class_name, class_id);
    }
    let constructor_key = php_symbol_key("__construct");
    let (
        class_id,
        property_count,
        allow_dynamic_properties,
        uninitialized_marker_offsets,
        property_defaults,
        constructor_impl,
    ) = {
        let class_info = ctx
            .module
            .class_infos
            .get(&class_name)
            .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", class_name)))?;
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
            Some(ConstructorCallTarget {
                impl_class,
                param_types,
                ref_params: constructor.ref_params.clone(),
            })
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
            class_info.allow_dynamic_properties,
            marker_offsets,
            property_defaults,
            constructor_impl,
        )
    };
    emit_object_allocation(
        ctx,
        class_id,
        property_count,
        allow_dynamic_properties,
        &uninitialized_marker_offsets,
    )?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("object_new missing result value"))?;
    ctx.store_result_value(result)?;
    emit_property_defaults(ctx, result, &property_defaults)?;
    if let Some(constructor) = constructor_impl {
        emit_constructor_call(
            ctx,
            result,
            &inst.operands,
            &class_name,
            &constructor.impl_class,
            &constructor_key,
            &constructor.param_types,
            &constructor.ref_params,
        )?;
    }
    Ok(())
}

/// Lowers `new stdClass()` through the runtime helper that seeds its dynamic-property hash.
fn lower_stdclass_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "stdClass constructor with {} EIR operands",
            inst.operands.len()
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_new");
    store_if_result(ctx, inst)
}

/// Returns true when the class uses the runtime-managed SPL doubly-linked-list payload.
fn is_spl_doubly_linked_list_family(class_name: &str) -> bool {
    matches!(class_name, "SplDoublyLinkedList" | "SplStack" | "SplQueue")
}

/// Lowers `new SplDoublyLinkedList`, `new SplStack`, and `new SplQueue`.
fn lower_spl_doubly_linked_list_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    if !inst.operands.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{} constructor with {} EIR operands",
            class_name,
            inst.operands.len()
        )));
    }
    let class_id = ctx
        .module
        .class_infos
        .get(class_name)
        .map(|info| info.class_id)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", class_name)))?;
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        class_id as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_spl_dll_new");
    store_if_result(ctx, inst)
}

/// Lowers `new SplFixedArray($size = 0)` through the runtime-backed payload allocator.
fn lower_spl_fixed_array_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() > 1 {
        return Err(CodegenIrError::unsupported(format!(
            "SplFixedArray constructor with {} EIR operands",
            inst.operands.len()
        )));
    }
    let class_id = ctx
        .module
        .class_infos
        .get("SplFixedArray")
        .map(|info| info.class_id)
        .ok_or_else(|| CodegenIrError::unsupported("unknown class SplFixedArray"))?;
    if let Some(size) = inst.operands.first().copied() {
        ctx.load_value_to_result(size)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            class_id as i64,
        );
        abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1));
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            class_id as i64,
        );
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 0);
    }
    abi::emit_call_label(ctx.emitter, "__rt_spl_fixed_new");
    store_if_result(ctx, inst)
}

/// Lowers `new CallbackFilterIterator($iterator, $callback)` with callable-array capture.
fn lower_callback_filter_iterator_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
) -> Result<()> {
    if inst.operands.len() != 2 {
        return Err(CodegenIrError::unsupported(format!(
            "{} constructor with {} EIR operands",
            class_name,
            inst.operands.len()
        )));
    }
    let source = expect_operand(inst, 0)?;
    let callback = expect_operand(inst, 1)?;
    let (class_id, property_count, uninitialized_marker_offsets, property_defaults, callback_env_offset) = {
        let class_info = ctx
            .module
            .class_infos
            .get(class_name)
            .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", class_name)))?;
        if class_info.allow_dynamic_properties {
            return Err(CodegenIrError::unsupported(format!(
                "object allocation requiring dynamic properties for {}",
                class_name
            )));
        }
        if class_interfaces_require_missing_method_symbols(ctx, class_name, class_info) {
            return Err(CodegenIrError::unsupported(format!(
                "object allocation requiring interface method symbols not emitted by EIR for {}",
                class_name
            )));
        }
        (
            class_info.class_id,
            class_info.properties.len(),
            uninitialized_property_marker_offsets(class_info),
            collect_property_defaults(class_info, inst)?,
            class_info.property_offsets.get("callbackEnv").copied(),
        )
    };
    let inner_slot = resolve_property_slot_for_class(ctx, class_name, "inner", inst)?;
    let callback_slot = resolve_property_slot_for_class(ctx, class_name, "callback", inst)?;
    emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
    )?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("object_new missing result value"))?;
    ctx.store_result_value(result)?;
    emit_property_defaults(ctx, result, &property_defaults)?;
    if let Some(offset) = callback_env_offset {
        emit_zero_pointer_property(ctx, result, offset)?;
    }
    emit_callback_filter_source_property(ctx, source, result, &inner_slot, inst)?;
    emit_callback_filter_callback_property(ctx, callback, result, &callback_slot, inst)
}

/// Stores CallbackFilterIterator::$inner from a constructor source operand.
fn emit_callback_filter_source_property(
    ctx: &mut FunctionContext<'_>,
    source: ValueId,
    target: ValueId,
    slot: &PropertySlot,
    inst: &Instruction,
) -> Result<()> {
    let value_ty = ctx.value_php_type(source)?;
    ensure_property_value_supported(ctx, slot, source, &value_ty, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(target, base_reg)?;
    emit_property_store(ctx, source, slot, base_reg)
}

/// Stores CallbackFilterIterator::$callback, converting callable arrays to descriptors.
fn emit_callback_filter_callback_property(
    ctx: &mut FunctionContext<'_>,
    callback: ValueId,
    target: ValueId,
    slot: &PropertySlot,
    inst: &Instruction,
) -> Result<()> {
    let value_ty = ctx.value_php_type(callback)?;
    match value_ty.codegen_repr() {
        PhpType::Array(elem) if matches!(elem.codegen_repr(), PhpType::Mixed | PhpType::Str) => {
            callables::emit_runtime_callable_array_descriptor_value(
                ctx,
                callback,
                "callback_filter_constructor",
            )?;
            emit_store_result_to_pointer_property(ctx, target, slot.offset)
        }
        _ => {
            ensure_property_value_supported(ctx, slot, callback, &value_ty, inst)?;
            let base_reg = abi::symbol_scratch_reg(ctx.emitter);
            ctx.load_value_to_reg(target, base_reg)?;
            emit_property_store(ctx, callback, slot, base_reg)
        }
    }
}

/// Stores the current single-register result into one pointer-sized object property.
fn emit_store_result_to_pointer_property(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    offset: usize,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    ctx.load_value_to_reg(target, base_reg)?;
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_store_to_address(ctx.emitter, result_reg, base_reg, offset);
    abi::emit_store_zero_to_address(ctx.emitter, base_reg, offset + 8);
    Ok(())
}

/// Initializes one pointer-sized object property to null.
fn emit_zero_pointer_property(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    offset: usize,
) -> Result<()> {
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(target, base_reg)?;
    abi::emit_store_zero_to_address(ctx.emitter, base_reg, offset);
    abi::emit_store_zero_to_address(ctx.emitter, base_reg, offset + 8);
    Ok(())
}

/// Lowers `new IteratorIterator($iterator)` by normalizing a Traversable source to an Iterator.
fn lower_iterator_iterator_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_operand(inst, 0)?;
    let source_ty = ctx.value_php_type(source)?.codegen_repr();
    let PhpType::Object(source_name) = &source_ty else {
        return Err(CodegenIrError::unsupported(format!(
            "IteratorIterator source PHP type {:?}",
            source_ty
        )));
    };
    if !object_type_is_a(ctx, source_name, "Traversable") {
        return Err(CodegenIrError::unsupported(format!(
            "IteratorIterator Traversable normalization for PHP type {:?}",
            source_ty
        )));
    }
    let class_info = ctx
        .module
        .class_infos
        .get("IteratorIterator")
        .ok_or_else(|| CodegenIrError::unsupported("unknown class IteratorIterator"))?;
    if class_info.allow_dynamic_properties {
        return Err(CodegenIrError::unsupported(
            "object allocation requiring dynamic properties for IteratorIterator",
        ));
    }
    if class_interfaces_require_missing_method_symbols(ctx, "IteratorIterator", class_info) {
        return Err(CodegenIrError::unsupported(
            "object allocation requiring interface method symbols not emitted by EIR for IteratorIterator",
        ));
    }
    let inner_offset = class_info
        .property_offsets
        .get("inner")
        .copied()
        .ok_or_else(|| CodegenIrError::missing_entry("property offset", 0))?;
    let inner_ty = class_info
        .properties
        .iter()
        .find(|(name, _)| name == "inner")
        .map(|(_, ty)| ty.clone())
        .ok_or_else(|| CodegenIrError::missing_entry("property inner", 0))?;
    let class_id = class_info.class_id;
    let property_count = class_info.properties.len();
    let uninitialized_marker_offsets = uninitialized_property_marker_offsets(class_info);
    let slot = PropertySlot {
        class_name: "IteratorIterator".to_string(),
        property: "inner".to_string(),
        php_type: inner_ty,
        offset: inner_offset,
        is_declared: true,
        is_packed: false,
        is_reference: false,
    };
    emit_object_allocation(
        ctx,
        class_id,
        property_count,
        false,
        &uninitialized_marker_offsets,
    )?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("object_new missing result value"))?;
    ctx.store_result_value(result)?;
    emit_iterator_iterator_inner_from_traversable(ctx, source, inst.operands.get(1).copied(), result, &slot)
}

/// Stores IteratorIterator::$inner after converting IteratorAggregate inputs through getIterator().
fn emit_iterator_iterator_inner_from_traversable(
    ctx: &mut FunctionContext<'_>,
    source: ValueId,
    downcast: Option<ValueId>,
    target: ValueId,
    slot: &PropertySlot,
) -> Result<()> {
    emit_push_iterator_iterator_downcast_status(ctx, downcast)?;
    ctx.load_value_to_result(source)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let direct_case = ctx.next_label("iterator_iterator_source_iterator");
    let aggregate_case = ctx.next_label("iterator_iterator_source_aggregate");
    let done = ctx.next_label("iterator_iterator_source_done");
    emit_branch_if_saved_traversable_implements(ctx, "Iterator", &direct_case)?;
    emit_branch_if_saved_traversable_implements(ctx, "IteratorAggregate", &aggregate_case)?;
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_call_label(ctx.emitter, "__rt_iterable_unsupported_kind");

    ctx.emitter.label(&direct_case);
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_incref_if_refcounted(ctx.emitter, &slot.php_type.codegen_repr());
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&aggregate_case);
    emit_validate_iterator_iterator_aggregate_downcast(ctx)?;
    abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    move_result_to_receiver_arg(ctx);
    iterators::emit_interface_dispatch_call(ctx, "IteratorAggregate", "getiterator", None)?;

    ctx.emitter.label(&done);
    emit_iterator_inner_property_from_result(ctx, target, slot.offset)
}

/// Pushes `[status, class_id]` metadata for IteratorIterator's optional downcast argument.
fn emit_push_iterator_iterator_downcast_status(
    ctx: &mut FunctionContext<'_>,
    downcast: Option<ValueId>,
) -> Result<()> {
    let Some(value) = downcast else {
        emit_push_iterator_iterator_downcast_status_pair(ctx, 0, 0);
        return Ok(());
    };
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(value, ptr_reg, len_reg)?;
            abi::emit_call_label(ctx.emitter, "__rt_instanceof_lookup");
            emit_push_iterator_iterator_downcast_status_from_lookup(ctx);
        }
        PhpType::Void | PhpType::Never => {
            emit_push_iterator_iterator_downcast_status_pair(ctx, 0, 0);
        }
        _ => {
            emit_push_iterator_iterator_downcast_status_pair(ctx, 2, 0);
        }
    }
    Ok(())
}

/// Pushes downcast status metadata after `__rt_instanceof_lookup` returned target metadata.
fn emit_push_iterator_iterator_downcast_status_from_lookup(ctx: &mut FunctionContext<'_>) {
    let invalid = ctx.next_label("iterator_iterator_downcast_lookup_invalid");
    let done = ctx.next_label("iterator_iterator_downcast_lookup_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // did the downcast class-string resolve to metadata?
            ctx.emitter.instruction(&format!("b.eq {}", invalid));              // invalid downcast names throw for IteratorAggregate inputs
            ctx.emitter.instruction("cmp x2, #0");                              // target kind 0 means a concrete class, not an interface
            ctx.emitter.instruction(&format!("b.ne {}", invalid));              // interface names are invalid downcast targets
            ctx.emitter.instruction("mov x0, #1");                              // status 1 means x1 carries a concrete downcast class id
            ctx.emitter.instruction(&format!("b {}", done));                    // preserve the resolved class id for later validation

            ctx.emitter.label(&invalid);
            ctx.emitter.instruction("mov x0, #2");                              // status 2 means the downcast must throw for aggregates
            ctx.emitter.instruction("mov x1, #0");                              // invalid downcast targets have no usable class id
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // did the downcast class-string resolve to metadata?
            ctx.emitter.instruction(&format!("je {}", invalid));                // invalid downcast names throw for IteratorAggregate inputs
            ctx.emitter.instruction("test rdx, rdx");                           // target kind 0 means a concrete class, not an interface
            ctx.emitter.instruction(&format!("jne {}", invalid));               // interface names are invalid downcast targets
            ctx.emitter.instruction("mov rax, 1");                              // status 1 means rdi carries a concrete downcast class id
            ctx.emitter.instruction(&format!("jmp {}", done));                  // preserve the resolved class id for later validation

            ctx.emitter.label(&invalid);
            ctx.emitter.instruction("mov rax, 2");                              // status 2 means the downcast must throw for aggregates
            ctx.emitter.instruction("xor edi, edi");                            // invalid downcast targets have no usable class id
        }
    }
    ctx.emitter.label(&done);
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg_pair(ctx.emitter, "x0", "x1"),
        Arch::X86_64 => abi::emit_push_reg_pair(ctx.emitter, "rax", "rdi"),
    }
}

/// Pushes a literal IteratorIterator downcast status pair.
fn emit_push_iterator_iterator_downcast_status_pair(
    ctx: &mut FunctionContext<'_>,
    status: i64,
    class_id: i64,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", status);
            abi::emit_load_int_immediate(ctx.emitter, "x1", class_id);
            abi::emit_push_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rax", status);
            abi::emit_load_int_immediate(ctx.emitter, "rdi", class_id);
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdi");
        }
    }
}

/// Validates the optional downcast metadata before wrapping an IteratorAggregate source.
fn emit_validate_iterator_iterator_aggregate_downcast(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let aggregate_interface_id = interface_info_by_name(ctx, "IteratorAggregate")
        .ok_or_else(|| CodegenIrError::unsupported("missing interface IteratorAggregate"))?
        .interface_id as i64;
    let skip = ctx.next_label("iterator_iterator_downcast_skip");
    let throw = ctx.next_label("iterator_iterator_downcast_throw");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [sp, #16]");                       // load downcast status: 0 omitted/null, 1 class id, 2 invalid
            ctx.emitter.instruction(&format!("cbz x9, {}", skip));              // omitted/null class arguments do not constrain aggregates
            ctx.emitter.instruction("cmp x9, #1");                              // only status 1 carries a valid concrete class id
            ctx.emitter.instruction(&format!("b.ne {}", throw));                // invalid names and interfaces throw for aggregates
            ctx.emitter.instruction("ldr x0, [sp]");                            // pass the saved IteratorAggregate object to the class matcher
            ctx.emitter.instruction("ldr x1, [sp, #24]");                       // pass the requested downcast class id to the class matcher
            abi::emit_load_int_immediate(ctx.emitter, "x2", 0);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
            ctx.emitter.instruction("cmp x0, #0");                              // did the aggregate object match the requested class?
            ctx.emitter.instruction(&format!("b.eq {}", throw));                // non-base downcast classes are rejected like PHP
            ctx.emitter.instruction("ldr x0, [sp, #24]");                       // pass the requested class id to the interface checker
            abi::emit_load_int_immediate(ctx.emitter, "x1", aggregate_interface_id);
            abi::emit_call_label(ctx.emitter, "__rt_class_implements_interface");
            ctx.emitter.instruction("cmp x0, #0");                              // did the downcast class implement IteratorAggregate?
            ctx.emitter.instruction(&format!("b.eq {}", throw));                // non-Traversable base classes are rejected like PHP
            ctx.emitter.instruction(&format!("b {}", skip));                    // the aggregate downcast class is valid
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 16]");           // load downcast status: 0 omitted/null, 1 class id, 2 invalid
            ctx.emitter.instruction("test r10, r10");                           // is there an explicit downcast class to validate?
            ctx.emitter.instruction(&format!("je {}", skip));                   // omitted/null class arguments do not constrain aggregates
            ctx.emitter.instruction("cmp r10, 1");                              // only status 1 carries a valid concrete class id
            ctx.emitter.instruction(&format!("jne {}", throw));                 // invalid names and interfaces throw for aggregates
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // pass the saved IteratorAggregate object to the class matcher
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");           // pass the requested downcast class id to the class matcher
            abi::emit_load_int_immediate(ctx.emitter, "rdx", 0);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
            ctx.emitter.instruction("test rax, rax");                           // did the aggregate object match the requested class?
            ctx.emitter.instruction(&format!("je {}", throw));                  // non-base downcast classes are rejected like PHP
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");           // pass the requested class id to the interface checker
            abi::emit_load_int_immediate(ctx.emitter, "rsi", aggregate_interface_id);
            abi::emit_call_label(ctx.emitter, "__rt_class_implements_interface");
            ctx.emitter.instruction("test rax, rax");                           // did the downcast class implement IteratorAggregate?
            ctx.emitter.instruction(&format!("je {}", throw));                  // non-Traversable base classes are rejected like PHP
            ctx.emitter.instruction(&format!("jmp {}", skip));                  // the aggregate downcast class is valid
        }
    }

    ctx.emitter.label(&throw);
    emit_throw_iterator_iterator_downcast_logic_exception(ctx);
    ctx.emitter.label(&skip);
    Ok(())
}

/// Throws the LogicException required for invalid IteratorIterator aggregate downcasts.
fn emit_throw_iterator_iterator_downcast_logic_exception(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #32");                             // request Throwable payload storage
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #6");                              // heap kind 6 marks object instances
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp allocation as a runtime object
            abi::emit_symbol_address(ctx.emitter, "x9", "_spl_logic_exception_class_id");
            ctx.emitter.instruction("ldr x9, [x9]");                            // load LogicException's runtime class id
            ctx.emitter.instruction("str x9, [x0]");                            // store the class id at object header
            abi::emit_symbol_address(ctx.emitter, "x9", "_iterator_iterator_downcast_msg");
            ctx.emitter.instruction("str x9, [x0, #8]");                        // store static exception message pointer
            ctx.emitter.instruction(&format!("mov x9, #{}", ITERATOR_ITERATOR_DOWNCAST_MESSAGE.len())); // load static exception message length
            ctx.emitter.instruction("str x9, [x0, #16]");                       // store static exception message length
            ctx.emitter.instruction("str xzr, [x0, #24]");                      // exception code defaults to zero
            abi::emit_symbol_address(ctx.emitter, "x9", "_exc_value");
            ctx.emitter.instruction("str x0, [x9]");                            // publish the active exception object
            ctx.emitter.instruction("b __rt_throw_current");                    // enter the standard exception unwinder
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rbp");                                // preserve caller frame pointer for exception allocation
            ctx.emitter.instruction("mov rbp, rsp");                            // establish an aligned helper frame
            ctx.emitter.instruction("sub rsp, 16");                             // keep the nested heap allocation call aligned
            ctx.emitter.instruction("mov rax, 32");                             // request Throwable payload storage
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov r10, 0x4548504c00000006");             // materialize the x86_64 object heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp allocation as a runtime object
            ctx.emitter.instruction("mov r10, QWORD PTR [rip + _spl_logic_exception_class_id]"); // load LogicException's runtime class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the class id at object header
            ctx.emitter.instruction("lea r10, [rip + _iterator_iterator_downcast_msg]"); // materialize static exception message pointer
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store static exception message pointer
            ctx.emitter.instruction(&format!("mov QWORD PTR [rax + 16], {}", ITERATOR_ITERATOR_DOWNCAST_MESSAGE.len())); // store static exception message length
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");             // exception code defaults to zero
            ctx.emitter.instruction("mov QWORD PTR [rip + _exc_value], rax");   // publish the active exception object
            ctx.emitter.instruction("mov rsp, rbp");                            // release helper frame before throwing
            ctx.emitter.instruction("pop rbp");                                 // restore caller frame pointer before throwing
            ctx.emitter.instruction("jmp __rt_throw_current");                  // enter the standard exception unwinder
        }
    }
}

/// Branches when the saved Traversable candidate implements the requested interface.
fn emit_branch_if_saved_traversable_implements(
    ctx: &mut FunctionContext<'_>,
    interface_name: &str,
    target_label: &str,
) -> Result<()> {
    let interface_id = interface_info_by_name(ctx, interface_name)
        .ok_or_else(|| CodegenIrError::unsupported(format!("missing interface {}", interface_name)))?
        .interface_id as i64;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 0);
            abi::emit_load_int_immediate(ctx.emitter, "x1", interface_id);
            abi::emit_load_int_immediate(ctx.emitter, "x2", 1);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the saved Traversable matches this interface
            ctx.emitter.instruction(&format!("b.ne {}", target_label));         // select the matching IteratorIterator normalization path
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", interface_id);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", 1);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
            ctx.emitter.instruction("test rax, rax");                           // test whether the saved Traversable matches this interface
            ctx.emitter.instruction(&format!("jne {}", target_label));          // select the matching IteratorIterator normalization path
        }
    }
    Ok(())
}

/// Moves the object result into the receiver ABI slot before an interface method call.
fn move_result_to_receiver_arg(ctx: &mut FunctionContext<'_>) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the normalized object result as the method receiver
    }
}

/// Writes the normalized Iterator pointer into IteratorIterator::$inner.
fn emit_iterator_inner_property_from_result(
    ctx: &mut FunctionContext<'_>,
    target: ValueId,
    inner_offset: usize,
) -> Result<()> {
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    let tag_reg = abi::secondary_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(target, base_reg)?;
    abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), base_reg, inner_offset);
    abi::emit_load_int_immediate(ctx.emitter, tag_reg, 6);
    abi::emit_store_to_address(ctx.emitter, tag_reg, base_reg, inner_offset + 8);
    Ok(())
}

/// Lowers builtin Throwable allocation using the compact runtime payload layout.
fn lower_builtin_throwable_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    class_name: &str,
    class_id: u64,
) -> Result<()> {
    if inst.operands.len() > 2 {
        return Err(CodegenIrError::unsupported(format!(
            "{}::__construct with {} EIR operands",
            class_name,
            inst.operands.len()
        )));
    }
    emit_throwable_allocation(ctx, class_id);
    preserve_throwable_for_init(ctx);
    emit_throwable_message_fields(ctx, inst.operands.first().copied())?;
    emit_throwable_code_field(ctx, inst.operands.get(1).copied())?;
    restore_throwable_after_init(ctx);
    store_if_result(ctx, inst)
}

/// Returns true for builtin classes that share PHP's compact Throwable payload.
fn is_builtin_throwable_payload_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "Error"
            | "TypeError"
            | "ValueError"
            | "Exception"
            | "RuntimeException"
            | "JsonException"
            | "FiberError"
            | "LogicException"
            | "BadFunctionCallException"
            | "BadMethodCallException"
            | "DomainException"
            | "InvalidArgumentException"
            | "LengthException"
            | "OutOfRangeException"
            | "OutOfBoundsException"
            | "OverflowException"
            | "RangeException"
            | "UnderflowException"
            | "UnexpectedValueException"
    )
}

/// Returns a class id for Throwable-compatible classes that can use the compact payload.
fn throwable_payload_class_id(ctx: &FunctionContext<'_>, class_name: &str) -> Option<u64> {
    let class_info = ctx.module.class_infos.get(class_name)?;
    if is_builtin_throwable_payload_class(class_name)
        || throwable_payload_compatible_user_class(ctx, class_name, class_info)
    {
        Some(class_info.class_id)
    } else {
        None
    }
}

/// Returns true when a user subclass can reuse the compact Throwable storage layout.
fn throwable_payload_compatible_user_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    class_info: &ClassInfo,
) -> bool {
    super::is_throwable_like_class(ctx, class_name)
        && !class_declares_own_instance_properties(class_name, class_info)
        && !class_declares_own_constructor(class_name, class_info)
}

/// Returns true when `class_name` declares an instance property of its own.
fn class_declares_own_instance_properties(class_name: &str, class_info: &ClassInfo) -> bool {
    class_info
        .property_declaring_classes
        .values()
        .any(|declaring_class| declaring_class == class_name)
}

/// Returns true when `class_name` declares its own `__construct` method.
fn class_declares_own_constructor(class_name: &str, class_info: &ClassInfo) -> bool {
    let constructor_key = php_symbol_key("__construct");
    class_info
        .method_declaring_classes
        .get(&constructor_key)
        .is_some_and(|declaring_class| declaring_class == class_name)
}

/// Allocates a 32-byte Throwable payload and stamps its heap kind and class id.
fn emit_throwable_allocation(ctx: &mut FunctionContext<'_>, class_id: u64) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #32");                             // request compact Throwable payload storage
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #6");                              // heap kind 6 marks runtime object payloads
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the heap header before the Throwable payload
            ctx.emitter.instruction(&format!("mov x9, #{}", class_id));         // materialize the Throwable runtime class id
            ctx.emitter.instruction("str x9, [x0]");                            // store class id at payload offset zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, 32");                             // request compact Throwable payload storage
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 6)); // materialize the x86_64 Throwable heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the heap header before the Throwable payload
            ctx.emitter.instruction(&format!("mov r10, {}", class_id));         // materialize the Throwable runtime class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store class id at payload offset zero
        }
    }
}

/// Saves the newly allocated Throwable object while constructor operands are loaded.
fn preserve_throwable_for_init(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
}

/// Restores the initialized Throwable object to the canonical object result register.
fn restore_throwable_after_init(ctx: &mut FunctionContext<'_>) {
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
}

/// Writes the message pointer and length into the compact Throwable payload.
fn emit_throwable_message_fields(
    ctx: &mut FunctionContext<'_>,
    message: Option<ValueId>,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_throwable_message_fields_aarch64(ctx, message),
        Arch::X86_64 => emit_throwable_message_fields_x86_64(ctx, message),
    }
}

/// Writes AArch64 Throwable message fields from a string operand or an empty default.
fn emit_throwable_message_fields_aarch64(
    ctx: &mut FunctionContext<'_>,
    message: Option<ValueId>,
) -> Result<()> {
    if let Some(message) = message {
        ctx.load_string_value_to_regs(message, "x1", "x2")?;
    } else {
        emit_empty_string_to_regs(ctx, "x1", "x2");
    }
    ctx.emitter.instruction("ldr x9, [sp]");                                    // reload the saved Throwable object for message initialization
    ctx.emitter.instruction("str x1, [x9, #8]");                                // store Throwable message pointer
    ctx.emitter.instruction("str x2, [x9, #16]");                               // store Throwable message length
    Ok(())
}

/// Writes x86_64 Throwable message fields from a string operand or an empty default.
fn emit_throwable_message_fields_x86_64(
    ctx: &mut FunctionContext<'_>,
    message: Option<ValueId>,
) -> Result<()> {
    if let Some(message) = message {
        ctx.load_string_value_to_regs(message, "rax", "rdx")?;
    } else {
        emit_empty_string_to_regs(ctx, "rax", "rdx");
    }
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp]");                        // reload the saved Throwable object for message initialization
    ctx.emitter.instruction("mov QWORD PTR [r11 + 8], rax");                    // store Throwable message pointer
    ctx.emitter.instruction("mov QWORD PTR [r11 + 16], rdx");                   // store Throwable message length
    Ok(())
}

/// Materializes the shared empty string constant into a string register pair.
fn emit_empty_string_to_regs(ctx: &mut FunctionContext<'_>, ptr_reg: &str, len_reg: &str) {
    let (label, len) = ctx.data.add_string(b"");
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Writes the integer exception code into the compact Throwable payload.
fn emit_throwable_code_field(
    ctx: &mut FunctionContext<'_>,
    code: Option<ValueId>,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_throwable_code_field_aarch64(ctx, code),
        Arch::X86_64 => emit_throwable_code_field_x86_64(ctx, code),
    }
}

/// Writes the AArch64 Throwable code field from an integer operand or zero default.
fn emit_throwable_code_field_aarch64(
    ctx: &mut FunctionContext<'_>,
    code: Option<ValueId>,
) -> Result<()> {
    if let Some(code) = code {
        ctx.load_value_to_reg(code, "x1")?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "x1", 0);
    }
    ctx.emitter.instruction("ldr x9, [sp]");                                    // reload the saved Throwable object for code initialization
    ctx.emitter.instruction("str x1, [x9, #24]");                               // store Throwable code
    Ok(())
}

/// Writes the x86_64 Throwable code field from an integer operand or zero default.
fn emit_throwable_code_field_x86_64(
    ctx: &mut FunctionContext<'_>,
    code: Option<ValueId>,
) -> Result<()> {
    if let Some(code) = code {
        ctx.load_value_to_reg(code, "rax")?;
    } else {
        abi::emit_load_int_immediate(ctx.emitter, "rax", 0);
    }
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp]");                        // reload the saved Throwable object for code initialization
    ctx.emitter.instruction("mov QWORD PTR [r11 + 24], rax");                   // store Throwable code
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
        let callable_ty = ctx.value_php_type(callable)?.codegen_repr();
        if callable_ty == PhpType::Str {
            callables::emit_runtime_string_descriptor_value(
                ctx,
                callable,
                callable_arg,
                "fiber_constructor",
            )?;
        } else if matches!(
            &callable_ty,
            PhpType::Array(elem) if matches!(elem.codegen_repr(), PhpType::Mixed | PhpType::Str)
        ) {
            callables::emit_runtime_callable_array_descriptor_value(
                ctx,
                callable,
                "fiber_constructor",
            )?;
            move_fiber_callable_result_to_arg(ctx, callable_arg);
        } else if let PhpType::Object(class_name) = callable_ty {
            callables::emit_invokable_object_descriptor_value(
                ctx,
                callable,
                &class_name,
                "fiber_constructor",
            )?;
            move_fiber_callable_result_to_arg(ctx, callable_arg);
        } else {
            ctx.load_value_to_reg(callable, callable_arg)?;
        }
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

/// Moves a descriptor materialized in the result register into Fiber constructor arg 1.
fn move_fiber_callable_result_to_arg(ctx: &mut FunctionContext<'_>, callable_arg: &str) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    if result_reg == callable_arg {
        return;
    }
    ctx.emitter.instruction(&format!("mov {}, {}", callable_arg, result_reg));  // pass selected callable descriptor to Fiber constructor
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

/// Lowers generic PHP `new $class(...)` into AOT candidates plus the runtime registry fallback.
pub(super) fn lower_dynamic_object_new_mixed(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let class_name_value = expect_operand(inst, 0)?;
    let constructor_args = inst
        .operands
        .get(1..)
        .ok_or_else(|| CodegenIrError::invalid_module("dynamic_object_new_mixed missing class operand"))?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("dynamic_object_new_mixed missing result value"))?;
    let done_label = ctx.next_label("dynamic_new_mixed_done");
    let non_string_label = ctx.next_label("dynamic_new_mixed_non_string");
    if !emit_generic_dynamic_new_class_string(ctx, class_name_value, &non_string_label)? {
        emit_boxed_null(ctx);
        return store_if_result(ctx, inst);
    }
    abi::emit_push_result_value(ctx.emitter, &PhpType::Str);

    let fallback_label = ctx.next_label("dynamic_new_mixed_fallback");
    let candidates = dynamic_new_mixed_candidates(ctx, constructor_args.len(), inst)?;
    let case_labels = candidates
        .iter()
        .map(|candidate| {
            let label = ctx.next_label("dynamic_new_mixed_case");
            emit_branch_if_dynamic_new_mixed_class_name_matches(ctx, &candidate.class_name, &label);
            label
        })
        .collect::<Vec<_>>();
    abi::emit_jump(ctx.emitter, &fallback_label);

    for (candidate, label) in candidates.iter().zip(case_labels.iter()) {
        ctx.emitter.label(label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        emit_dynamic_new_mixed_candidate(ctx, candidate, constructor_args, class_name_value, result)?;
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&fallback_label);
    emit_dynamic_new_mixed_fallback(ctx);
    ctx.store_result_value(result)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&non_string_label);
    emit_boxed_null(ctx);
    ctx.store_result_value(result)?;

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes the dynamic class name as a string result pair, branching for non-string Mixed.
fn emit_generic_dynamic_new_class_string(
    ctx: &mut FunctionContext<'_>,
    class_name_value: ValueId,
    non_string_label: &str,
) -> Result<bool> {
    let class_ty = ctx.value_php_type(class_name_value)?.codegen_repr();
    match class_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            ctx.load_string_value_to_regs(class_name_value, ptr_reg, len_reg)?;
            Ok(true)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, class_name_value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x0, #1");                      // require a boxed string class name for dynamic construction
                    ctx.emitter.instruction(&format!("b.ne {}", non_string_label)); // non-string class names produce the runtime null fallback
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rax, 1");                      // require a boxed string class name for dynamic construction
                    ctx.emitter.instruction(&format!("jne {}", non_string_label)); // non-string class names produce the runtime null fallback
                    ctx.emitter.instruction("mov rax, rdi");                    // move the unboxed string pointer into the string result register
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Returns AOT dynamic-new candidates in stable class-id order.
fn dynamic_new_mixed_candidates(
    ctx: &FunctionContext<'_>,
    arg_count: usize,
    inst: &Instruction,
) -> Result<Vec<DynamicNewCandidate>> {
    let mut candidates = Vec::new();
    let mut sorted_classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in sorted_classes {
        if !is_dynamic_new_mixed_aot_candidate(class_name) {
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

/// Returns true when a class can safely use the static allocation path for `new $name`.
fn is_dynamic_new_mixed_aot_candidate(class_name: &str) -> bool {
    if class_name.starts_with("__Elephc") {
        return false;
    }
    if supported_dynamic_new_builtin_class_names().contains(&class_name) {
        return true;
    }
    !known_dynamic_new_builtin_class_names().contains(&class_name)
}

/// Builtin class names with allocation paths that are safe for dynamic `new`.
fn supported_dynamic_new_builtin_class_names() -> &'static [&'static str] {
    &[
        "ArrayIterator",
        "ArrayObject",
        "BadFunctionCallException",
        "BadMethodCallException",
        "CallbackFilterIterator",
        "DomainException",
        "Error",
        "Exception",
        "Fiber",
        "FiberError",
        "InvalidArgumentException",
        "IteratorIterator",
        "JsonException",
        "LengthException",
        "LogicException",
        "OutOfBoundsException",
        "OutOfRangeException",
        "OverflowException",
        "RangeException",
        "RecursiveCallbackFilterIterator",
        "ReflectionClass",
        "ReflectionMethod",
        "ReflectionProperty",
        "RuntimeException",
        "SplDoublyLinkedList",
        "SplFixedArray",
        "SplQueue",
        "SplStack",
        "TypeError",
        "UnderflowException",
        "UnexpectedValueException",
        "ValueError",
        "stdClass",
    ]
}

/// Builtin class names that must not be mistaken for user-instantiable classes.
fn known_dynamic_new_builtin_class_names() -> &'static [&'static str] {
    &[
        "AppendIterator",
        "ArrayIterator",
        "ArrayObject",
        "BadFunctionCallException",
        "BadMethodCallException",
        "CachingIterator",
        "CallbackFilterIterator",
        "DirectoryIterator",
        "DomainException",
        "EmptyIterator",
        "Error",
        "Exception",
        "Fiber",
        "FiberError",
        "FilesystemIterator",
        "FilterIterator",
        "Generator",
        "GlobIterator",
        "InfiniteIterator",
        "InternalIterator",
        "InvalidArgumentException",
        "IteratorIterator",
        "JsonException",
        "LengthException",
        "LimitIterator",
        "LogicException",
        "MultipleIterator",
        "NoRewindIterator",
        "OutOfBoundsException",
        "OutOfRangeException",
        "OverflowException",
        "ParentIterator",
        "Phar",
        "PharData",
        "RangeException",
        "RecursiveArrayIterator",
        "RecursiveCachingIterator",
        "RecursiveCallbackFilterIterator",
        "RecursiveDirectoryIterator",
        "RecursiveFilterIterator",
        "RecursiveIteratorIterator",
        "RecursiveRegexIterator",
        "ReflectionAttribute",
        "ReflectionClass",
        "ReflectionMethod",
        "ReflectionProperty",
        "RegexIterator",
        "RuntimeException",
        "SplDoublyLinkedList",
        "SplFileInfo",
        "SplFileObject",
        "SplFixedArray",
        "SplHeap",
        "SplMaxHeap",
        "SplMinHeap",
        "SplObjectStorage",
        "SplPriorityQueue",
        "SplQueue",
        "SplStack",
        "SplTempFileObject",
        "TypeError",
        "UnderflowException",
        "UnexpectedValueException",
        "ValueError",
        "stdClass",
    ]
}

/// Branches when the saved dynamic class-string matches one AOT candidate class.
fn emit_branch_if_dynamic_new_mixed_class_name_matches(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    matched_label: &str,
) {
    let (candidate_label, candidate_len) = ctx.data.add_string(class_name.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            abi::emit_symbol_address(ctx.emitter, "x3", &candidate_label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", candidate_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("cmp x0, #0");                              // check whether the dynamic class-string matches this AOT class
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // select this AOT allocation path on a class-name match
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
            abi::emit_symbol_address(ctx.emitter, "rdx", &candidate_label);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", candidate_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_strcasecmp");
            ctx.emitter.instruction("test rax, rax");                           // check whether the dynamic class-string matches this AOT class
            ctx.emitter.instruction(&format!("je {}", matched_label));          // select this AOT allocation path on a class-name match
        }
    }
}

/// Allocates one generic dynamic-new candidate, runs defaults/constructor, and boxes it as Mixed.
fn emit_dynamic_new_mixed_candidate(
    ctx: &mut FunctionContext<'_>,
    candidate: &DynamicNewCandidate,
    constructor_args: &[ValueId],
    dummy_receiver_operand: ValueId,
    result: ValueId,
) -> Result<()> {
    if candidate.class_name == "SplFixedArray" {
        return emit_dynamic_new_mixed_spl_fixed_array_candidate(
            ctx,
            candidate.class_id,
            constructor_args,
            result,
        );
    }
    if is_spl_doubly_linked_list_family(&candidate.class_name) {
        return emit_dynamic_new_mixed_spl_dll_candidate(ctx, candidate.class_id, result);
    }
    emit_object_allocation(
        ctx,
        candidate.class_id,
        candidate.property_count,
        candidate.allow_dynamic_properties,
        &candidate.uninitialized_marker_offsets,
    )?;
    let object_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, object_reg);
    let object_base_reg = abi::secondary_scratch_reg(ctx.emitter);
    for default in &candidate.property_defaults {
        abi::emit_load_temporary_stack_slot(ctx.emitter, object_base_reg, 0);
        emit_property_default(ctx, object_base_reg, default)?;
    }
    if let Some(constructor) = &candidate.constructor_impl {
        emit_dynamic_new_mixed_constructor_call(
            ctx,
            candidate,
            constructor,
            constructor_args,
            dummy_receiver_operand,
        )?;
    }
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    emit_box_current_value_as_mixed(
        ctx.emitter,
        &PhpType::Object(candidate.class_name.clone()),
    );
    ctx.store_result_value(result)
}

/// Allocates a dynamic `SplFixedArray` candidate through its runtime storage constructor.
fn emit_dynamic_new_mixed_spl_fixed_array_candidate(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    constructor_args: &[ValueId],
    result: ValueId,
) -> Result<()> {
    if constructor_args.len() > 1 {
        return Err(CodegenIrError::unsupported(format!(
            "dynamic SplFixedArray constructor with {} EIR operands",
            constructor_args.len()
        )));
    }
    if let Some(size) = constructor_args.first().copied() {
        ctx.load_value_to_result(size)?;
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            class_id as i64,
        );
        abi::emit_pop_reg(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1));
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            class_id as i64,
        );
        abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 1), 0);
    }
    abi::emit_call_label(ctx.emitter, "__rt_spl_fixed_new");
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Object("SplFixedArray".to_string()));
    ctx.store_result_value(result)
}

/// Allocates a dynamic SPL doubly-linked-list-family candidate through runtime storage.
fn emit_dynamic_new_mixed_spl_dll_candidate(
    ctx: &mut FunctionContext<'_>,
    class_id: u64,
    result: ValueId,
) -> Result<()> {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_arg_reg_name(ctx.emitter.target, 0),
        class_id as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_spl_dll_new");
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Object(String::new()));
    ctx.store_result_value(result)
}

/// Calls the selected candidate constructor while the new object is parked on the temp stack.
fn emit_dynamic_new_mixed_constructor_call(
    ctx: &mut FunctionContext<'_>,
    candidate: &DynamicNewCandidate,
    constructor: &ConstructorCallTarget,
    constructor_args: &[ValueId],
    dummy_receiver_operand: ValueId,
) -> Result<()> {
    let object_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_temporary_stack_slot(ctx.emitter, object_reg, 0);
    let mut operands = Vec::with_capacity(constructor_args.len() + 1);
    operands.push(dummy_receiver_operand);
    operands.extend(constructor_args.iter().copied());
    let object_ty = PhpType::Object(candidate.class_name.clone());
    let mut param_types = Vec::with_capacity(constructor.param_types.len() + 1);
    param_types.push(object_ty.clone());
    param_types.extend_from_slice(&constructor.param_types);
    let mut ref_params = Vec::with_capacity(constructor.ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend_from_slice(&constructor.ref_params);
    let call_args = materialize_method_call_args_with_receiver_reg_and_refs(
        ctx,
        object_reg,
        &object_ty,
        &operands,
        &param_types,
        &ref_params,
    )?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(
        ctx.emitter,
        &method_symbol(&constructor.impl_class, &php_symbol_key("__construct")),
    );
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Invokes the runtime class-name registry fallback and boxes object/null as Mixed.
fn emit_dynamic_new_mixed_fallback(ctx: &mut FunctionContext<'_>) {
    let null_label = ctx.next_label("dynamic_new_mixed_null");
    let done_label = ctx.next_label("dynamic_new_mixed_fallback_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_call_label(ctx.emitter, "__rt_new_by_name");
            ctx.emitter.instruction(&format!("cbz x0, {}", null_label));        // registry miss returns PHP null for dynamic construction
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Object(String::new()));
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip null boxing after a registry allocation
            ctx.emitter.label(&null_label);
            emit_boxed_null(ctx);
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
            abi::emit_call_label(ctx.emitter, "__rt_new_by_name");
            ctx.emitter.instruction("test rax, rax");                           // registry miss returns PHP null for dynamic construction
            ctx.emitter.instruction(&format!("jz {}", null_label));             // box PHP null when no runtime class table entry matched
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Object(String::new()));
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip null boxing after a registry allocation
            ctx.emitter.label(&null_label);
            emit_boxed_null(ctx);
            ctx.emitter.label(&done_label);
        }
    }
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
    if let Some(candidate) =
        spl_runtime_storage_dynamic_new_candidate(class_name, class_info, arg_count)
    {
        return Ok(Some(candidate));
    }
    if class_interfaces_require_missing_method_symbols(ctx, class_name, class_info) {
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
        Some(ConstructorCallTarget {
            impl_class,
            param_types,
            ref_params: constructor.ref_params.clone(),
        })
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
        allow_dynamic_properties: class_info.allow_dynamic_properties,
        uninitialized_marker_offsets: uninitialized_property_marker_offsets(class_info),
        property_defaults,
        constructor_impl,
    }))
}

/// Builds dynamic-new metadata for SPL classes whose storage is runtime-managed.
fn spl_runtime_storage_dynamic_new_candidate(
    class_name: &str,
    class_info: &ClassInfo,
    arg_count: usize,
) -> Option<DynamicNewCandidate> {
    if class_name == "SplFixedArray" {
        if arg_count > 1 {
            return None;
        }
    } else if is_spl_doubly_linked_list_family(class_name) {
        if arg_count != 0 {
            return None;
        }
    } else {
        return None;
    }
    Some(DynamicNewCandidate {
        class_name: class_name.to_string(),
        class_id: class_info.class_id,
        property_count: class_info.properties.len(),
        allow_dynamic_properties: class_info.allow_dynamic_properties,
        uninitialized_marker_offsets: Vec::new(),
        property_defaults: Vec::new(),
        constructor_impl: None,
    })
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
        candidate.allow_dynamic_properties,
        &candidate.uninitialized_marker_offsets,
    )?;
    ctx.store_result_value(result)?;
    emit_property_defaults(ctx, result, &candidate.property_defaults)?;
    if let Some(constructor) = &candidate.constructor_impl {
        emit_constructor_call(
            ctx,
            result,
            constructor_args,
            &candidate.class_name,
            &constructor.impl_class,
            &php_symbol_key("__construct"),
            &constructor.param_types,
            &constructor.ref_params,
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
            emit_string_literal_default_to_result(ctx, value);
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, object_reg, default.offset);
            abi::emit_store_to_address(ctx.emitter, len_reg, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::Null => {
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::NullSentinel => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, RUNTIME_NULL_SENTINEL);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::TaggedNull => {
            emit_tagged_null_literal_to_result(ctx);
            abi::emit_store_to_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                object_reg,
                default.offset,
            );
            abi::emit_store_to_address(
                ctx.emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
                object_reg,
                default.offset + 8,
            );
        }
        LiteralDefaultValue::BoxedNull => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_boxed_null_literal_to_result(ctx);
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::BoxedStr(value) => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_boxed_string_literal_default_to_result(ctx, value);
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::BoxedInt(value) => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_boxed_int_literal_to_result(ctx, *value);
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::BoxedBool(value) => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_boxed_bool_literal_to_result(ctx, *value);
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::BoxedFloat(value) => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_boxed_float_literal_to_result(ctx, *value);
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
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
        LiteralDefaultValue::AssocArray {
            value_type,
            elements,
        } => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_assoc_array_literal_default_to_result(ctx, value_type, elements)?;
            abi::emit_pop_reg(ctx.emitter, object_reg);
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, int_reg, object_reg, default.offset);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, default.offset + 8);
        }
        LiteralDefaultValue::EmptyAssocArray { value_type } => {
            abi::emit_push_reg(ctx.emitter, object_reg);
            emit_empty_assoc_array_literal_to_result(ctx, value_type);
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
    constructor_ref_params: &[bool],
) -> Result<()> {
    let mut args = Vec::with_capacity(constructor_args.len() + 1);
    args.push(object);
    args.extend(constructor_args.iter().copied());
    let mut param_types = Vec::with_capacity(constructor_param_types.len() + 1);
    param_types.push(PhpType::Object(class_name.to_string()));
    param_types.extend_from_slice(constructor_param_types);
    let mut ref_params = Vec::with_capacity(constructor_ref_params.len() + 1);
    ref_params.push(false);
    ref_params.extend_from_slice(constructor_ref_params);
    let call_args = materialize_direct_call_args_with_refs(ctx, &args, &param_types, &ref_params)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, call_args.overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &method_symbol(impl_class, constructor_key));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, call_args.overflow_bytes);
    emit_ref_arg_writebacks(ctx, &call_args.ref_writebacks)
}

/// Lowers a declared object property read for statically known object receivers.
pub(super) fn lower_prop_get(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property = property_name_immediate(ctx, inst)?.to_string();
    if let Some((class_name, true)) = nullable_object_receiver_class(ctx, object)? {
        return lower_nullable_prop_get_with_warning(ctx, inst, object, &class_name, &property);
    }
    if let Some(class_name) = union_object_member_class(ctx, object)? {
        return lower_union_object_prop_get(ctx, inst, object, &class_name, &property);
    }
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return lower_mixed_prop_get(ctx, inst, object, &property);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_stdclass_prop_get(ctx, inst, object, &property);
    }
    if let Some(class_name) = magic_get_receiver_class(ctx, object, &property)? {
        return lower_magic_get_prop(ctx, inst, object, &class_name, &property);
    }
    if let Some(offset) = dynamic_property_hash_offset_for_object(ctx, object, &property)? {
        return lower_allow_dynamic_prop_get(ctx, inst, object, &property, offset);
    }
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    emit_property_load(ctx, &slot, base_reg)?;
    materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
    store_if_result(ctx, inst)
}

/// Returns the receiver class when an undeclared property should route through `__get`.
fn magic_get_receiver_class(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    property: &str,
) -> Result<Option<String>> {
    let PhpType::Object(class_name) = ctx.value_php_type(object)?.codegen_repr() else {
        return Ok(None);
    };
    let normalized = class_name.trim_start_matches('\\');
    let Some(class_info) = ctx.module.class_infos.get(normalized) else {
        return Ok(None);
    };
    if class_info.properties.iter().any(|(name, _)| name == property) {
        return Ok(None);
    }
    if class_info.methods.contains_key(&php_symbol_key("__get")) {
        return Ok(Some(normalized.to_string()));
    }
    Ok(None)
}

/// Lowers a missing declared-property read by calling the class `__get` method.
fn lower_magic_get_prop(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    let target = resolve_method_call_target(ctx, class_name, "__get", 2)?;
    if target.ref_params.first().copied().unwrap_or(false) {
        return Err(CodegenIrError::unsupported(format!(
            "magic __get by-reference name parameter on {}",
            class_name
        )));
    }
    emit_magic_get_args(ctx, object, property)?;
    if let Some(slot) = target.dynamic_slot {
        super::emit_dynamic_instance_method_call(ctx, slot);
    } else {
        abi::emit_call_label(ctx.emitter, &method_symbol(&target.impl_class, &target.method_key));
    }
    store_call_result(ctx, inst, &target.return_ty)
}

/// Loads `$this` and the static property name into ABI registers for `__get`.
fn emit_magic_get_args(
    ctx: &mut FunctionContext<'_>,
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
    Ok(())
}

/// Lowers a named property read from a statically known stdClass receiver.
fn lower_stdclass_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    emit_stdclass_get_call(ctx, object, property)?;
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Calls the stdClass runtime getter for an object receiver and static property name.
fn emit_stdclass_get_call(
    ctx: &mut FunctionContext<'_>,
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
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
    Ok(())
}

/// Lowers a static-name read from an undeclared property on an allow-dynamic class.
fn lower_allow_dynamic_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
    hash_offset: usize,
) -> Result<()> {
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let (label, key_len) = ctx.data.add_string(property.as_bytes());
    let miss_label = ctx.next_label("dynamic_prop_miss");
    let done_label = ctx.next_label("dynamic_prop_done");
    ctx.load_value_to_reg(object, object_reg)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x0, [{}, #{}]", object_reg, hash_offset)); // load the dynamic-property hash pointer from the receiver
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction(&format!("cbz x0, {}", miss_label));        // missing dynamic properties read as PHP null
            ctx.emitter.instruction("mov x0, x1");                              // return the boxed Mixed cell stored in the hash entry
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the null fallback after a successful dynamic-property hit
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rdi, QWORD PTR [{} + {}]", object_reg, hash_offset)); // load the dynamic-property hash pointer from the receiver
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_get");
            ctx.emitter.instruction("test rax, rax");                           // check whether the dynamic-property key was present
            ctx.emitter.instruction(&format!("je {}", miss_label));             // missing dynamic properties read as PHP null
            ctx.emitter.instruction("mov rax, rdi");                            // return the boxed Mixed cell stored in the hash entry
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the null fallback after a successful dynamic-property hit
        }
    }
    ctx.emitter.label(&miss_label);
    emit_boxed_null(ctx);
    ctx.emitter.label(&done_label);
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Lowers a declared-property read from a boxed union that may hold one known object class.
fn lower_union_object_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
    let object_label = ctx.next_label("union_prop_object");
    let done_label = ctx.next_label("union_prop_done");
    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_object(ctx, &object_label);
    emit_dynamic_property_miss_result(ctx, inst);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&object_label);
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    move_mixed_unboxed_object_payload(ctx, base_reg);
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    emit_property_load(ctx, &slot, base_reg)?;
    materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `$mixed->property` through the shared stdClass-aware runtime helper.
fn lower_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
) -> Result<()> {
    let candidates = declared_mixed_property_candidates(ctx, property, inst)?;
    if !candidates.is_empty() {
        return lower_declared_mixed_prop_get(ctx, inst, object, property, candidates);
    }
    lower_runtime_mixed_prop_get(ctx, inst, object, property)
}

/// Lowers a `Mixed` receiver by dispatching known user classes before stdClass fallback.
fn lower_declared_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property: &str,
    candidates: Vec<MixedPropertyCandidate>,
) -> Result<()> {
    let null_label = ctx.next_label("mixed_prop_null");
    let done_label = ctx.next_label("mixed_prop_done");
    let stdclass_label = ctx.next_label("mixed_prop_stdclass");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_prop_{}",
                label_fragment(&candidate.slot.class_name)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_mixed_object_payload_or_null(ctx, &null_label);
    emit_mixed_property_class_dispatch(ctx, &candidates, &match_labels, &stdclass_label);

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let base_reg = abi::int_result_reg(ctx.emitter);
        if candidate.slot.is_declared {
            emit_uninitialized_typed_property_guard(ctx, &candidate.slot, base_reg);
        }
        emit_property_load(ctx, &candidate.slot, base_reg)?;
        box_mixed_property_candidate_result(ctx, &candidate.slot.php_type);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&stdclass_label);
    emit_stdclass_get_from_loaded_object(ctx, property);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_boxed_null(ctx);

    ctx.emitter.label(&done_label);
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Lowers a `Mixed` receiver through the runtime stdClass-style property helper.
fn lower_runtime_mixed_prop_get(
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

/// Collects declared-property candidates for a property read on an unknown `Mixed` object.
fn declared_mixed_property_candidates(
    ctx: &FunctionContext<'_>,
    property: &str,
    inst: &Instruction,
) -> Result<Vec<MixedPropertyCandidate>> {
    let mut candidates = Vec::new();
    for (class_name, class_info) in &ctx.module.class_infos {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
            continue;
        }
        if !class_info
            .properties
            .iter()
            .any(|(name, _)| name == property)
        {
            continue;
        }
        let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
        candidates.push(MixedPropertyCandidate {
            class_id: class_info.class_id,
            slot,
        });
    }
    candidates.sort_by_key(|candidate| candidate.class_id);
    Ok(candidates)
}

/// Promotes an unboxed Mixed object payload into the normal result register or jumps to null.
fn emit_mixed_object_payload_or_null(ctx: &mut FunctionContext<'_>, null_label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // check whether the Mixed receiver holds an object payload
            ctx.emitter.instruction(&format!("b.ne {}", null_label));           // non-object Mixed receivers produce a null property result
            ctx.emitter.instruction("mov x0, x1");                              // promote the unboxed object payload for class-id dispatch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // check whether the Mixed receiver holds an object payload
            ctx.emitter.instruction(&format!("jne {}", null_label));            // non-object Mixed receivers produce a null property result
            ctx.emitter.instruction("mov rax, rdi");                            // promote the unboxed object payload for class-id dispatch
        }
    }
}

/// Emits class-id dispatch for declared property candidates and stdClass fallback.
fn emit_mixed_property_class_dispatch(
    ctx: &mut FunctionContext<'_>,
    candidates: &[MixedPropertyCandidate],
    match_labels: &[String],
    stdclass_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the receiver class id for Mixed property dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "x10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp x9, x10");                         // compare the receiver class id against this declared-property owner
                ctx.emitter.instruction(&format!("b.eq {}", label));            // read the declared property when the class id matches
            }
            emit_branch_to_stdclass_candidate(ctx, "x9", "x10", stdclass_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r11, QWORD PTR [rax]");                // load the receiver class id for Mixed property dispatch
            for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
                abi::emit_load_int_immediate(ctx.emitter, "r10", candidate.class_id as i64);
                ctx.emitter.instruction("cmp r11, r10");                        // compare the receiver class id against this declared-property owner
                ctx.emitter.instruction(&format!("je {}", label));              // read the declared property when the class id matches
            }
            emit_branch_to_stdclass_candidate(ctx, "r11", "r10", stdclass_label);
        }
    }
}

/// Branches to the stdClass fallback when the runtime module contains stdClass metadata.
fn emit_branch_to_stdclass_candidate(
    ctx: &mut FunctionContext<'_>,
    class_id_reg: &str,
    scratch_reg: &str,
    stdclass_label: &str,
) {
    let Some(stdclass_id) = stdclass_class_id(ctx) else {
        return;
    };
    abi::emit_load_int_immediate(ctx.emitter, scratch_reg, stdclass_id as i64);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, scratch_reg)); // check whether the object uses stdClass dynamic storage
            ctx.emitter.instruction(&format!("b.eq {}", stdclass_label));       // route stdClass reads through the hash-backed helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", class_id_reg, scratch_reg)); // check whether the object uses stdClass dynamic storage
            ctx.emitter.instruction(&format!("je {}", stdclass_label));         // route stdClass reads through the hash-backed helper
        }
    }
}

/// Returns the runtime class id assigned to stdClass in this module.
fn stdclass_class_id(ctx: &FunctionContext<'_>) -> Option<u64> {
    ctx.module
        .class_infos
        .iter()
        .find(|(class_name, _)| crate::types::checker::builtin_stdclass::is_stdclass(class_name))
        .map(|(_, class_info)| class_info.class_id)
}

/// Boxes a declared-property load so all Mixed receiver paths produce a Mixed cell.
fn box_mixed_property_candidate_result(ctx: &mut FunctionContext<'_>, source_ty: &PhpType) {
    if source_ty.codegen_repr() != PhpType::Mixed {
        emit_box_current_value_as_mixed(ctx.emitter, &source_ty.codegen_repr());
    }
}

/// Reads a static property name from an already-unboxed stdClass payload.
fn emit_stdclass_get_from_loaded_object(ctx: &mut FunctionContext<'_>, property: &str) {
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the unboxed stdClass object pointer to the dynamic getter
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
        }
    }
}

/// Branches when `__rt_mixed_unbox` returned an object payload tag.
fn emit_branch_if_mixed_unboxed_object(ctx: &mut FunctionContext<'_>, object_label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // runtime tag 6 means the boxed union holds an object payload
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // read the declared property only for object payloads
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // runtime tag 6 means the boxed union holds an object payload
            ctx.emitter.instruction(&format!("je {}", object_label));           // read the declared property only for object payloads
        }
    }
}

/// Moves the low payload produced by `__rt_mixed_unbox` into the object base register.
fn move_mixed_unboxed_object_payload(ctx: &mut FunctionContext<'_>, base_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, x1", base_reg));          // use the unboxed object pointer as the declared-property base
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, rdi", base_reg));         // use the unboxed object pointer as the declared-property base
        }
    }
}

/// Lowers `$maybeObject->property`, warning when the receiver is PHP null.
fn lower_nullable_prop_get_with_warning(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
    let null_label = ctx.next_label("nullable_prop_warning_null");
    let done_label = ctx.next_label("nullable_prop_warning_done");
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    emit_nullable_receiver_object_payload(ctx, object, &null_label, base_reg)?;
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    emit_property_load(ctx, &slot, base_reg)?;
    materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_property_on_null_warning(ctx, property);
    emit_boxed_null(ctx);

    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Emits PHP's warning for reading a property from null.
fn emit_property_on_null_warning(ctx: &mut FunctionContext<'_>, property: &str) {
    let message = format!("Warning: Attempt to read property \"{}\" on null\n", property);
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));      // pass the property-on-null warning byte length
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &message_label);
            ctx.emitter.instruction(&format!("mov esi, {}", message_len));      // pass the property-on-null warning byte length
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
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
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return lower_runtime_dynamic_mixed_prop_get(ctx, inst, object, property_value);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_runtime_dynamic_stdclass_prop_get(ctx, inst, object, property_value);
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
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return lower_mixed_prop_get(ctx, inst, object, property);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_stdclass_prop_get(ctx, inst, object, property);
    }
    if let Some(class_name) = magic_get_receiver_class(ctx, object, property)? {
        return lower_magic_get_prop(ctx, inst, object, &class_name, property);
    }
    if let Some(offset) = dynamic_property_hash_offset_for_object(ctx, object, property)? {
        return lower_allow_dynamic_prop_get(ctx, inst, object, property, offset);
    }
    let slot = resolve_property_slot(ctx, object, property, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    if slot.is_declared {
        emit_uninitialized_typed_property_guard(ctx, &slot, base_reg);
    }
    emit_property_load(ctx, &slot, base_reg)?;
    materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
    store_if_result(ctx, inst)
}

/// Lowers a runtime-name dynamic property read from a boxed `Mixed` receiver.
fn lower_runtime_dynamic_mixed_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property_value: ValueId,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            ctx.load_string_value_to_regs(property_value, "x1", "x2")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            ctx.load_string_value_to_regs(property_value, "rsi", "rdx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_property_get");
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
    store_if_result(ctx, inst)
}

/// Lowers a runtime-name dynamic property read from a statically known `stdClass`.
fn lower_runtime_dynamic_stdclass_prop_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    property_value: ValueId,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            ctx.load_string_value_to_regs(property_value, "x1", "x2")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            ctx.load_string_value_to_regs(property_value, "rsi", "rdx")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
    cast_loaded_mixed_pointer_to_result(ctx, &inst.result_php_type.codegen_repr())?;
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
        materialize_loaded_property_result(ctx, inst, &slot.php_type)?;
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

/// Converts a just-loaded property payload into the EIR result representation.
fn materialize_loaded_property_result(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source_ty: &PhpType,
) -> Result<()> {
    match inst.result_php_type.codegen_repr() {
        PhpType::Mixed if source_ty.codegen_repr() != PhpType::Mixed => {
            emit_box_current_value_as_mixed(ctx.emitter, &source_ty.codegen_repr());
            Ok(())
        }
        PhpType::TaggedScalar if source_ty.codegen_repr() != PhpType::TaggedScalar => {
            super::coerce_loaded_value_to_tagged_scalar(ctx, source_ty)?;
            Ok(())
        }
        _ => Ok(()),
    }
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
    if let Some((class_name, true)) = nullable_object_receiver_class(ctx, object)? {
        return lower_nullable_prop_set(ctx, inst, object, value, &class_name, &property);
    }
    if matches!(ctx.value_php_type(object)?.codegen_repr(), PhpType::Mixed) {
        return lower_mixed_prop_set(ctx, object, value, &property);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_stdclass_prop_set(ctx, object, value, &property);
    }
    if let Some(offset) = dynamic_property_hash_offset_for_object(ctx, object, &property)? {
        return lower_allow_dynamic_prop_set(ctx, object, value, &property, offset);
    }
    let slot = resolve_property_slot(ctx, object, &property, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    if is_promoted_reference_property_bind(ctx, object, value, &slot)? {
        return emit_reference_property_bind(ctx, value, &slot, base_reg);
    }
    emit_property_store(ctx, value, &slot, base_reg)
}

/// Lowers a dynamic property write (`$object->{$name} = $value`).
pub(super) fn lower_dynamic_prop_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let object = expect_operand(inst, 0)?;
    let property_value = expect_operand(inst, 1)?;
    let value = expect_operand(inst, 2)?;
    if let Some(property) = const_string_operand(ctx, property_value)? {
        return lower_const_dynamic_prop_set(ctx, object, value, property, inst);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_runtime_stdclass_prop_set(ctx, object, property_value, value, inst);
    }
    match ctx.value_php_type(object)?.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => {
            lower_runtime_mixed_prop_set(ctx, object, property_value, value, inst)
        }
        PhpType::Object(class_name) => {
            lower_runtime_object_prop_set(ctx, object, property_value, value, &class_name, inst)
        }
        object_ty => Err(CodegenIrError::unsupported(format!(
            "{} for receiver PHP type {:?}",
            inst.op.name(),
            object_ty
        ))),
    }
}

/// Lowers a dynamic property write when the name expression folded to a string.
fn lower_const_dynamic_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
    inst: &Instruction,
) -> Result<()> {
    if matches!(
        ctx.value_php_type(object)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return lower_mixed_prop_set(ctx, object, value, property);
    }
    if object_is_builtin_stdclass(ctx, object)? {
        return lower_stdclass_prop_set(ctx, object, value, property);
    }
    if let Some(offset) = dynamic_property_hash_offset_for_object(ctx, object, property)? {
        return lower_allow_dynamic_prop_set(ctx, object, value, property, offset);
    }
    let slot = resolve_property_slot(ctx, object, property, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(object, base_reg)?;
    emit_property_store(ctx, value, &slot, base_reg)
}

/// Lowers a runtime-name write to a statically known stdClass receiver.
fn lower_runtime_stdclass_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
            abi::emit_pop_reg(ctx.emitter, "x3");
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
            abi::emit_pop_reg(ctx.emitter, "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    Ok(())
}

/// Lowers a runtime-name write to a known class by comparing against declared slots.
fn lower_runtime_object_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    class_name: &str,
    inst: &Instruction,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    let slots = declared_dynamic_property_set_slots(ctx, class_name, value, inst)?;
    let match_labels = slots
        .iter()
        .map(|slot| ctx.next_label(&format!("dyn_prop_set_{}", label_fragment(&slot.property))))
        .collect::<Vec<_>>();
    let miss_label = ctx.next_label("dyn_prop_set_miss");
    let done_label = ctx.next_label("dyn_prop_set_done");

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
        emit_property_store(ctx, value, slot, base_reg)?;
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a runtime-name write when the receiver is a boxed Mixed object.
fn lower_runtime_mixed_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    property_value: ValueId,
    value: ValueId,
    inst: &Instruction,
) -> Result<()> {
    ensure_runtime_dynamic_property_name(ctx, property_value, inst)?;
    let candidates = declared_mixed_property_set_candidates(ctx, value, inst)?;
    let done_label = ctx.next_label("mixed_dyn_prop_set_done");
    let miss_label = ctx.next_label("mixed_dyn_prop_set_miss");
    let stdclass_label = ctx.next_label("mixed_dyn_prop_set_stdclass");
    let match_labels = candidates
        .iter()
        .map(|candidate| {
            ctx.next_label(&format!(
                "mixed_dyn_prop_set_{}",
                label_fragment(&candidate.slot.property)
            ))
        })
        .collect::<Vec<_>>();

    ctx.load_value_to_reg(object, abi::int_result_reg(ctx.emitter))?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_if_mixed_unboxed_not_object(ctx, &done_label);
    push_mixed_unboxed_object_payload(ctx);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(property_value, ptr_reg, len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        emit_branch_if_mixed_dynamic_set_candidate_matches(ctx, candidate, label);
    }
    emit_branch_if_stacked_object_is_stdclass(ctx, 16, &stdclass_label);
    abi::emit_jump(ctx.emitter, &miss_label);

    for (candidate, label) in candidates.iter().zip(match_labels.iter()) {
        ctx.emitter.label(label);
        let base_reg = abi::symbol_scratch_reg(ctx.emitter);
        abi::emit_load_temporary_stack_slot(ctx.emitter, base_reg, 16);
        emit_property_store(ctx, value, &candidate.slot, base_reg)?;
        abi::emit_release_temporary_stack(ctx.emitter, 32);
        abi::emit_jump(ctx.emitter, &done_label);
    }

    ctx.emitter.label(&stdclass_label);
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    emit_runtime_stdclass_set_for_stacked_name(ctx, value, &value_ty, 16, 0)?;
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&miss_label);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Resolves declared slots on a known object class that can accept this value.
fn declared_dynamic_property_set_slots(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    value: ValueId,
    inst: &Instruction,
) -> Result<Vec<PropertySlot>> {
    let value_ty = ctx.value_php_type(value)?;
    let normalized = class_name.trim_start_matches('\\');
    let property_names = {
        let class_info =
            ctx.module.class_infos.get(normalized).ok_or_else(|| {
                CodegenIrError::unsupported(format!("unknown class {}", normalized))
            })?;
        class_info
            .properties
            .iter()
            .map(|(property, _)| property.clone())
            .collect::<Vec<_>>()
    };
    let mut slots = Vec::new();
    for property in property_names {
        let slot = resolve_property_slot_for_class(ctx, normalized, &property, inst)?;
        ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
        slots.push(slot);
    }
    Ok(slots)
}

/// Collects Mixed receiver declared-property candidates that can accept this value.
fn declared_mixed_property_set_candidates(
    ctx: &FunctionContext<'_>,
    value: ValueId,
    inst: &Instruction,
) -> Result<Vec<MixedPropertyCandidate>> {
    let value_ty = ctx.value_php_type(value)?;
    let mut candidates = Vec::new();
    let mut sorted_classes = ctx.module.class_infos.iter().collect::<Vec<_>>();
    sorted_classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in sorted_classes {
        if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
            continue;
        }
        for (property, _) in &class_info.properties {
            let Ok(slot) = resolve_property_slot_for_class(ctx, class_name, property, inst) else {
                continue;
            };
            if ensure_property_value_supported(ctx, &slot, value, &value_ty, inst).is_err() {
                continue;
            }
            candidates.push(MixedPropertyCandidate {
                class_id: class_info.class_id,
                slot,
            });
        }
    }
    candidates.sort_by(|left, right| {
        left.class_id
            .cmp(&right.class_id)
            .then_with(|| left.slot.property.cmp(&right.slot.property))
    });
    Ok(candidates)
}

/// Branches to `target_label` when the unboxed Mixed result is not an object.
fn emit_branch_if_mixed_unboxed_not_object(ctx: &mut FunctionContext<'_>, target_label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // check whether the boxed receiver holds an object payload
            ctx.emitter.instruction(&format!("b.ne {}", target_label));         // non-object dynamic property writes are ignored
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // check whether the boxed receiver holds an object payload
            ctx.emitter.instruction(&format!("jne {}", target_label));          // non-object dynamic property writes are ignored
        }
    }
}

/// Pushes the object payload returned by `__rt_mixed_unbox` onto the temp stack.
fn push_mixed_unboxed_object_payload(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg(ctx.emitter, "x1"),
        Arch::X86_64 => abi::emit_push_reg(ctx.emitter, "rdi"),
    }
}

/// Branches when both the stacked object class id and runtime property name match.
fn emit_branch_if_mixed_dynamic_set_candidate_matches(
    ctx: &mut FunctionContext<'_>,
    candidate: &MixedPropertyCandidate,
    matched_label: &str,
) {
    let next_label = ctx.next_label("mixed_dyn_prop_set_next");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 16);
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the candidate receiver class id
            abi::emit_load_int_immediate(ctx.emitter, "x11", candidate.class_id as i64);
            ctx.emitter.instruction("cmp x10, x11");                            // compare receiver class id before checking the property name
            ctx.emitter.instruction(&format!("b.ne {}", next_label));           // skip name comparison for unrelated classes
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", 16);
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // load the candidate receiver class id
            abi::emit_load_int_immediate(ctx.emitter, "r12", candidate.class_id as i64);
            ctx.emitter.instruction("cmp r10, r12");                            // compare receiver class id before checking the property name
            ctx.emitter.instruction(&format!("jne {}", next_label));            // skip name comparison for unrelated classes
        }
    }
    emit_branch_if_dynamic_name_matches(ctx, &candidate.slot.property, matched_label);
    ctx.emitter.label(&next_label);
}

/// Branches when a stacked object payload is a stdClass instance.
fn emit_branch_if_stacked_object_is_stdclass(
    ctx: &mut FunctionContext<'_>,
    object_stack_offset: usize,
    matched_label: &str,
) {
    let Some(stdclass_id) = stdclass_class_id(ctx) else {
        return;
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", object_stack_offset);
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the stacked object's class id
            abi::emit_load_int_immediate(ctx.emitter, "x11", stdclass_id as i64);
            ctx.emitter.instruction("cmp x10, x11");                            // check whether the runtime receiver is stdClass
            ctx.emitter.instruction(&format!("b.eq {}", matched_label));        // route stdClass writes through the dynamic-property helper
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r11", object_stack_offset);
            ctx.emitter.instruction("mov r10, QWORD PTR [r11]");                // load the stacked object's class id
            abi::emit_load_int_immediate(ctx.emitter, "r12", stdclass_id as i64);
            ctx.emitter.instruction("cmp r10, r12");                            // check whether the runtime receiver is stdClass
            ctx.emitter.instruction(&format!("je {}", matched_label));          // route stdClass writes through the dynamic-property helper
        }
    }
}

/// Calls `__rt_stdclass_set` using a stacked object pointer and runtime name pair.
fn emit_runtime_stdclass_set_for_stacked_name(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
    object_stack_offset: usize,
    name_stack_offset: usize,
) -> Result<()> {
    materialize_dynamic_property_mixed_value(ctx, value, value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", object_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", name_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", name_stack_offset + 24);
            abi::emit_pop_reg(ctx.emitter, "x3");
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", object_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", name_stack_offset + 16);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", name_stack_offset + 24);
            abi::emit_pop_reg(ctx.emitter, "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    Ok(())
}

/// Lowers a named property write to a statically known stdClass receiver.
fn lower_stdclass_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            abi::emit_pop_reg(ctx.emitter, "x3");
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            abi::emit_pop_reg(ctx.emitter, "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    Ok(())
}

/// Lowers a named property write through the runtime Mixed object-property setter.
fn lower_mixed_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    let (label, len) = ctx.data.add_string(property.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(object, "x0")?;
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            abi::emit_pop_reg(ctx.emitter, "x3");
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(object, "rdi")?;
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            abi::emit_pop_reg(ctx.emitter, "rcx");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_property_set");
    Ok(())
}

/// Lowers a static-name write to an undeclared property on an allow-dynamic class.
fn lower_allow_dynamic_prop_set(
    ctx: &mut FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    property: &str,
    hash_offset: usize,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?.codegen_repr();
    let object_reg = abi::symbol_scratch_reg(ctx.emitter);
    let boxed_reg = abi::secondary_scratch_reg(ctx.emitter);
    let (label, key_len) = ctx.data.add_string(property.as_bytes());
    ctx.load_value_to_reg(object, object_reg)?;
    abi::emit_push_reg(ctx.emitter, object_reg);
    materialize_dynamic_property_mixed_value(ctx, value, &value_ty)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, x0", boxed_reg));         // preserve the boxed dynamic-property value across receiver restore
            abi::emit_pop_reg(ctx.emitter, object_reg);
            ctx.emitter.instruction(&format!("ldr x0, [{}, #{}]", object_reg, hash_offset)); // load the dynamic-property hash pointer from the receiver
            abi::emit_push_reg(ctx.emitter, object_reg);
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", key_len as i64);
            ctx.emitter.instruction(&format!("mov x3, {}", boxed_reg));         // pass the boxed Mixed cell as the hash value payload
            ctx.emitter.instruction("mov x4, xzr");                             // boxed Mixed hash entries do not use the high payload word
            abi::emit_load_int_immediate(ctx.emitter, "x5", runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "x0", object_reg, hash_offset);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, rax", boxed_reg));        // preserve the boxed dynamic-property value across receiver restore
            abi::emit_pop_reg(ctx.emitter, object_reg);
            ctx.emitter.instruction(&format!("mov rdi, QWORD PTR [{} + {}]", object_reg, hash_offset)); // load the dynamic-property hash pointer from the receiver
            abi::emit_push_reg(ctx.emitter, object_reg);
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", key_len as i64);
            ctx.emitter.instruction(&format!("mov rcx, {}", boxed_reg));        // pass the boxed Mixed cell as the hash value payload
            ctx.emitter.instruction("xor r8, r8");                              // boxed Mixed hash entries do not use the high payload word
            abi::emit_load_int_immediate(ctx.emitter, "r9", runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_set");
            abi::emit_pop_reg(ctx.emitter, object_reg);
            abi::emit_store_to_address(ctx.emitter, "rax", object_reg, hash_offset);
        }
    }
    Ok(())
}

/// Materializes a property value as an owned boxed `Mixed` cell in the result register.
fn materialize_dynamic_property_mixed_value(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    if matches!(value_ty, PhpType::Mixed | PhpType::Union(_)) {
        if !ctx.value_can_own_mixed_box_source(value)? {
            abi::emit_incref_if_refcounted(ctx.emitter, &value_ty.codegen_repr());
        }
    } else {
        emit_box_current_value_as_mixed(ctx.emitter, value_ty);
    }
    Ok(())
}

/// Lowers a property write on a nullable receiver, fataling after RHS evaluation when null.
fn lower_nullable_prop_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    object: ValueId,
    value: ValueId,
    class_name: &str,
    property: &str,
) -> Result<()> {
    let slot = resolve_property_slot_for_class(ctx, class_name, property, inst)?;
    let value_ty = ctx.value_php_type(value)?;
    ensure_property_value_supported(ctx, &slot, value, &value_ty, inst)?;
    let null_label = ctx.next_label("nullable_prop_set_null");
    let done_label = ctx.next_label("nullable_prop_set_done");
    let base_reg = abi::symbol_scratch_reg(ctx.emitter);
    emit_nullable_receiver_object_payload(ctx, object, &null_label, base_reg)?;
    emit_property_store(ctx, value, &slot, base_reg)?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&null_label);
    emit_property_assign_on_null_fatal(ctx, property);

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits PHP's fatal diagnostic for assigning a property on null.
fn emit_property_assign_on_null_fatal(ctx: &mut FunctionContext<'_>, property: &str) {
    let message = format!(
        "Fatal error: Attempt to assign property \"{}\" on null\n",
        property
    );
    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // write the property-assign-on-null fatal to stderr
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));      // pass the property-assign-on-null fatal byte length
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // write the property-assign-on-null fatal to Linux stderr
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter.instruction(&format!("mov edx, {}", message_len));      // pass the property-assign-on-null fatal byte length
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the property-assign-on-null fatal before exiting
            abi::emit_exit(ctx.emitter, 1);
        }
    }
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
    allow_dynamic_properties: bool,
    uninitialized_marker_offsets: &[usize],
) -> Result<()> {
    let dynamic_properties_offset = dynamic_property_hash_offset(property_count);
    let dynamic_properties_bytes = if allow_dynamic_properties { 8 } else { 0 };
    let payload_size = dynamic_properties_offset + dynamic_properties_bytes;
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
    if allow_dynamic_properties {
        emit_dynamic_property_hash_init(ctx, object_reg, dynamic_properties_offset);
    }
    Ok(())
}

/// Returns the byte offset of the dynamic-property hash pointer for this layout.
fn dynamic_property_hash_offset(property_count: usize) -> usize {
    8 + property_count * 16
}

/// Allocates the per-object dynamic-property hash and stores it in the object payload.
fn emit_dynamic_property_hash_init(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    offset: usize,
) {
    let hash_reg = abi::secondary_scratch_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, object_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 4);
            abi::emit_load_int_immediate(ctx.emitter, "x1", runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction(&format!("mov {}, x0", hash_reg));          // preserve the dynamic-property hash across object restore
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", 4);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", runtime_value_tag(&PhpType::Mixed) as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_new");
            ctx.emitter.instruction(&format!("mov {}, rax", hash_reg));         // preserve the dynamic-property hash across object restore
        }
    }
    abi::emit_pop_reg(ctx.emitter, object_reg);
    abi::emit_store_to_address(ctx.emitter, hash_reg, object_reg, offset);
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
            interface_name,
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
    interface_name: &str,
    fallback_class: &str,
    class_info: &ClassInfo,
    interface_info: &InterfaceInfo,
    emitted_methods: &HashSet<(String, String)>,
) -> bool {
    for method_name in &interface_info.method_order {
        if super::is_throwable_standard_method_key(method_name)
            && (super::is_throwable_like_class(ctx, fallback_class)
                || super::is_throwable_like_class(ctx, interface_name))
        {
            continue;
        }
        let impl_class = class_info
            .method_impl_classes
            .get(method_name)
            .map(String::as_str)
            .unwrap_or(fallback_class);
        let key = (impl_class.to_string(), method_name.clone());
        if !emitted_methods.contains(&key)
            && IntrinsicCall::instance_method(impl_class, method_name)
                .and_then(|intrinsic| intrinsic.runtime_helper())
                .is_none()
        {
            return true;
        }
    }
    false
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

/// Returns the dynamic-property hash slot offset for an undeclared allow-dynamic property.
fn dynamic_property_hash_offset_for_object(
    ctx: &FunctionContext<'_>,
    object: crate::ir::ValueId,
    property: &str,
) -> Result<Option<usize>> {
    let object_ty = ctx.value_php_type(object)?;
    let PhpType::Object(class_name) = object_ty else {
        return Ok(None);
    };
    dynamic_property_hash_offset_for_class(ctx, &class_name, property)
}

/// Returns the dynamic-property hash slot offset for a known class and property name.
fn dynamic_property_hash_offset_for_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> Result<Option<usize>> {
    let normalized = class_name.trim_start_matches('\\');
    if is_builtin_stdclass(normalized) {
        return Ok(Some(dynamic_property_hash_offset(0)));
    }
    let class_info = ctx
        .module
        .class_infos
        .get(normalized)
        .ok_or_else(|| CodegenIrError::unsupported(format!("unknown class {}", normalized)))?;
    if class_info.properties.iter().any(|(name, _)| name == property) {
        return Ok(None);
    }
    if class_info.allow_dynamic_properties {
        return Ok(Some(dynamic_property_hash_offset(class_info.properties.len())));
    }
    Ok(None)
}

/// Returns true when a class name is the builtin `stdClass` dynamic-property container.
fn is_builtin_stdclass(class_name: &str) -> bool {
    crate::types::checker::builtin_stdclass::is_stdclass(class_name.trim_start_matches('\\'))
}

/// Returns true when the SSA value is known to hold a stdClass object pointer.
fn object_is_builtin_stdclass(ctx: &FunctionContext<'_>, object: ValueId) -> Result<bool> {
    Ok(matches!(
        ctx.value_php_type(object)?.codegen_repr(),
        PhpType::Object(class_name) if is_builtin_stdclass(&class_name)
    ))
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
    let is_reference = class_info.reference_properties.contains(property);
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
    let php_type = runtime_property_type_override(ctx, normalized, property)
        .unwrap_or_else(|| php_type.clone());
    ensure_property_type_supported(&php_type, inst)?;
    let offset = class_info
        .property_offsets
        .get(property)
        .copied()
        .unwrap_or(8 + index * 16);
    Ok(PropertySlot {
        class_name: normalized.to_string(),
        property: property.to_string(),
        php_type,
        offset,
        is_declared: class_info.declared_properties.contains(property),
        is_packed: false,
        is_reference,
    })
}

/// Returns precise runtime storage types for inherited SPL callback-filter internals.
fn runtime_property_type_override(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    property: &str,
) -> Option<PhpType> {
    if !class_extends_class(ctx, class_name, "CallbackFilterIterator") {
        return None;
    }
    match property {
        "callback" => Some(PhpType::Callable),
        "callbackEnv" => Some(PhpType::Pointer(None)),
        _ => None,
    }
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

/// Returns the unique object class carried by a boxed union, ignoring null and scalar arms.
fn union_object_member_class(
    ctx: &FunctionContext<'_>,
    object: ValueId,
) -> Result<Option<String>> {
    let PhpType::Union(members) = raw_value_php_type(ctx, object)? else {
        return Ok(None);
    };
    let mut class_name = None;
    for member in members {
        let PhpType::Object(candidate) = member else {
            continue;
        };
        if class_name
            .as_ref()
            .is_some_and(|existing: &String| existing != &candidate)
        {
            return Ok(None);
        }
        class_name = Some(candidate);
    }
    Ok(class_name)
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
        is_packed: true,
        is_reference: false,
    })
}

/// Verifies that this slice knows how to represent the property type in an object slot.
fn ensure_property_type_supported(php_type: &PhpType, inst: &Instruction) -> Result<()> {
    match php_type {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Void
        | PhpType::Never => Ok(()),
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
    if can_store_object_for_object_property(ctx, value_ty, &slot.php_type) {
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
    if can_convert_indexed_array_to_mixed_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_coerce_tagged_scalar_to_int_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_store_class_default_in_refined_null_property(ctx, value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_box_value_for_mixed_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_store_boxed_value_for_mixed_property(value_ty, &slot.php_type) {
        return Ok(());
    }
    if can_coerce_mixed_to_scalar_property(value_ty, &slot.php_type) {
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

/// Returns true when a concrete object value is assignable to an object-typed property.
fn can_store_object_for_object_property(
    ctx: &FunctionContext<'_>,
    value_ty: &PhpType,
    slot_ty: &PhpType,
) -> bool {
    let value_ty = value_ty.codegen_repr();
    let slot_ty = slot_ty.codegen_repr();
    let (PhpType::Object(value_name), PhpType::Object(slot_name)) = (&value_ty, &slot_ty) else {
        return false;
    };
    object_type_is_a(ctx, value_name, slot_name)
}

/// Returns true when `source_name` is the same class/interface or inherits `target_name`.
fn object_type_is_a(ctx: &FunctionContext<'_>, source_name: &str, target_name: &str) -> bool {
    if same_php_type_name(source_name, target_name) {
        return true;
    }
    if interface_info_by_name(ctx, target_name).is_some() {
        return object_type_implements_interface(ctx, source_name, target_name);
    }
    class_extends_class(ctx, source_name, target_name)
}

/// Returns true when a class or interface source satisfies an interface target.
fn object_type_implements_interface(
    ctx: &FunctionContext<'_>,
    source_name: &str,
    target_interface: &str,
) -> bool {
    if interface_info_by_name(ctx, source_name).is_some() {
        return interface_extends_interface(ctx, source_name, target_interface);
    }
    let mut current = Some(source_name.to_string());
    while let Some(class_name) = current {
        let Some(class_info) = class_info_by_name(ctx, &class_name) else {
            return false;
        };
        if class_info
            .interfaces
            .iter()
            .any(|interface_name| interface_extends_interface(ctx, interface_name, target_interface))
        {
            return true;
        }
        current = class_info.parent.clone();
    }
    false
}

/// Returns true when an interface is or extends the target interface.
fn interface_extends_interface(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    target_interface: &str,
) -> bool {
    if same_php_type_name(interface_name, target_interface) {
        return true;
    }
    let Some(interface_info) = interface_info_by_name(ctx, interface_name) else {
        return false;
    };
    interface_info
        .parents
        .iter()
        .any(|parent| interface_extends_interface(ctx, parent, target_interface))
}

/// Returns true when a class is or extends the target class.
fn class_extends_class(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    target_class: &str,
) -> bool {
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        if same_php_type_name(&name, target_class) {
            return true;
        }
        current = class_info_by_name(ctx, &name).and_then(|class_info| class_info.parent.clone());
    }
    false
}

/// Finds class metadata by PHP-case-insensitive name.
fn class_info_by_name<'a>(
    ctx: &'a FunctionContext<'_>,
    class_name: &str,
) -> Option<&'a ClassInfo> {
    let wanted = php_symbol_key(class_name.trim_start_matches('\\'));
    ctx.module
        .class_infos
        .iter()
        .find(|(name, _)| php_symbol_key(name.trim_start_matches('\\')) == wanted)
        .map(|(_, info)| info)
}

/// Finds interface metadata by PHP-case-insensitive name.
fn interface_info_by_name<'a>(
    ctx: &'a FunctionContext<'_>,
    interface_name: &str,
) -> Option<&'a InterfaceInfo> {
    let wanted = php_symbol_key(interface_name.trim_start_matches('\\'));
    ctx.module
        .interface_infos
        .iter()
        .find(|(name, _)| php_symbol_key(name.trim_start_matches('\\')) == wanted)
        .map(|(_, info)| info)
}

/// Compares class/interface names using PHP's case-insensitive symbol rules.
fn same_php_type_name(left: &str, right: &str) -> bool {
    php_symbol_key(left.trim_start_matches('\\')) == php_symbol_key(right.trim_start_matches('\\'))
}

/// Returns true when a concrete value can be boxed into Mixed-shaped property storage.
fn can_box_value_for_mixed_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    slot_ty.codegen_repr() == PhpType::Mixed && value_ty.codegen_repr() != PhpType::Mixed
}

/// Returns true when a boxed Mixed/Union value already matches Mixed-shaped property storage.
fn can_store_boxed_value_for_mixed_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    matches!(value_ty.codegen_repr(), PhpType::Mixed)
        && matches!(slot_ty.codegen_repr(), PhpType::Mixed)
}

/// Returns true when a boxed Mixed value can be coerced before a scalar typed-property store.
fn can_coerce_mixed_to_scalar_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && matches!(
            slot_ty.codegen_repr(),
            PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str
        )
}

/// Returns true when a nullable inline scalar can be narrowed into int property storage.
fn can_coerce_tagged_scalar_to_int_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    value_ty.codegen_repr() == PhpType::TaggedScalar && slot_ty.codegen_repr() == PhpType::Int
}

/// Returns true when a class default initializer writes into an untyped property later refined to null.
fn can_store_class_default_in_refined_null_property(
    ctx: &FunctionContext<'_>,
    value_ty: &PhpType,
    slot_ty: &PhpType,
) -> bool {
    if !ctx.function.name.starts_with("_class_propinit_") {
        return false;
    }
    if slot_ty.codegen_repr() != PhpType::Void {
        return false;
    }
    matches!(value_ty.codegen_repr(), PhpType::Int | PhpType::Bool)
}

/// Returns true when an empty array literal initializes a typed array property.
fn is_empty_array_for_array_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    matches!(
        (value_ty, slot_ty),
        (PhpType::Array(elem_ty), PhpType::Array(_))
            if matches!(elem_ty.as_ref(), PhpType::Never | PhpType::Void)
    )
}

/// Returns true when an indexed array can be widened into array<Mixed> storage.
fn can_convert_indexed_array_to_mixed_property(value_ty: &PhpType, slot_ty: &PhpType) -> bool {
    let (PhpType::Array(value_elem), PhpType::Array(slot_elem)) =
        (value_ty.codegen_repr(), slot_ty.codegen_repr())
    else {
        return false;
    };
    slot_elem.codegen_repr() == PhpType::Mixed
        && value_elem.codegen_repr() != PhpType::Mixed
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
    if slot.is_packed {
        return emit_packed_field_load(ctx, slot, base_reg);
    }
    if slot.is_reference {
        return emit_reference_property_load(ctx, slot, base_reg);
    }
    match &slot.php_type {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            if base_reg == ptr_reg {
                abi::emit_load_from_address(ctx.emitter, len_reg, base_reg, slot.offset + 8);
                abi::emit_load_from_address(ctx.emitter, ptr_reg, base_reg, slot.offset);
            } else {
                abi::emit_load_from_address(ctx.emitter, ptr_reg, base_reg, slot.offset);
                abi::emit_load_from_address(ctx.emitter, len_reg, base_reg, slot.offset + 8);
            }
        }
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, float_reg, base_reg, slot.offset);
        }
        PhpType::Bool | PhpType::Int | PhpType::Void | PhpType::Never => {
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

/// Emits a declared reference-property load by dereferencing the stored ref-cell pointer.
fn emit_reference_property_load(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    let pointer_reg = reference_pointer_reg(ctx, base_reg);
    abi::emit_load_from_address(ctx.emitter, pointer_reg, base_reg, slot.offset);
    match slot.php_type.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, ptr_reg, pointer_reg, 0);
            abi::emit_load_from_address(ctx.emitter, len_reg, pointer_reg, 8);
        }
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, float_reg, pointer_reg, 0);
        }
        ty if is_pointer_sized_property_type(&ty)
            || matches!(ty, PhpType::Bool | PhpType::Int | PhpType::Void | PhpType::Never) =>
        {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, int_reg, pointer_reg, 0);
        }
        ty => return Err(CodegenIrError::unsupported(format!(
            "reference property load for PHP type {:?}",
            ty
        ))),
    }
    Ok(())
}

/// Emits a compact packed-field load from a pointer to the containing packed record.
fn emit_packed_field_load(
    ctx: &mut FunctionContext<'_>,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    match &slot.php_type {
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, float_reg, base_reg, slot.offset);
        }
        PhpType::Bool | PhpType::Int | PhpType::Pointer(_) | PhpType::Resource(_) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, int_reg, base_reg, slot.offset);
        }
        PhpType::Packed(_) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            if slot.offset == 0 {
                ctx.emitter.instruction(&format!("mov {}, {}", int_reg, base_reg)); // return the nested packed field address directly
            } else {
                match ctx.emitter.target.arch {
                    Arch::AArch64 => {
                        ctx.emitter.instruction(&format!("add {}, {}, #{}", int_reg, base_reg, slot.offset)); // compute the nested packed field address
                    }
                    Arch::X86_64 => {
                        ctx.emitter.instruction(&format!("lea {}, [{} + {}]", int_reg, base_reg, slot.offset)); // compute the nested packed field address
                    }
                }
            }
        }
        _ => return Err(CodegenIrError::unsupported(format!(
            "packed field load for PHP type {:?}",
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
    if slot.is_packed {
        return emit_packed_field_store(ctx, value, slot, base_reg);
    }
    if slot.is_reference {
        return emit_reference_property_write(ctx, value, slot, base_reg);
    }
    let value_ty = ctx.value_php_type(value)?;
    if is_pointer_sized_property_type(&slot.php_type)
        && is_pointer_slot_null_sentinel(ctx, value, &value_ty)?
    {
        release_previous_property_value(ctx, base_reg, &slot.php_type, slot.offset, None);
        abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset);
        abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        return Ok(());
    }
    match &slot.php_type {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            load_property_store_value_to_result(ctx, value, &slot.php_type)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            release_previous_property_value(
                ctx,
                base_reg,
                &slot.php_type,
                slot.offset,
                Some(&slot.php_type),
            );
            abi::emit_store_to_address(ctx.emitter, ptr_reg, base_reg, slot.offset);
            abi::emit_store_to_address(ctx.emitter, len_reg, base_reg, slot.offset + 8);
        }
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            load_property_store_value_to_result(ctx, value, &slot.php_type)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, float_reg, base_reg, slot.offset);
            abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        }
        PhpType::Bool | PhpType::Int | PhpType::Void | PhpType::Never => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            load_property_store_value_to_result(ctx, value, &slot.php_type)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, int_reg, base_reg, slot.offset);
            abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
        }
        ty if is_pointer_sized_property_type(ty) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            load_property_store_value_to_result(ctx, value, &slot.php_type)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            release_previous_property_value(
                ctx,
                base_reg,
                &slot.php_type,
                slot.offset,
                Some(&slot.php_type),
            );
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

/// Emits a promoted constructor-property bind by storing the parameter ref-cell address.
fn emit_reference_property_bind(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    super::materialize_local_ref_arg_address(ctx, value)?;
    abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), base_reg, slot.offset);
    abi::emit_store_zero_to_address(ctx.emitter, base_reg, slot.offset + 8);
    Ok(())
}

/// Returns true for the constructor-promotion bind pattern `$this->x = $x`.
fn is_promoted_reference_property_bind(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    value: ValueId,
    slot: &PropertySlot,
) -> Result<bool> {
    if !slot.is_reference {
        return Ok(false);
    }
    let Some(object_source) = loaded_local_source(ctx, object)? else {
        return Ok(false);
    };
    if !local_slot_name_is(ctx, object_source.slot, "this") {
        return Ok(false);
    }
    let Some(value_source) = loaded_local_source(ctx, value)? else {
        return Ok(false);
    };
    if !local_slot_name_is(ctx, value_source.slot, &slot.property) {
        return Ok(false);
    }
    Ok(value_source.is_ref_cell && local_slot_is_by_ref_param(ctx, value_source.slot))
}

/// Describes an SSA value that was produced by loading an addressable local slot.
struct LoadedLocalSource {
    slot: LocalSlotId,
    is_ref_cell: bool,
}

/// Resolves a loaded SSA value back to its source local slot when possible.
fn loaded_local_source(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<LoadedLocalSource>> {
    let Some(value_ref) = ctx.function.value(value) else {
        return Err(CodegenIrError::missing_entry("value", value.as_raw()));
    };
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    let is_ref_cell = match inst_ref.op {
        Op::LoadLocal => false,
        Op::LoadRefCell => true,
        _ => return Ok(None),
    };
    let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "loaded local value has no local slot immediate",
        ));
    };
    Ok(Some(LoadedLocalSource { slot, is_ref_cell }))
}

/// Returns true when a local slot has the requested PHP source name.
fn local_slot_name_is(ctx: &FunctionContext<'_>, slot: LocalSlotId, expected: &str) -> bool {
    ctx.function
        .locals
        .get(slot.as_raw() as usize)
        .and_then(|local| local.name.as_deref())
        .is_some_and(|name| name == expected)
}

/// Returns true when a local slot is the storage slot for a by-reference parameter.
fn local_slot_is_by_ref_param(ctx: &FunctionContext<'_>, slot: LocalSlotId) -> bool {
    ctx.function
        .params
        .get(slot.as_raw() as usize)
        .is_some_and(|param| param.by_ref)
}

/// Emits an assignment through a reference property's stored ref-cell pointer.
fn emit_reference_property_write(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    abi::emit_push_reg(ctx.emitter, base_reg);
    load_property_store_value_to_result(ctx, value, &slot.php_type)?;
    abi::emit_pop_reg(ctx.emitter, base_reg);
    let pointer_reg = reference_pointer_reg(ctx, base_reg);
    abi::emit_load_from_address(ctx.emitter, pointer_reg, base_reg, slot.offset);
    release_previous_referenced_value(ctx, pointer_reg, &slot.php_type, Some(&slot.php_type));
    store_current_result_to_reference_cell(ctx, pointer_reg, &slot.php_type)
}

/// Releases the old value held in a reference cell before overwriting it.
fn release_previous_referenced_value(
    ctx: &mut FunctionContext<'_>,
    pointer_reg: &str,
    prop_ty: &PhpType,
    preserve_result_ty: Option<&PhpType>,
) {
    let prop_ty = prop_ty.codegen_repr();
    let releases_value =
        matches!(prop_ty, PhpType::Str | PhpType::Callable) || prop_ty.is_refcounted();
    if !releases_value {
        return;
    }
    if let Some(result_ty) = preserve_result_ty {
        abi::emit_push_result_value(ctx.emitter, &result_ty.codegen_repr());
    }
    abi::emit_push_reg(ctx.emitter, pointer_reg);
    abi::emit_load_from_address(ctx.emitter, abi::int_result_reg(ctx.emitter), pointer_reg, 0);
    match prop_ty {
        PhpType::Str => abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe"),
        PhpType::Callable => callable_descriptor::emit_release_current_descriptor(ctx.emitter),
        ty => abi::emit_decref_if_refcounted(ctx.emitter, &ty),
    }
    abi::emit_pop_reg(ctx.emitter, pointer_reg);
    if let Some(result_ty) = preserve_result_ty {
        restore_property_store_result(ctx, &result_ty.codegen_repr());
    }
}

/// Stores the current result registers into a reference cell.
fn store_current_result_to_reference_cell(
    ctx: &mut FunctionContext<'_>,
    pointer_reg: &str,
    prop_ty: &PhpType,
) -> Result<()> {
    match prop_ty.codegen_repr() {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_store_to_address(ctx.emitter, ptr_reg, pointer_reg, 0);
            abi::emit_store_to_address(ctx.emitter, len_reg, pointer_reg, 8);
        }
        PhpType::Float => {
            abi::emit_store_to_address(ctx.emitter, abi::float_result_reg(ctx.emitter), pointer_reg, 0);
        }
        ty if is_pointer_sized_property_type(&ty)
            || matches!(ty, PhpType::Bool | PhpType::Int | PhpType::Void | PhpType::Never) =>
        {
            abi::emit_store_to_address(ctx.emitter, abi::int_result_reg(ctx.emitter), pointer_reg, 0);
        }
        ty => return Err(CodegenIrError::unsupported(format!(
            "reference property store for PHP type {:?}",
            ty
        ))),
    }
    Ok(())
}

/// Returns a scratch register that can hold a reference-cell pointer beside the object base.
fn reference_pointer_reg(ctx: &FunctionContext<'_>, base_reg: &str) -> &'static str {
    let symbol_reg = abi::symbol_scratch_reg(ctx.emitter);
    if base_reg == symbol_reg {
        abi::secondary_scratch_reg(ctx.emitter)
    } else {
        symbol_reg
    }
}

/// Releases the old value in a declared property slot before overwriting it.
fn release_previous_property_value(
    ctx: &mut FunctionContext<'_>,
    base_reg: &str,
    prop_ty: &PhpType,
    offset: usize,
    preserve_result_ty: Option<&PhpType>,
) {
    let prop_ty = prop_ty.codegen_repr();
    let releases_value =
        matches!(prop_ty, PhpType::Str | PhpType::Callable) || prop_ty.is_refcounted();
    if !releases_value {
        return;
    }
    if let Some(result_ty) = preserve_result_ty {
        abi::emit_push_result_value(ctx.emitter, &result_ty.codegen_repr());
    }
    abi::emit_push_reg(ctx.emitter, base_reg);
    abi::emit_load_from_address(ctx.emitter, abi::int_result_reg(ctx.emitter), base_reg, offset);
    match prop_ty {
        PhpType::Str => abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe"),
        PhpType::Callable => callable_descriptor::emit_release_current_descriptor(ctx.emitter),
        ty => abi::emit_decref_if_refcounted(ctx.emitter, &ty),
    }
    abi::emit_pop_reg(ctx.emitter, base_reg);
    if let Some(result_ty) = preserve_result_ty {
        restore_property_store_result(ctx, &result_ty.codegen_repr());
    }
}

/// Restores a property-store result value saved around previous-slot release.
fn restore_property_store_result(ctx: &mut FunctionContext<'_>, result_ty: &PhpType) {
    match result_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        }
    }
}

/// Loads an SSA value in the shape required by a typed object property store.
fn load_property_store_value_to_result(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    slot_ty: &PhpType,
) -> Result<()> {
    let value_ty = ctx.value_php_type(value)?;
    if can_box_value_for_mixed_property(&value_ty, slot_ty) {
        let loaded_ty = ctx.load_value_to_result(value)?.codegen_repr();
        // Property stores do not consume the SSA source; explicit release ops still
        // own temporary cleanup after `prop_set`.
        emit_box_current_value_as_mixed(ctx.emitter, &loaded_ty);
        return Ok(());
    }
    if can_store_boxed_value_for_mixed_property(&value_ty, slot_ty) {
        ctx.load_value_to_result(value)?;
        if !ctx.value_can_own_mixed_box_source(value)? {
            abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
        }
        return Ok(());
    }
    if can_convert_indexed_array_to_mixed_property(&value_ty, slot_ty) {
        let PhpType::Array(source_elem) = ctx.load_value_to_result(value)?.codegen_repr() else {
            return Err(CodegenIrError::unsupported(format!(
                "property array widening from PHP type {:?}",
                value_ty
            )));
        };
        emit_loaded_indexed_array_to_mixed(ctx, &source_elem.codegen_repr());
        abi::emit_incref_if_refcounted(ctx.emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
        return Ok(());
    }
    if can_coerce_tagged_scalar_to_int_property(&value_ty, slot_ty) {
        ctx.load_value_to_result(value)?;
        crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
        return Ok(());
    }
    if matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        load_value_to_first_int_arg(ctx, value)?;
        match slot_ty.codegen_repr() {
            PhpType::Str => {
                abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
                abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            }
            PhpType::Int => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int"),
            PhpType::Bool => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool"),
            PhpType::Float => abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float"),
            _ => {}
        }
        return Ok(());
    }
    let loaded_ty = ctx.load_value_to_result(value)?;
    if matches!(slot_ty.codegen_repr(), PhpType::Str) {
        abi::emit_call_label(ctx.emitter, "__rt_str_persist");
        return Ok(());
    }
    if slot_ty.codegen_repr().is_refcounted() {
        abi::emit_incref_if_refcounted(ctx.emitter, &loaded_ty.codegen_repr());
    }
    Ok(())
}

/// Emits a compact packed-field store without writing object-property metadata words.
fn emit_packed_field_store(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    slot: &PropertySlot,
    base_reg: &str,
) -> Result<()> {
    match &slot.php_type {
        PhpType::Float => {
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            ctx.load_value_to_reg(value, float_reg)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, float_reg, base_reg, slot.offset);
        }
        PhpType::Bool
        | PhpType::Int
        | PhpType::Void
        | PhpType::Never
        | PhpType::Pointer(_)
        | PhpType::Resource(_) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_push_reg(ctx.emitter, base_reg);
            ctx.load_value_to_reg(value, int_reg)?;
            abi::emit_pop_reg(ctx.emitter, base_reg);
            abi::emit_store_to_address(ctx.emitter, int_reg, base_reg, slot.offset);
        }
        _ => return Err(CodegenIrError::unsupported(format!(
            "packed field store for PHP type {:?}",
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
            | PhpType::Union(_)
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
