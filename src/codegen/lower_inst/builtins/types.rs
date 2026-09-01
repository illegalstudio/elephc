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
use crate::codegen::emit::Emitter;
use crate::codegen::emit_box_current_value_as_mixed;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, LocalSlotId, Op, ValueDef, ValueId};
use crate::names::php_symbol_key;
use crate::types::{ClassInfo, PhpType};

use super::super::super::context::FunctionContext;
use super::super::predicates;
use super::{expect_operand, load_value_to_first_int_arg, store_if_result};

/// Lowers `intval($value, $base)`, PHP's two-argument integer conversion.
///
/// Reference PHP honors `$base` only when `$value` is a string, so the subject's checker type
/// picks the path: a known string goes straight to `__rt_str_to_int_base`, a boxed `Mixed`
/// goes to `__rt_mixed_intval_base` (which repeats that test at run time against the cell's
/// tag), and every other scalar keeps the ordinary integer cast with the base discarded —
/// `intval(42.9, 8) === 42`, not `34`.
pub(crate) fn lower_intval_base(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count(inst, "intval", 2)?;
    let value = expect_operand(inst, 0)?;
    let base = expect_operand(inst, 1)?;
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Str => lower_intval_base_from_string(ctx, value, base)?,
        PhpType::Mixed => lower_intval_base_from_mixed(ctx, value, base)?,
        _ => super::strings::load_as_int(ctx, value, "intval")?,
    }
    store_if_result(ctx, inst)
}

