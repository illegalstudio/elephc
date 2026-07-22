//! Purpose:
//! Lowers PHP type/reflection builtins for the EIR backend.
//! Handles local retyping, class-name lookup against static metadata, and runtime object class ids.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_language_construct_call()`.
//!
//! Key details:
//! - Dynamic object lookups use the same dense `_class_name_*` runtime tables
//!   emitted for codegen, preserving concrete subclasses.

use crate::codegen::abi;
use crate::codegen::emit_box_current_value_as_mixed;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::names::php_symbol_key;
use crate::types::{ClassInfo, PhpType};

use super::super::super::context::FunctionContext;
use super::super::predicates;
use super::{expect_operand, load_value_to_first_int_arg, store_if_result};

/// Lowers `settype($local, "type")` by mutating the resolved local slot and returning true.
pub(crate) fn lower_settype(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "settype", 2)?;
    let value = expect_operand(inst, 0)?;
    let type_name = expect_operand(inst, 1)?;
    let Some(target_ty) = settype_target_type(&const_string_operand(ctx, type_name)?) else {
        emit_bool_result(ctx, true);
        return store_if_result(ctx, inst);
    };
    let slot = super::super::local_slot_for_loaded_value(ctx, value)?;
    emit_settype_conversion(ctx, value, &target_ty)?;
    store_settype_local_result(ctx, slot, &target_ty)?;
    emit_bool_result(ctx, true);
    store_if_result(ctx, inst)
}

/// Lowers the defensive `class_alias()` fallback that remains after AOT alias extraction.
pub(crate) fn lower_class_alias(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count_between(inst, "class_alias", 2, 3)?;
    emit_bool_result(ctx, false);
    store_if_result(ctx, inst)
}

/// Rejects `unset()` calls that were not converted into direct EIR unbind operations.
pub(super) fn lower_unset_builtin(
    _ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    Err(CodegenIrError::unsupported(format!(
        "unset target shape with {} lowered operands",
        inst.operands.len()
    )))
}

/// Returns the concrete PHP type requested by a supported `settype()` type name.
fn settype_target_type(name: &str) -> Option<PhpType> {
    match php_symbol_key(name).as_str() {
        "int" | "integer" => Some(PhpType::Int),
        "float" | "double" => Some(PhpType::Float),
        "string" => Some(PhpType::Str),
        "bool" | "boolean" => Some(PhpType::Bool),
        _ => None,
    }
}

/// Emits conversion from the current operand type into the requested `settype()` target type.
fn emit_settype_conversion(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    target_ty: &PhpType,
) -> Result<()> {
    match target_ty.codegen_repr() {
        PhpType::Int => emit_settype_int_conversion(ctx, value),
        PhpType::Float => emit_settype_float_conversion(ctx, value),
        PhpType::Str => emit_settype_string_conversion(ctx, value),
        PhpType::Bool => emit_settype_bool_conversion(ctx, value),
        other => Err(CodegenIrError::unsupported(format!(
            "settype target PHP type {:?}",
            other
        ))),
    }
}

/// Emits PHP integer conversion for a `settype(..., "int"|"integer")` mutation.
fn emit_settype_int_conversion(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let raw_ty = ctx.raw_value_php_type(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        ctx.load_value_to_result(value)?;
        emit_resource_display_id_to_int(ctx);
        return Ok(());
    }
    match raw_ty.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_float_result_to_int_result(ctx.emitter);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            super::super::predicates::emit_array_truthiness(ctx, value)?;
        }
        _ => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
    }
    Ok(())
}

/// Emits PHP float conversion for a `settype(..., "float"|"double")` mutation.
fn emit_settype_float_conversion(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let raw_ty = ctx.raw_value_php_type(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        ctx.load_value_to_result(value)?;
        emit_resource_display_id_to_int(ctx);
        abi::emit_int_result_to_float_result(ctx.emitter);
        return Ok(());
    }
    match raw_ty.codegen_repr() {
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_float");
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            super::super::predicates::emit_array_truthiness(ctx, value)?;
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        _ => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
    }
    Ok(())
}

/// Emits PHP string conversion for a `settype(..., "string")` mutation.
fn emit_settype_string_conversion(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let raw_ty = ctx.raw_value_php_type(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_call_label(ctx.emitter, "__rt_resource_to_string");
        return Ok(());
    }
    match raw_ty.codegen_repr() {
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_ftoa");
        }
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
        }
        PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            emit_loaded_bool_to_string(ctx);
        }
        PhpType::Void | PhpType::Never => {
            emit_string_result(ctx, b"");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            emit_string_result(ctx, b"Array");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "settype string conversion from PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Emits PHP boolean conversion for a `settype(..., "bool"|"boolean")` mutation.
fn emit_settype_bool_conversion(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let raw_ty = ctx.raw_value_php_type(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        return Ok(());
    }
    match raw_ty.codegen_repr() {
        PhpType::Bool | PhpType::Int => {
            ctx.load_value_to_result(value)?;
            emit_int_result_nonzero_bool(ctx);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            emit_float_result_nonzero_bool(ctx);
        }
        PhpType::Str => {
            super::super::predicates::emit_string_truthiness(ctx, value)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            super::super::predicates::emit_array_truthiness(ctx, value)?;
        }
        _ => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
    }
    Ok(())
}

/// Stores the converted `settype()` payload into the local slot's storage representation.
fn store_settype_local_result(
    ctx: &mut FunctionContext<'_>,
    slot: LocalSlotId,
    target_ty: &PhpType,
) -> Result<()> {
    let storage_ty = ctx.local_php_type(slot)?.codegen_repr();
    let target_ty = target_ty.codegen_repr();
    if storage_ty == PhpType::Mixed && target_ty != PhpType::Mixed {
        emit_box_current_value_as_mixed(ctx.emitter, &target_ty);
        let offset = ctx.local_offset(slot)?;
        abi::emit_store(ctx.emitter, &PhpType::Mixed, offset);
        return Ok(());
    }
    let offset = ctx.local_offset(slot)?;
    abi::emit_store(ctx.emitter, &target_ty, offset);
    Ok(())
}

/// Converts the loaded boolean payload into PHP string result registers.
fn emit_loaded_bool_to_string(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("settype_bool_string_false");
    let done_label = ctx.next_label("settype_bool_string_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // false stringifies to an empty string
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the empty-string fallback after true conversion
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the boolean payload is false
            ctx.emitter.instruction(&format!("je {}", false_label));            // false stringifies to an empty string
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the empty-string fallback after true conversion
        }
    }
    ctx.emitter.label(&false_label);
    emit_string_result(ctx, b"");
    ctx.emitter.label(&done_label);
}

/// Converts the loaded integer result register into a canonical bool.
fn emit_int_result_nonzero_bool(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // compare the scalar payload against zero for PHP truthiness
            ctx.emitter.instruction("cset x0, ne");                             // normalize non-zero payloads to true
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // compare the scalar payload against zero for PHP truthiness
            ctx.emitter.instruction("setne al");                                // normalize non-zero payloads to true
            ctx.emitter.instruction("movzx rax, al");                           // widen the normalized boolean byte
        }
    }
}

/// Converts the loaded float result register into a canonical bool.
fn emit_float_result_nonzero_bool(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov d1, #0.0");                           // materialize 0.0 for PHP float truthiness
            ctx.emitter.instruction("fcmp d0, d1");                             // compare the float payload against zero
            ctx.emitter.instruction("cset x0, ne");                             // normalize non-zero floats to true
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xorpd xmm1, xmm1");                        // materialize 0.0 for PHP float truthiness
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare the float payload against zero
            ctx.emitter.instruction("setne al");                                // normalize non-zero floats to true
            ctx.emitter.instruction("movzx rax, al");                           // widen the normalized boolean byte
        }
    }
}

/// Converts the loaded resource payload into PHP's one-based integer id.
fn emit_resource_display_id_to_int(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add x0, x0, #1");                          // convert native resource payload to PHP's one-based display id
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rax, 1");                              // convert native resource payload to PHP's one-based display id
        }
    }
}