/// Materializes a known-string `intval()` subject and parses it in the requested base.
///
/// The subject is staged first because materializing `$base` may itself need the result
/// register, and the string pair is restored only after the base has reached its own
/// argument register.
fn lower_intval_base_from_string(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    base: ValueId,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            super::strings::load_value_as_string_to_regs(ctx, value, "intval", "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the subject string while the base is materialized
            super::strings::load_as_int(ctx, base, "intval base")?;
            ctx.emitter.instruction("mov x3, x0");                              // pass the requested base as the parser's third argument
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the subject into the parser's string argument pair
        }
        Arch::X86_64 => {
            super::strings::load_value_as_string_to_regs(ctx, value, "intval", "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            super::strings::load_as_int(ctx, base, "intval base")?;
            ctx.emitter.instruction("mov r8, rax");                             // park the requested base while the subject is restored
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");                  // restore the subject into the parser's SysV string arguments
            ctx.emitter.instruction("mov rdx, r8");                             // pass the requested base as the parser's third argument
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_to_int_base");
    Ok(())
}

/// Materializes a boxed `Mixed` `intval()` subject and defers the string test to run time.
///
/// The cell pointer stays in the canonical integer result register, which is exactly where
/// `__rt_mixed_intval_base` and the `__rt_mixed_cast_int` it falls back to expect it.
fn lower_intval_base_from_mixed(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    base: ValueId,
) -> Result<()> {
    let cell_reg = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_result(value)?;
    abi::emit_push_reg(ctx.emitter, cell_reg);
    super::strings::load_as_int(ctx, base, "intval base")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the requested base as the helper's second argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the requested base as the helper's second argument
        }
    }
    abi::emit_pop_reg(ctx.emitter, cell_reg);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_intval_base");
    Ok(())
}

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
///
/// Reaching this lowering means `crate::ir_lower::expr` could not turn the target
/// into a slot clear, a hash/array removal, an `offsetUnset()` call, a `__unset()`
/// call or a dynamic-property removal, so the message lists the shapes that do lower
/// directly and then names the one shape users hit most.
///
/// THE UNTYPED FIXED SLOT is that shape. `unset($obj->untypedProp)` on a property
/// declared without a type (`public $foo = 1;`) truly REMOVES it in PHP: a later read
/// warns `Undefined property` and answers `null`, and a later write recreates it.
/// elephc gives each declared property a fixed, monomorphically typed slot, so a
/// property the checker typed `Int` has no encoding for "removed and reading as null"
/// — every candidate encoding answers `int(0)` or a raw marker word instead. A loud
/// error beats a wrong value, so the shape is refused here. Untyped properties whose
/// storage is a DYNAMIC hash (`stdClass`, undeclared names on
/// `#[AllowDynamicProperties]` classes) are genuinely removable and lower fine.
pub(super) fn lower_unset_builtin(
    _ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    Err(CodegenIrError::unsupported(format!(
        "unset target shape with {} lowered operands (supported: variables, \
         array/hash elements, ArrayAccess offsets, __unset()-backed properties, \
         declared typed object properties, and dynamic object properties). \
         An UNTYPED declared property (`public $p = 1;`) is not supported: its fixed \
         slot has no representation for PHP's removed-then-null read",
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
///
/// This is `settype($x, "bool")`'s own copy of float truthiness; it must agree with
/// `predicates::emit_float_result_nonzero_bool` on every value, NAN included — PHP settypes a
/// NAN to `true`. See that function for why the x86_64 arm needs a parity fixup and the
/// AArch64 arm does not.
fn emit_float_result_nonzero_bool(ctx: &mut FunctionContext<'_>) {
    super::super::predicates::emit_nan_bool_coercion_probe(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov d1, #0.0");                           // materialize 0.0 for PHP float truthiness
            ctx.emitter.instruction("fcmp d0, d1");                             // compare the float payload against zero
            ctx.emitter.instruction("cset x0, ne");                             // normalize non-zero floats to true
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xorpd xmm1, xmm1");                        // materialize 0.0 for PHP float truthiness
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare the float payload against zero
            ctx.emitter.instruction("setne al");                                // normalize ordered non-zero floats to true
            ctx.emitter.instruction("setp r10b");                               // materialize whether the comparison was unordered (a NAN)
            ctx.emitter.instruction("or al, r10b");                             // PHP settypes a NAN to true, so merge the unordered case in
            ctx.emitter.instruction("movzx rax, al");                           // widen the normalized boolean byte
        }
    }
}

/// Converts the loaded resource payload into PHP's resource id.
///
/// Answers from the resource-id registry (`runtime::resource_ids`) rather than
/// from the payload itself; see the twin helper in `lower_inst::conversions` for
/// why `payload + 1` was not a numbering scheme.
fn emit_resource_display_id_to_int(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_resource_id_of");
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

/// Lowers `get_object_vars()` to a fresh public-property hash projection.
pub(crate) fn lower_get_object_vars(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "get_object_vars", 1)?;
    let value = expect_operand(inst, 0)?;
    let scope_class_id = lexical_object_vars_scope_class_id(ctx);
    emit_object_hash_projection(ctx, value, false, scope_class_id)?;
    store_if_result(ctx, inst)
}

/// Lowers an explicit object-to-array cast to a fresh all-property hash projection.
pub(crate) fn lower_object_array_cast(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    emit_object_hash_projection(ctx, value, true, -1)?;
    store_if_result(ctx, inst)
}

/// Materializes an object pointer and invokes the shared target-aware projection helper.
fn emit_object_hash_projection(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    cast_mode: bool,
    scope_class_id: i64,
) -> Result<()> {
    let source_ty = ctx.raw_value_php_type(value)?.codegen_repr();
    ctx.load_value_to_result(value)?;
    match source_ty {
        PhpType::Object(_) => {}
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    let object_label = ctx.next_label("object_hash_mixed_object");
                    ctx.emitter.instruction("cmp x0, #6");                      // require the boxed Mixed object tag
                    ctx.emitter
                        .instruction(&format!("b.eq {}", object_label));         // branch to the object payload extraction path
                    emit_dynamic_non_object_projection_fatal(ctx, cast_mode);
                    ctx.emitter.label(&object_label);
                    ctx.emitter.instruction("mov x0, x1");                      // move the unboxed object pointer into the helper input
                }
                Arch::X86_64 => {
                    let object_label = ctx.next_label("object_hash_mixed_object_x");
                    ctx.emitter.instruction("cmp rax, 6");                      // require the boxed Mixed object tag
                    ctx.emitter
                        .instruction(&format!("je {}", object_label));           // branch to the object payload extraction path
                    emit_dynamic_non_object_projection_fatal(ctx, cast_mode);
                    ctx.emitter.label(&object_label);
                    ctx.emitter.instruction("mov rax, rdi");                    // move the unboxed object pointer into the helper input
                }
            }
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "object property projection from PHP type {:?}",
                other
            )))
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x1", i64::from(cast_mode));
            abi::emit_load_int_immediate(ctx.emitter, "x2", scope_class_id);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdx", i64::from(cast_mode));
            abi::emit_load_int_immediate(ctx.emitter, "rcx", scope_class_id);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_object_to_hash");
    Ok(())
}

/// Resolves the EIR function's lexical class id, using `-1` for global scopes.
fn lexical_object_vars_scope_class_id(ctx: &FunctionContext<'_>) -> i64 {
    let Some(class_name) = ctx.function.lexical_class.as_deref() else {
        return -1;
    };
    ctx.module
        .class_infos
        .get(class_name)
        .map_or(-1, |class| class.class_id as i64)
}