/// Lowers `get_class()` and `get_parent_class()` through static or dynamic class metadata.
pub(crate) fn lower_class_name_lookup(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::ensure_arg_count_between(inst, name, 0, 1)?;
    if inst.operands.is_empty() {
        emit_no_arg_class_name_lookup(ctx, name);
        return store_if_result(ctx, inst);
    }

    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Object(_) => {
            ctx.load_value_to_result(value)?;
            emit_dynamic_object_class_name(ctx, name);
        }
        PhpType::Mixed | PhpType::Union(_) if super::has_eval_context(ctx) => {
            return super::lower_eval_object_class_name(ctx, inst, value, name);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            emit_mixed_object_class_name(ctx, name);
        }
        PhpType::Str if name == "get_parent_class" => {
            let class_name = const_string_operand(ctx, value)?;
            let parent = parent_of(ctx, &class_name);
            emit_string_result(ctx, parent.as_bytes());
        }
        _ => {
            ctx.load_value_to_result(value)?;
            emit_string_result(ctx, b"");
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `is_a()` and `is_subclass_of()` for object operands and literal targets.
pub(crate) fn lower_is_a_relation(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::ensure_arg_count_between(inst, name, 2, 3)?;
    for value in &inst.operands {
        ctx.load_value_to_result(*value)?;
    }

    let object = expect_operand(inst, 0)?;
    let target = expect_operand(inst, 1)?;
    let exclude_self = name == "is_subclass_of";
    if matches!(ctx.value_php_type(object)?, PhpType::Mixed | PhpType::Union(_))
        && super::has_eval_context(ctx)
    {
        if let Some(target_class) = optional_const_string_operand(ctx, target)? {
            return super::lower_eval_object_is_a(ctx, inst, object, &target_class, exclude_self);
        }
    }
    let result = static_relation_holds(ctx, object, target, exclude_self)?;
    emit_bool_result(ctx, result);
    store_if_result(ctx, inst)
}

/// Lowers `get_declared_classes/interfaces/traits()` using the shared declaration registry.
pub(crate) fn lower_get_declared_names(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 0)?;
    let names = declared_names(ctx, name)?;
    emit_string_array(ctx, &names)?;
    store_if_result(ctx, inst)
}

/// Lowers `is_resource(value)` for static resources and boxed Mixed resource cells.
pub(crate) fn lower_is_resource(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "is_resource", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.raw_value_php_type(value)? {
        PhpType::Resource(_) => emit_bool_result(ctx, true),
        PhpType::Mixed | PhpType::Union(_) => predicates::emit_mixed_tag_eq(ctx, value, 9)?,
        _ => emit_bool_result(ctx, false),
    }
    store_if_result(ctx, inst)
}

/// Lowers `get_resource_type(resource)` to elephc's current resource type label.
pub(crate) fn lower_get_resource_type(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "get_resource_type", 1)?;
    let value = expect_operand(inst, 0)?;
    ctx.load_value_to_result(value)?;
    emit_string_result(ctx, b"stream");
    store_if_result(ctx, inst)
}

/// Lowers `get_resource_id(resource)` by unboxing the native handle and making it one-based.
pub(crate) fn lower_get_resource_id(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "get_resource_id", 1)?;
    let value = expect_operand(inst, 0)?;
    super::io::load_stream_fd_to_result(ctx, value, "get_resource_id")?;
    emit_resource_display_id_to_int(ctx);
    store_if_result(ctx, inst)
}

/// Emits a static no-argument class-name result for the current method scope.
fn emit_no_arg_class_name_lookup(ctx: &mut FunctionContext<'_>, name: &str) {
    let class_name = current_method_class(ctx).unwrap_or_default();
    let result = if name == "get_parent_class" {
        parent_of(ctx, class_name)
    } else {
        class_name.to_string()
    };
    emit_string_result(ctx, result.as_bytes());
}

/// Emits dynamic class-name lookup for an object pointer already loaded in the result register.
fn emit_dynamic_object_class_name(ctx: &mut FunctionContext<'_>, name: &str) {
    let empty_label = ctx.next_label("get_class_empty");
    let done_label = ctx.next_label("get_class_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_dynamic_object_class_name_aarch64(ctx, name, &empty_label, &done_label),
        Arch::X86_64 => emit_dynamic_object_class_name_x86_64(ctx, name, &empty_label, &done_label),
    }
}

/// Emits class-name lookup for a boxed Mixed value that may contain an object.
fn emit_mixed_object_class_name(ctx: &mut FunctionContext<'_>, name: &str) {
    let empty_label = ctx.next_label("get_class_mixed_empty");
    let done_label = ctx.next_label("get_class_mixed_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // require a boxed object payload for class-name lookup
            ctx.emitter
                .instruction(&format!("b.ne {}", empty_label));                 // non-object Mixed payloads produce an empty class name
            ctx.emitter.instruction("mov x0, x1");                              // expose the unboxed object pointer to the object lookup path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // require a boxed object payload for class-name lookup
            ctx.emitter
                .instruction(&format!("jne {}", empty_label));                  // non-object Mixed payloads produce an empty class name
            ctx.emitter.instruction("mov rax, rdi");                            // expose the unboxed object pointer to the object lookup path
        }
    }
    emit_dynamic_object_class_name(ctx, name);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&empty_label);
    emit_string_result(ctx, b"");

    ctx.emitter.label(&done_label);
}

/// Emits AArch64 runtime object class-name lookup for `get_class()` and `get_parent_class()`.
fn emit_dynamic_object_class_name_aarch64(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    empty_label: &str,
    done_label: &str,
) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.emitter.instruction(&format!("cbz x0, {}", empty_label));               // null object pointers produce an empty class name
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load the object's concrete runtime class id
    abi::emit_symbol_address(ctx.emitter, "x10", "_class_name_count");
    ctx.emitter.instruction("ldr x10, [x10]");                                  // load the number of dense class-name lookup rows
    if name == "get_parent_class" {
        ctx.emitter.instruction("cmp x9, x10");                                 // validate the object class id before reading its parent id
        ctx.emitter.instruction(&format!("b.hs {}", empty_label));              // reject unknown object class ids as parentless
        abi::emit_symbol_address(ctx.emitter, "x11", "_class_parent_ids");
        ctx.emitter.instruction("lsl x12, x9, #3");                             // scale the class id to a parent-id table byte offset
        ctx.emitter.instruction("ldr x9, [x11, x12]");                          // replace the class id with its parent class id
        ctx.emitter.instruction("mov x13, #-1");                                // materialize the parentless class sentinel
        ctx.emitter.instruction("cmp x9, x13");                                 // check whether the runtime class has no parent
        ctx.emitter.instruction(&format!("b.eq {}", empty_label));              // parentless runtime classes produce an empty string
    }
    ctx.emitter.instruction("cmp x9, x10");                                     // validate the class id before indexing class-name metadata
    ctx.emitter.instruction(&format!("b.hs {}", empty_label));                  // invalid class ids produce an empty class name
    abi::emit_symbol_address(ctx.emitter, "x11", "_class_name_entries");
    ctx.emitter.instruction("lsl x12, x9, #4");                                 // scale the class id by the 16-byte class-name row size
    ctx.emitter.instruction("add x11, x11, x12");                               // point at the selected class-name metadata row
    ctx.emitter.instruction(&format!("ldr {}, [x11]", ptr_reg));                // load the concrete class-name string pointer
    ctx.emitter.instruction(&format!("ldr {}, [x11, #8]", len_reg));            // load the concrete class-name string length
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the empty-string fallback after a successful lookup

    ctx.emitter.label(empty_label);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, "_class_name_missing");
    abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);

    ctx.emitter.label(done_label);
}