/// Emits an explicit fatal instead of silently projecting a runtime non-object as empty.
fn emit_dynamic_non_object_projection_fatal(ctx: &mut FunctionContext<'_>, cast_mode: bool) {
    let message = if cast_mode {
        b"Fatal error: elephc cannot cast this runtime-typed non-object value to array\n".as_slice()
    } else {
        b"Fatal error: get_object_vars(): Argument #1 ($object) must be of type object\n".as_slice()
    };
    let (label, len) = ctx.data.add_string(message);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // select stderr for the dynamic projection fatal diagnostic
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);       // pass the exact dynamic projection diagnostic byte length
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // select stderr for the dynamic projection fatal diagnostic
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);      // pass the exact dynamic projection diagnostic byte length
            ctx.emitter.instruction("mov eax, 1");                              // select the Linux write syscall for the diagnostic
            ctx.emitter.instruction("syscall");                                 // write the dynamic projection diagnostic before exiting
            abi::emit_exit(ctx.emitter, 1);
        }
    }
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
    if ctx.value_php_type(object)?.codegen_repr() == PhpType::Str {
        return lower_class_string_relation(ctx, inst, object, target, exclude_self);
    }
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

/// Lowers class-string `is_a()`/`is_subclass_of()` through runtime class metadata.
fn lower_class_string_relation(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    source: ValueId,
    target: ValueId,
    exclude_self: bool,
) -> Result<()> {
    let early_false = ctx.next_label("class_string_relation_false");
    let stacked_false = ctx.next_label("class_string_relation_stacked_false");
    let source_match = ctx.next_label("class_string_relation_source_match");
    let done = ctx.next_label("class_string_relation_done");
    let allow_string = inst.operands.get(2).copied();
    let dynamic_allow_string = match allow_string {
        Some(flag) => match const_bool_operand(ctx, flag)? {
            Some(false) => {
                emit_bool_result(ctx, false);
                return store_if_result(ctx, inst);
            }
            Some(true) => None,
            None => Some(flag),
        },
        None if !exclude_self => {
            emit_bool_result(ctx, false);
            return store_if_result(ctx, inst);
        }
        None => None,
    };
    if let (Some(source_class), Some(target_class)) = (
        optional_const_string_operand(ctx, source)?,
        optional_const_string_operand(ctx, target)?,
    ) {
        let result =
            static_class_string_relation_holds(ctx, &source_class, &target_class, exclude_self);
        if !result {
            emit_bool_result(ctx, false);
        } else if let Some(flag) = dynamic_allow_string {
            ctx.load_value_to_result(flag)?;
        } else {
            emit_bool_result(ctx, true);
        }
        return store_if_result(ctx, inst);
    }
    if let Some(flag) = dynamic_allow_string {
        ctx.load_value_to_result(flag)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #0");                          // test the runtime allow-string flag
                ctx.emitter.instruction(&format!("b.eq {}", early_false));      // reject class strings when allow_string is false
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test rax, rax");                       // test the runtime allow-string flag
                ctx.emitter.instruction(&format!("jz {}", early_false));        // reject class strings when allow_string is false
            }
        }
    }

    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(target, "x1", "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_instanceof_lookup");
            ctx.emitter.instruction("cmp x0, #0");                              // did the target class or interface resolve?
            ctx.emitter.instruction(&format!("b.eq {}", early_false));          // unresolved targets cannot match a class string
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            ctx.load_string_value_to_regs(source, "x1", "x2")?;
            abi::emit_call_label(ctx.emitter, "__rt_instanceof_lookup");
            ctx.emitter.instruction("cmp x0, #0");                              // did the source class string resolve?
            ctx.emitter.instruction(&format!("b.eq {}", stacked_false));        // unresolved source strings cannot satisfy the relation
            ctx.emitter.instruction("cmp x2, #0");                              // did the source resolve to a concrete class?
            ctx.emitter.instruction(&format!("b.ne {}", stacked_false));        // interface source strings need interface-parent metadata
            if exclude_self {
                ctx.emitter.instruction("ldr x9, [sp]");                        // reload the target class or interface id
                ctx.emitter.instruction("ldr x10, [sp, #8]");                   // reload the target kind
                ctx.emitter.instruction(&format!("cbnz x10, {}", source_match)); // interface targets cannot be exact concrete-class matches
                ctx.emitter.instruction("cmp x1, x9");                          // compare source and target concrete class ids
                ctx.emitter.instruction(&format!("b.eq {}", stacked_false));    // is_subclass_of excludes exact class-string matches
            }
            ctx.emitter.label(&source_match);
            abi::emit_push_reg_pair(ctx.emitter, "x1", "xzr");
            ctx.emitter.instruction("mov x0, sp");                              // pass a fake object header containing the source class id
            ctx.emitter.instruction("ldr x1, [sp, #16]");                       // pass the target metadata id
            ctx.emitter.instruction("ldr x2, [sp, #24]");                       // pass the target metadata kind
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction(&format!("b {}", done));                    // keep the matcher result after balanced cleanup
            ctx.emitter.label(&stacked_false);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_bool_result(ctx, false);
            ctx.emitter.instruction(&format!("b {}", done));                    // join the unresolved-source false path
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(target, "rax", "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_instanceof_lookup");
            ctx.emitter.instruction("test rax, rax");                           // did the target class or interface resolve?
            ctx.emitter.instruction(&format!("jz {}", early_false));            // unresolved targets cannot match a class string
            abi::emit_push_reg_pair(ctx.emitter, "rdi", "rdx");
            ctx.load_string_value_to_regs(source, "rax", "rdx")?;
            abi::emit_call_label(ctx.emitter, "__rt_instanceof_lookup");
            ctx.emitter.instruction("test rax, rax");                           // did the source class string resolve?
            ctx.emitter.instruction(&format!("jz {}", stacked_false));          // unresolved source strings cannot satisfy the relation
            ctx.emitter.instruction("test rdx, rdx");                           // did the source resolve to a concrete class?
            ctx.emitter.instruction(&format!("jnz {}", stacked_false));         // interface source strings need interface-parent metadata
            if exclude_self {
                ctx.emitter.instruction("cmp QWORD PTR [rsp + 8], 0");          // is the target a concrete class rather than an interface?
                ctx.emitter.instruction(&format!("jne {}", source_match));      // interface targets cannot be exact concrete-class matches
                ctx.emitter.instruction("cmp rdi, QWORD PTR [rsp]");            // compare source and target concrete class ids
                ctx.emitter.instruction(&format!("je {}", stacked_false));      // is_subclass_of excludes exact class-string matches
            }
            ctx.emitter.label(&source_match);
            ctx.emitter.instruction("xor r8d, r8d");                            // clear padding beside the fake object class id
            abi::emit_push_reg_pair(ctx.emitter, "rdi", "r8");
            ctx.emitter.instruction("mov rdi, rsp");                            // pass a fake object header containing the source class id
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");           // pass the target metadata id
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");           // pass the target metadata kind
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // keep the matcher result after balanced cleanup
            ctx.emitter.label(&stacked_false);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            emit_bool_result(ctx, false);
            ctx.emitter.instruction(&format!("jmp {}", done));                  // join the unresolved-source false path
        }
    }
    ctx.emitter.label(&early_false);
    emit_bool_result(ctx, false);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Evaluates a relation between two literal class strings from checked class metadata.