/// Emits x86_64 runtime object class-name lookup for `get_class()` and `get_parent_class()`.
fn emit_dynamic_object_class_name_x86_64(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    empty_label: &str,
    done_label: &str,
) {
    ctx.emitter.instruction("test rax, rax");                                   // test whether the object pointer is null
    ctx.emitter.instruction(&format!("je {}", empty_label));                    // null object pointers produce an empty class name
    ctx.emitter.instruction("mov r8, QWORD PTR [rax]");                         // load the object's concrete runtime class id
    ctx.emitter.instruction("mov r9, QWORD PTR [rip + _class_name_count]");     // load the number of dense class-name lookup rows
    if name == "get_parent_class" {
        ctx.emitter.instruction("cmp r8, r9");                                  // validate the object class id before reading its parent id
        ctx.emitter.instruction(&format!("jae {}", empty_label));               // reject unknown object class ids as parentless
        ctx.emitter.instruction("lea r10, [rip + _class_parent_ids]");          // materialize the runtime parent-id table base pointer
        ctx.emitter.instruction("mov r8, QWORD PTR [r10 + r8 * 8]");            // replace the class id with its parent class id
        ctx.emitter.instruction("cmp r8, -1");                                  // check whether the runtime class has no parent
        ctx.emitter.instruction(&format!("je {}", empty_label));                // parentless runtime classes produce an empty string
    }
    ctx.emitter.instruction("cmp r8, r9");                                      // validate the class id before indexing class-name metadata
    ctx.emitter.instruction(&format!("jae {}", empty_label));                   // invalid class ids produce an empty class name
    ctx.emitter.instruction("lea r10, [rip + _class_name_entries]");            // materialize the class-name metadata table base pointer
    ctx.emitter.instruction("shl r8, 4");                                       // scale the class id by the 16-byte class-name row size
    ctx.emitter.instruction("mov rax, QWORD PTR [r10 + r8]");                   // load the concrete class-name string pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [r10 + r8 + 8]");               // load the concrete class-name string length
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the empty-string fallback after a successful lookup

    ctx.emitter.label(empty_label);
    ctx.emitter.instruction("lea rax, [rip + _class_name_missing]");            // return the shared empty class-name string pointer
    ctx.emitter.instruction("xor edx, edx");                                    // return zero bytes for the empty class name

    ctx.emitter.label(done_label);
}

/// Emits `bytes` as the current string result register pair.
fn emit_string_result(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Emits `value` as the current boolean result.
fn emit_bool_result(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
}

/// Statically evaluates an object/class relation against a literal target class name.
fn static_relation_holds(
    ctx: &FunctionContext<'_>,
    object: ValueId,
    target: ValueId,
    exclude_self: bool,
) -> Result<bool> {
    let PhpType::Object(object_class) = ctx.value_php_type(object)? else {
        return Ok(false);
    };
    let Some(target_class) = optional_const_string_operand(ctx, target)? else {
        return Ok(false);
    };
    let object_class = object_class.trim_start_matches('\\');
    let target_class = target_class.trim_start_matches('\\');
    let target_key = php_symbol_key(target_class);
    if !exclude_self && php_symbol_key(object_class) == target_key {
        return Ok(true);
    }
    if parent_chain_contains(ctx, object_class, &target_key) {
        return Ok(true);
    }
    Ok(class_interfaces_contain(ctx, object_class, &target_key))
}

/// Returns true when an object's parent chain contains the target PHP symbol key.
fn parent_chain_contains(
    ctx: &FunctionContext<'_>,
    object_class: &str,
    target_key: &str,
) -> bool {
    let mut current = object_class.to_string();
    while let Some(info) = lookup_class(ctx, &current) {
        let Some(parent) = &info.parent else {
            return false;
        };
        let parent = parent.trim_start_matches('\\');
        if php_symbol_key(parent) == target_key {
            return true;
        }
        current = parent.to_string();
    }
    false
}

/// Returns true when an object's implemented interface set contains the target PHP symbol key.
fn class_interfaces_contain(
    ctx: &FunctionContext<'_>,
    object_class: &str,
    target_key: &str,
) -> bool {
    lookup_class(ctx, object_class).is_some_and(|info| {
        info.interfaces
            .iter()
            .any(|name| php_symbol_key(name.trim_start_matches('\\')) == target_key)
    })
}

/// Returns declaration names from EIR order metadata, falling back to legacy registries.
fn declared_names(ctx: &FunctionContext<'_>, name: &str) -> Result<Vec<String>> {
    let mut names = match name {
        "get_declared_classes" => ctx.module.declared_class_names.clone(),
        "get_declared_interfaces" => ctx.module.declared_interface_names.clone(),
        "get_declared_traits" => ctx.module.declared_trait_names.clone(),
        _ => {
            return Err(CodegenIrError::unsupported(format!(
                "declared-name builtin {}",
                name
            )));
        }
    };
    if names.is_empty() {
        names = match name {
            "get_declared_classes" => crate::codegen::declared_class_names(),
            "get_declared_interfaces" => crate::codegen::declared_interface_names(),
            "get_declared_traits" => crate::codegen::declared_trait_names(),
            _ => unreachable!(),
        };
    }
    if names.is_empty() {
        names = match name {
            "get_declared_classes" => ctx
                .module
                .class_table
                .names
                .iter()
                .filter(|name| !super::is_internal_synthetic_class_name(name))
                .cloned()
                .collect(),
            "get_declared_interfaces" => ctx.module.interface_table.names.clone(),
            "get_declared_traits" => ctx.module.trait_table.names.clone(),
            _ => unreachable!(),
        };
    }
    Ok(names)
}

/// Allocates an indexed string array and appends every declaration name.
fn emit_string_array(ctx: &mut FunctionContext<'_>, names: &[String]) -> Result<()> {
    let capacity = names.len().max(1);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", 16);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", 16);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    if names.is_empty() {
        return Ok(());
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_string_array_fill_aarch64(ctx, names),
        Arch::X86_64 => emit_string_array_fill_x86_64(ctx, names),
    }
    Ok(())
}

/// Appends declaration names to the current result array on AArch64.
fn emit_string_array_fill_aarch64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("str x0, [sp, #-16]!");                             // park the declared-name array while appending names
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("ldr x0, [sp]");                                // reload the declared-name array for this append
        abi::emit_symbol_address(ctx.emitter, "x1", &label);
        abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("str x0, [sp]");                                // preserve the possibly-grown declared-name array
    }
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // restore the final declared-name array as the result
}

/// Appends declaration names to the current result array on x86_64.
fn emit_string_array_fill_x86_64(ctx: &mut FunctionContext<'_>, names: &[String]) {
    ctx.emitter.instruction("push rax");                                        // park the declared-name array while appending names
    ctx.emitter.instruction("sub rsp, 8");                                      // keep stack alignment stable across append helper calls
    for name in names {
        let (label, len) = ctx.data.add_string(name.as_bytes());
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // reload the declared-name array for this append
        abi::emit_symbol_address(ctx.emitter, "rsi", &label);
        abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                // preserve the possibly-grown declared-name array
    }
    ctx.emitter.instruction("add rsp, 8");                                      // drop the temporary alignment slot
    ctx.emitter.instruction("pop rax");                                         // restore the final declared-name array as the result
}

/// Looks up a class by PHP-style case-insensitive name.
fn lookup_class<'a>(ctx: &'a FunctionContext<'_>, name: &str) -> Option<&'a ClassInfo> {
    let clean = name.trim_start_matches('\\');
    let key = php_symbol_key(clean);
    ctx.module
        .class_infos
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == key)
        .map(|(_, info)| info)
}

/// Returns the lexical class name encoded in an EIR method function name.
fn current_method_class<'a>(ctx: &'a FunctionContext<'_>) -> Option<&'a str> {
    ctx.function
        .name
        .rsplit_once("::")
        .map(|(class_name, _)| class_name)
}

/// Returns the parent class name for a known class, or an empty string when unavailable.
fn parent_of(ctx: &FunctionContext<'_>, class_name: &str) -> String {
    if class_name.is_empty() {
        return String::new();
    }
    ctx.module
        .class_infos
        .get(class_name.trim_start_matches('\\'))
        .and_then(|info| info.parent.clone())
        .unwrap_or_default()
}

/// Returns a string literal value defined by a `ConstStr` operand.
fn const_string_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<String> {
    optional_const_string_operand(ctx, value)?.ok_or_else(|| {
        CodegenIrError::unsupported("get_parent_class with non-literal class name")
    })
}

/// Returns a `ConstStr` operand value, or `None` when the operand is not a literal string.
fn optional_const_string_operand(
    ctx: &FunctionContext<'_>,
    value: ValueId,
) -> Result<Option<String>> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "string literal operand has no data id",
        ));
    };
    Ok(Some(ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?))
}