fn static_class_string_relation_holds(
    ctx: &FunctionContext<'_>,
    source_class: &str,
    target_class: &str,
    exclude_self: bool,
) -> bool {
    let source_class = source_class.trim_start_matches('\\');
    let target_key = php_symbol_key(target_class.trim_start_matches('\\'));
    if !exclude_self && php_symbol_key(source_class) == target_key {
        return true;
    }
    parent_chain_contains(ctx, source_class, &target_key)
        || class_interfaces_contain(ctx, source_class, &target_key)
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

/// Lowers `get_loaded_extensions($zend_extensions)` as a string array.
///
/// The optional flag selects between the regular extension list (default / `false`) and the Zend
/// extension list (`true`). BOTH lists are known at compile time, so a literal flag bakes exactly
/// one of them into the emitted array; a dynamic flag emits both behind a runtime branch (see
/// [`lower_dynamic_get_loaded_extensions`]) rather than failing the compile.
///
/// The regular (non-Zend) list is the always-present core set followed by the canonical names of
/// the bridges actually linked into this compilation (`crate::codegen::linked_extensions()`, e.g.
/// `PDO`/`hash`), de-duplicated case-insensitively. The Zend list is unaffected: bridges are
/// ordinary (non-Zend) extensions.
pub(crate) fn lower_get_loaded_extensions(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count_between(inst, "get_loaded_extensions", 0, 1)?;
    let flag = inst.operands.first().copied();
    let constant_flag = match flag {
        Some(value) => const_bool_operand(ctx, value)?,
        None => Some(false),
    };
    match (constant_flag, flag) {
        (Some(zend_extensions), _) => {
            emit_string_array(ctx, &loaded_extension_names(zend_extensions))?
        }
        (None, Some(value)) => lower_dynamic_get_loaded_extensions(ctx, value)?,
        (None, None) => unreachable!("a missing flag always folds to false"),
    }
    store_if_result(ctx, inst)
}

/// Returns the extension-name list `get_loaded_extensions($zend_extensions)` reports.
///
/// Single source of truth for the const-folded and the runtime-selected forms, so they can never
/// report different sets.
fn loaded_extension_names(zend_extensions: bool) -> Vec<String> {
    if zend_extensions {
        return super::ZEND_LOADED_EXTENSIONS
            .iter()
            .map(|name| (*name).to_string())
            .collect();
    }
    let mut names: Vec<String> = super::CORE_LOADED_EXTENSIONS
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    for extension in crate::codegen::linked_extensions() {
        if !names.iter().any(|name| name.eq_ignore_ascii_case(&extension)) {
            names.push(extension);
        }
    }
    names
}

/// Lowers `get_loaded_extensions($flag)` for a flag that is only known at runtime.
///
/// Both candidate lists are compile-time constants, so the emitted code just picks between two
/// fully baked arrays with a PHP-truthiness test on the flag. Each branch runs the same
/// [`emit_string_array`] sequence and leaves an `Array<Str>` pointer in the integer result
/// register, so the two arms agree in shape as well as in type — nothing observes a different
/// representation depending on which branch ran.
fn lower_dynamic_get_loaded_extensions(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    let flag_type = ctx.value_php_type(value)?.codegen_repr();
    if !matches!(flag_type, PhpType::Bool | PhpType::False | PhpType::Int) {
        return Err(CodegenIrError::unsupported(format!(
            "get_loaded_extensions with a {:?} flag argument",
            flag_type
        )));
    }
    ctx.load_value_to_result(value)?;
    let zend_label = ctx.next_label("get_loaded_extensions_zend");
    let done_label = ctx.next_label("get_loaded_extensions_done");
    let flag_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", flag_reg));          // did the caller ask for the Zend extension list?
            ctx.emitter.instruction(&format!("b.ne {}", zend_label));           // a truthy flag selects the Zend list
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("test {}, {}", flag_reg, flag_reg));      // did the caller ask for the Zend extension list?
            ctx.emitter.instruction(&format!("jne {}", zend_label));            // a truthy flag selects the Zend list
        }
    }
    emit_string_array(ctx, &loaded_extension_names(false))?;
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&zend_label);
    emit_string_array(ctx, &loaded_extension_names(true))?;

    ctx.emitter.label(&done_label);
    Ok(())
}

/// Reads a literal boolean operand produced by a constant instruction, or `None` when non-literal.
///
/// Accepts `ConstBool`, integer, float, null, and string const instructions using PHP truthiness so
/// any literal the frontend folds into the flag operand resolves at compile time.
fn const_bool_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<bool>> {
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
    match (inst_ref.op, inst_ref.immediate.as_ref()) {
        (Op::ConstBool, Some(Immediate::Bool(value))) => Ok(Some(*value)),
        (Op::ConstI64, Some(Immediate::I64(value))) => Ok(Some(*value != 0)),
        (Op::ConstF64, Some(Immediate::F64(value))) => Ok(Some(*value != 0.0)),
        (Op::ConstNull, _) => Ok(Some(false)),
        (Op::ConstStr, Some(Immediate::Data(data))) => {
            let value = ctx
                .module
                .data
                .strings
                .get(data.as_raw() as usize)
                .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
            Ok(Some(!value.is_empty() && value != "0"))
        }
        _ => Ok(None),
    }
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
///
/// The label is resolved at RUNTIME through `__rt_resource_type_name`, not baked in as
/// a literal: PHP 8.5.6 renames a closed resource to `"Unknown"` — measured identical
/// for `fclose`, `pclose` and `closedir` — and the close state is carried by the sign
/// bit of the native payload (see `crate::codegen_support::runtime::resource_type_name`).
///
/// The operand is deliberately NOT routed through `super::io::load_stream_fd_to_result`.
/// That helper refuses a statically non-resource argument with
/// `CodegenIrError::unsupported`, which would turn `get_resource_type(5)` — a program
/// elephc compiles today — into a compile refusal. elephc over-accepting that call is a
/// real but SEPARATE debt (PHP throws a `TypeError`); closing it here would silently
/// change the accepted language. The `other` arm below therefore keeps answering
/// exactly what it answers today.
pub(crate) fn lower_get_resource_type(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "get_resource_type", 1)?;
    let value = expect_operand(inst, 0)?;
    ctx.load_value_to_result(value)?;
    match resource_type_name_shape(&ctx.raw_value_php_type(value)?) {
        ResourceTypeNameShape::Boxed => emit_boxed_resource_type_name(ctx),
        ResourceTypeNameShape::Unboxed => {
            abi::emit_call_label(ctx.emitter, "__rt_resource_type_name");
        }
        ResourceTypeNameShape::Constant => emit_string_result(ctx, b"stream"),
    }
    store_if_result(ctx, inst)
}

/// How `get_resource_type()` must reach its operand's payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceTypeNameShape {
    /// The operand is a Mixed/Union box: unbox, gate on the resource tag, then resolve.
    Boxed,
    /// The operand is an unboxed `Resource`: its payload is already in the result register.
    Unboxed,
    /// The operand cannot be a resource: keep the constant this builtin always answered.
    Constant,
}

/// Maps a `get_resource_type()` operand's static PHP type to its lowering shape.
///
/// Split out of the lowering so the DECISION is testable without a `FunctionContext`.
/// The `Constant` arm is what preserves today's acceptance: elephc compiles
/// `get_resource_type(5)` where PHP throws a `TypeError`, and turning that into a compile
/// refusal — which routing through `super::io::load_stream_fd_to_result` would do — would
/// change the accepted language in a change nobody reviewed for it.
fn resource_type_name_shape(raw_ty: &PhpType) -> ResourceTypeNameShape {
    match raw_ty {
        PhpType::Mixed | PhpType::Union(_) => ResourceTypeNameShape::Boxed,
        PhpType::Resource(_) => ResourceTypeNameShape::Unboxed,
        _ => ResourceTypeNameShape::Constant,
    }
}

/// Resolves the type name of a BOXED `get_resource_type()` operand.
///
/// Unboxes, and consults `__rt_resource_type_name` only when the runtime tag is 9
/// (resource). Every other tag keeps answering the constant `"stream"` this builtin has
/// always answered, which matters for two reasons: elephc accepts
/// `get_resource_type(5)` today where PHP throws a `TypeError` (a separate,
/// deliberately untouched debt), and a boxed float's payload word IS its sign-carrying
/// IEEE bit pattern — `get_resource_type(-1.5)` would otherwise start reporting
/// `"Unknown"` because bit 63 of `-1.5` is set. The tag gate makes the sign test apply
/// to genuine resource payloads only.
fn emit_boxed_resource_type_name(ctx: &mut FunctionContext<'_>) {
    let (fallback_label, fallback_len) = ctx.data.add_string(b"stream");
    let resource_label = ctx.next_label("get_resource_type_resource");
    let done_label = ctx.next_label("get_resource_type_done");
    emit_boxed_resource_type_name_asm(
        ctx.emitter,
        &fallback_label,
        fallback_len,
        &resource_label,
        &done_label,
    );
}

/// Emits the assembly body of `emit_boxed_resource_type_name`, split out so both target
/// variants can be pinned without a `FunctionContext` (the precedent is
/// `emit_resource_release_sentinel` in `crate::codegen::lower_inst::builtins::io`).
///
/// `fallback_label`/`fallback_len` name the `.data` literal answered for every non-resource
/// tag; `resource_label` and `done_label` are the two locally unique branch targets.
fn emit_boxed_resource_type_name_asm(
    emitter: &mut Emitter,
    fallback_label: &str,
    fallback_len: usize,
    resource_label: &str,
    done_label: &str,
) {
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #9");                                  // check whether the boxed operand carries the resource tag
            emitter.instruction(&format!("b.eq {}", resource_label));           // only a genuine resource payload gets a computed type name
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 9");                                  // check whether the boxed operand carries the resource tag
            emitter.instruction(&format!("je {}", resource_label));             // only a genuine resource payload gets a computed type name
        }
    }
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, ptr_reg, fallback_label);
    abi::emit_load_int_immediate(emitter, len_reg, fallback_len as i64);        // every non-resource tag keeps the constant this builtin always answered
    abi::emit_jump(emitter, done_label);
    emitter.label(resource_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, x1");                                  // move the unboxed Mixed low payload into the integer result register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, rdi");                                // move the unboxed Mixed low payload into the integer result register
        }
    }
    abi::emit_call_label(emitter, "__rt_resource_type_name");                   // stream while the handle is open, Unknown once it is closed
    emitter.label(done_label);
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
    if name == "get_class" {
        let incomplete_label = ctx.next_label("get_class_incomplete");
        ctx.emitter.instruction("cmn x9, #2");                                  // reserved id -2 denotes an incomplete unserialized object
        ctx.emitter.instruction(&format!("b.eq {}", incomplete_label));         // expose PHP's required incomplete-class name
        // The ordinary class lookup below remains the only path for real ids.
        abi::emit_symbol_address(ctx.emitter, "x10", "_class_name_count");
        ctx.emitter.instruction("ldr x10, [x10]");                              // load dense class-name lookup bound
        ctx.emitter.instruction("cmp x9, x10");                                 // validate the object class id before indexing metadata
        ctx.emitter.instruction(&format!("b.hs {}", empty_label));              // invalid ids produce an empty class name
        abi::emit_symbol_address(ctx.emitter, "x11", "_class_name_entries");
        ctx.emitter.instruction("lsl x12, x9, #4");                             // scale class id by the 16-byte metadata row
        ctx.emitter.instruction("add x11, x11, x12");                           // select the class-name metadata row
        ctx.emitter.instruction(&format!("ldr {}, [x11]", ptr_reg));            // load the real class-name pointer
        ctx.emitter.instruction(&format!("ldr {}, [x11, #8]", len_reg));        // load the real class-name length
        ctx.emitter.instruction(&format!("b {}", done_label));                  // skip empty fallback after successful lookup
        ctx.emitter.label(&incomplete_label);
        abi::emit_symbol_address(ctx.emitter, ptr_reg, "_incomplete_class_name");
        abi::emit_load_int_immediate(ctx.emitter, len_reg, 22);
        ctx.emitter.instruction(&format!("b {}", done_label));                  // incomplete class name is final
    }
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
    if name == "get_class" {
        let incomplete_label = ctx.next_label("get_class_incomplete_x");
        ctx.emitter.instruction("cmp r8, -2");                                  // reserved id -2 denotes an incomplete unserialized object
        ctx.emitter.instruction(&format!("je {}", incomplete_label));           // expose PHP's required incomplete-class name
        ctx.emitter.instruction("mov r9, QWORD PTR [rip + _class_name_count]"); // load dense class-name lookup bound
        ctx.emitter.instruction("cmp r8, r9");                                  // validate the object class id before indexing metadata
        ctx.emitter.instruction(&format!("jae {}", empty_label));               // invalid ids produce an empty class name
        ctx.emitter.instruction("lea r10, [rip + _class_name_entries]");        // materialize class-name metadata table base
        ctx.emitter.instruction("shl r8, 4");                                   // scale class id by 16-byte row size
        ctx.emitter.instruction("mov rax, QWORD PTR [r10 + r8]");               // load the real class-name pointer
        ctx.emitter.instruction("mov rdx, QWORD PTR [r10 + r8 + 8]");           // load the real class-name length
        ctx.emitter.instruction(&format!("jmp {}", done_label));                // skip empty fallback after successful lookup
        ctx.emitter.label(&incomplete_label);
        ctx.emitter.instruction("lea rax, [rip + _incomplete_class_name]");     // return PHP incomplete-class name
        ctx.emitter.instruction("mov rdx, 22");                                 // byte length of __PHP_Incomplete_Class
        ctx.emitter.instruction(&format!("jmp {}", done_label));                // incomplete class name is final
    }
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
pub(super) fn emit_bool_result(ctx: &mut FunctionContext<'_>, value: bool) {
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
pub(super) fn emit_string_array(ctx: &mut FunctionContext<'_>, names: &[String]) -> Result<()> {
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
pub(super) fn optional_const_string_operand(
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

#[cfg(test)]
mod get_resource_type_asm_tests {
    use super::emit_boxed_resource_type_name_asm;
    use crate::codegen::emit::Emitter;
    use crate::codegen::platform::{Arch, Platform, Target};

    /// Emits the boxed `get_resource_type` body for one target and returns the assembly.
    fn emit_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_boxed_resource_type_name_asm(
            &mut emitter,
            "_str_stream",
            6,
            "_gt_resource",
            "_gt_done",
        );
        emitter.output()
    }

    /// Pins the whole AArch64 body as an ordered, exact-line block.
    ///
    /// The load-bearing line is `bl __rt_resource_type_name`: before it the builtin
    /// answered the literal `"stream"` unconditionally, so `fclose($r);
    /// get_resource_type($r)` reported `"stream"` where PHP 8.5.6 reports `"Unknown"`.
    #[test]
    fn aarch64_consults_the_runtime_type_name_for_resource_tags() {
        let asm = emit_for(Target::new(Platform::MacOS, Arch::AArch64));
        let expected = concat!(
            "    bl __rt_mixed_unbox\n",
            "    cmp x0, #9\n",
            "    b.eq _gt_resource\n",
            "    adrp x1, _str_stream@PAGE\n",
            "    add x1, x1, _str_stream@PAGEOFF\n",
            "    mov x2, #6\n",
            "    b _gt_done\n",
            "_gt_resource:\n",
            "    mov x0, x1\n",
            "    bl __rt_resource_type_name\n",
            "_gt_done:\n",
        );
        assert!(asm.contains(expected), "expected block missing:\n{asm}");
    }

    /// Pins the whole x86_64 body, so the two targets cannot drift: the payload move is
    /// `mov rax, rdi` here and `mov x0, x1` there, and an aarch64-only pin has already let
    /// an x86 fix be deleted silently on this branch.
    #[test]
    fn x86_64_consults_the_runtime_type_name_for_resource_tags() {
        let asm = emit_for(Target::new(Platform::Linux, Arch::X86_64));
        let expected = concat!(
            "    call __rt_mixed_unbox\n",
            "    cmp rax, 9\n",
            "    je _gt_resource\n",
            "    lea rax, [rip + _str_stream]\n",
            "    mov rdx, 6\n",
            "    jmp _gt_done\n",
            "_gt_resource:\n",
            "    mov rax, rdi\n",
            "    call __rt_resource_type_name\n",
            "_gt_done:\n",
        );
        assert!(asm.contains(expected), "expected block missing:\n{asm}");
    }

    /// Pins the operand-shape decision, which the lowering can no longer make inline.
    ///
    /// `Mixed`/`Union` is the shape every real program takes (`fopen()` types as
    /// `Union([Resource(Some("stream")), Bool])`); a bare `Resource` is unreachable today
    /// but handled uniformly anyway, because a sign test on an already-loaded payload
    /// costs two instructions and cannot mis-fire. Everything else must stay `Constant`:
    /// that is the arm that keeps `get_resource_type(5)` compiling, which is a separate
    /// debt this change deliberately does not close.
    #[test]
    fn the_operand_shape_decides_how_the_payload_is_reached() {
        use super::{resource_type_name_shape, ResourceTypeNameShape};
        use crate::types::PhpType;

        assert_eq!(
            resource_type_name_shape(&PhpType::Mixed),
            ResourceTypeNameShape::Boxed
        );
        assert_eq!(
            resource_type_name_shape(&PhpType::Union(vec![
                PhpType::Resource(Some("stream".to_string())),
                PhpType::Bool,
            ])),
            ResourceTypeNameShape::Boxed
        );
        assert_eq!(
            resource_type_name_shape(&PhpType::Resource(Some("stream".to_string()))),
            ResourceTypeNameShape::Unboxed
        );
        for other in [PhpType::Int, PhpType::Float, PhpType::Str, PhpType::Bool] {
            assert_eq!(
                resource_type_name_shape(&other),
                ResourceTypeNameShape::Constant,
                "acceptance must not change for {other:?}"
            );
        }
    }

    /// The non-resource tag must keep answering the constant, on both targets.
    ///
    /// elephc accepts `get_resource_type(5)` today where PHP throws a `TypeError`; that
    /// over-acceptance is a separate debt, and routing every tag through the sign test
    /// would ALSO make `get_resource_type(-1.5)` report `"Unknown"`, because bit 63 of a
    /// negative double is set. The tag gate is what keeps both cases at today's answer.
    #[test]
    fn a_non_resource_tag_keeps_the_constant_answer_on_both_targets() {
        for (target, gate) in [
            (Target::new(Platform::MacOS, Arch::AArch64), "    b.eq _gt_resource\n"),
            (Target::new(Platform::Linux, Arch::X86_64), "    je _gt_resource\n"),
        ] {
            let asm = emit_for(target);
            let fallthrough = asm
                .split(gate)
                .nth(1)
                .unwrap_or_else(|| panic!("missing resource-tag gate for {target:?}:\n{asm}"))
                .split("_gt_resource:\n")
                .next()
                .expect("the fallthrough arm precedes the resource arm")
                .to_string();
            assert!(
                fallthrough.contains("_str_stream"),
                "the non-resource arm must answer the constant ({target:?}):\n{fallthrough}"
            );
            assert!(
                !fallthrough.contains("__rt_resource_type_name"),
                "the non-resource arm must not reach the runtime resolver ({target:?}):\n{fallthrough}"
            );
        }
    }
}
