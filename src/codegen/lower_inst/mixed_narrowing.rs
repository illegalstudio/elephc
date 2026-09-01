//! Purpose:
//! Lowers declared-boundary narrowing ops that verify a dynamically-typed value with
//! PHP's coercive-mode rules instead of silently truncating it.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - A float fits a PHP `int` boundary iff it is ordered (not NaN) and inside
//!   `[-2^63, 2^63)` — `ZEND_DOUBLE_FITS_LONG`'s exact comparison; everything outside
//!   throws `TypeError`, never wraps.
//! - Shares the unbox/tag-dispatch/throw helpers with `EnumBackingMixedToInt`, whose
//!   coercions (float truncation, null-to-0) are intentionally NOT reused here.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::Instruction;

use super::super::context::FunctionContext;
use super::enums::{
    emit_mixed_tag_branch, emit_move_reg, emit_string_result_to_int_checked,
    emit_throw_int_arg_type_error,
};
use super::store_if_result;
use crate::codegen::{CodegenIrError, Result};

/// IEEE-754 bit pattern of `(double)2^63`, the first double a PHP `int` cannot hold.
const F64_TWO_POW_63_BITS: i64 = 0x43E0000000000000;
/// IEEE-754 bit pattern of `(double)-2^63`, the smallest double a PHP `int` can hold.
const F64_NEG_TWO_POW_63_BITS: i64 = 0xC3E0_0000_0000_0000_u64 as i64;

/// Lowers `Op::ReturnBoundaryMixedToInt`: verifies a value reaching a DECLARED `int`
/// return with PHP's coercive rules. int/bool forward the payload, a numeric string
/// coerces, an in-range float truncates, and every other runtime shape — including a
/// float outside `[-2^63, 2^63)`, which the plain int coercion would silently wrap —
/// throws a catchable `TypeError` built from the message prefix plus the runtime type
/// word and `" returned"`.
pub(in crate::codegen::lower_inst) fn lower_return_boundary_mixed_to_int(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let input = *inst.operands.first().ok_or_else(|| {
        CodegenIrError::unsupported("return_boundary_mixed_to_int without operand".to_string())
    })?;
    let Some(crate::ir::Immediate::Data(data_id)) = inst.immediate else {
        return Err(CodegenIrError::unsupported(
            "return_boundary_mixed_to_int without a TypeError message prefix".to_string(),
        ));
    };
    let (prefix_label, prefix_len) = ctx.intern_string_data(data_id)?;
    let done = ctx.next_label("ret_boundary_int_done");
    let l_float_fail = ctx.next_label("ret_boundary_int_float_fail");
    let loaded_ty = ctx.load_value_to_result(input)?.codegen_repr();
    // Constant folding runs AFTER lowering and can retype the operand under the op: a
    // checker-Mixed value becomes a raw I64 or F64. The op must be total over every
    // post-fold representation — unboxing a raw scalar as a pointer is a segfault.
    if matches!(
        loaded_ty,
        crate::types::PhpType::Int | crate::types::PhpType::Bool
    ) {
        // Already the integer word; nothing to verify.
        return store_if_result(ctx, inst);
    }
    if matches!(loaded_ty, crate::types::PhpType::Float) {
        // A raw F64 (the promoted overflow result) without a box; the float-result
        // register already holds it.
        emit_float_result_fits_i64_or_jump(ctx, &l_float_fail);
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&l_float_fail);
        emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "float returned");
        ctx.emitter.label(&done);
        return store_if_result(ctx, inst);
    }
    // Unbox the Mixed cell. `__rt_mixed_unbox` returns tag in the int-result register and the
    // payload lo/hi in target-specific registers (AArch64: x1/x2; x86_64: rdi/rdx).
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let tag_reg = abi::int_result_reg(ctx.emitter);
    let (lo_reg, hi_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rdx"),
    };
    let l_scalar = ctx.next_label("ret_boundary_int_scalar");
    let l_float = ctx.next_label("ret_boundary_int_float");
    let l_string = ctx.next_label("ret_boundary_int_string");
    let l_null = ctx.next_label("ret_boundary_int_null");
    let l_array = ctx.next_label("ret_boundary_int_array");
    let l_resource = ctx.next_label("ret_boundary_int_resource");
    let l_callable = ctx.next_label("ret_boundary_int_callable");
    // Tag values: 0 int, 1 string, 2 float, 3 bool, 4 indexed array, 5 hash, 6 object,
    // 8 null, 9 resource, 10 callable (7 nested is peeled by `__rt_mixed_unbox`).
    emit_mixed_tag_branch(ctx, tag_reg, 0, &l_scalar);
    emit_mixed_tag_branch(ctx, tag_reg, 3, &l_scalar);
    emit_mixed_tag_branch(ctx, tag_reg, 2, &l_float);
    emit_mixed_tag_branch(ctx, tag_reg, 1, &l_string);
    emit_mixed_tag_branch(ctx, tag_reg, 8, &l_null);
    emit_mixed_tag_branch(ctx, tag_reg, 4, &l_array);
    emit_mixed_tag_branch(ctx, tag_reg, 5, &l_array);
    emit_mixed_tag_branch(ctx, tag_reg, 9, &l_resource);
    emit_mixed_tag_branch(ctx, tag_reg, 10, &l_callable);
    // Any other tag is an object-like value; each arm throws and never falls through.
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "object returned");
    // int / bool: the payload is already the integer value.
    ctx.emitter.label(&l_scalar);
    emit_move_reg(ctx, tag_reg, lo_reg);
    abi::emit_jump(ctx.emitter, &done);
    // float: verify the PHP int range before truncating — out of range is a TypeError,
    // exactly where the silent path used to wrap around.
    ctx.emitter.label(&l_float);
    emit_float_bits_to_float_result(ctx, lo_reg);
    emit_float_result_fits_i64_or_jump(ctx, &l_float_fail);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&l_float_fail);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "float returned");
    // string: move the payload ptr/len into the string-result regs, then coerce strictly.
    ctx.emitter.label(&l_string);
    let (string_ptr_reg, string_len_reg) = abi::string_result_regs(ctx.emitter);
    emit_move_reg(ctx, string_ptr_reg, lo_reg);
    emit_move_reg(ctx, string_len_reg, hi_reg);
    let string_invalid = ctx.next_label("ret_boundary_int_string_invalid");
    emit_string_result_to_int_checked(ctx, &string_invalid);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&string_invalid);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "string returned");
    // Non-coercible types throw a TypeError naming the runtime type, like PHP.
    ctx.emitter.label(&l_null);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "null returned");
    ctx.emitter.label(&l_array);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "array returned");
    ctx.emitter.label(&l_resource);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "resource returned");
    ctx.emitter.label(&l_callable);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "Closure returned");
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers a declared object return boundary from one owned boxed runtime value.
pub(in crate::codegen::lower_inst) fn lower_return_boundary_mixed_to_object(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let input = *inst.operands.first().ok_or_else(|| {
        CodegenIrError::unsupported("return_boundary_mixed_to_object without operand".to_string())
    })?;
    let Some(crate::ir::Immediate::Data(data_id)) = inst.immediate else {
        return Err(CodegenIrError::unsupported(
            "return_boundary_mixed_to_object without target metadata".to_string(),
        ));
    };
    let spec = ctx
        .module
        .data
        .strings
        .get(data_id.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("return boundary metadata", data_id.as_raw()))?;
    let (target_class, prefix) = spec.split_once('\0').ok_or_else(|| {
        CodegenIrError::unsupported("malformed object return boundary metadata".to_string())
    })?;
    let target_class = target_class.to_string();
    let prefix_bytes = crate::string_bytes::literal_bytes(prefix);
    let (prefix_label, prefix_len) = ctx.data.add_string(&prefix_bytes);
    let target = if target_class.is_empty() {
        None
    } else if let Some(info) = ctx.module.class_infos.get(&target_class) {
        Some((info.class_id, 0))
    } else if let Some(info) = ctx.module.interface_infos.get(&target_class) {
        Some((info.interface_id, 1))
    } else {
        return Err(CodegenIrError::unsupported(format!(
            "unknown object return boundary target {target_class}"
        )));
    };

    ctx.load_value_to_result(input)?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let tag_reg = abi::int_result_reg(ctx.emitter);
    let object_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    };
    let object = ctx.next_label("ret_boundary_object_value");
    let success = ctx.next_label("ret_boundary_object_success");
    let wrong_object = ctx.next_label("ret_boundary_object_wrong_class");
    let integer = ctx.next_label("ret_boundary_object_int");
    let string = ctx.next_label("ret_boundary_object_string");
    let float = ctx.next_label("ret_boundary_object_float");
    let boolean = ctx.next_label("ret_boundary_object_bool");
    let array = ctx.next_label("ret_boundary_object_array");
    let null = ctx.next_label("ret_boundary_object_null");
    let resource = ctx.next_label("ret_boundary_object_resource");
    let callable = ctx.next_label("ret_boundary_object_callable");
    emit_mixed_tag_branch(ctx, tag_reg, 6, &object);
    emit_mixed_tag_branch(ctx, tag_reg, 0, &integer);
    emit_mixed_tag_branch(ctx, tag_reg, 1, &string);
    emit_mixed_tag_branch(ctx, tag_reg, 2, &float);
    emit_mixed_tag_branch(ctx, tag_reg, 3, &boolean);
    emit_mixed_tag_branch(ctx, tag_reg, 4, &array);
    emit_mixed_tag_branch(ctx, tag_reg, 5, &array);
    emit_mixed_tag_branch(ctx, tag_reg, 8, &null);
    emit_mixed_tag_branch(ctx, tag_reg, 9, &resource);
    emit_mixed_tag_branch(ctx, tag_reg, 10, &callable);
    emit_consuming_return_type_error(ctx, &prefix_label, prefix_len, "unknown returned");

    ctx.emitter.label(&object);
    abi::emit_push_reg(ctx.emitter, object_reg);
    if let Some((target_id, target_kind)) = target {
        emit_move_reg(ctx, abi::int_arg_reg_name(ctx.emitter.target, 0), object_reg);
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
        abi::emit_call_label(ctx.emitter, "__rt_exception_matches");
        abi::emit_branch_if_int_result_zero(ctx.emitter, &wrong_object);
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_jump(ctx.emitter, &success);

    ctx.emitter.label(&wrong_object);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    emit_consuming_object_return_type_error(ctx, &prefix_label, prefix_len);

    for (label, suffix) in [
        (&integer, "int returned"),
        (&string, "string returned"),
        (&float, "float returned"),
        (&boolean, "bool returned"),
        (&array, "array returned"),
        (&null, "null returned"),
        (&resource, "resource returned"),
        (&callable, "Closure returned"),
    ] {
        ctx.emitter.label(label);
        emit_consuming_return_type_error(ctx, &prefix_label, prefix_len, suffix);
    }

    ctx.emitter.label(&success);
    abi::emit_incref_if_refcounted(ctx.emitter, &crate::types::PhpType::Object(target_class));
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    release_consumed_mixed_cell(ctx, 16);
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    store_if_result(ctx, inst)
}

/// Builds and throws one static-suffix return `TypeError`, consuming the boxed input owner.
fn emit_consuming_return_type_error(
    ctx: &mut FunctionContext<'_>,
    prefix_label: &str,
    prefix_len: usize,
    suffix: &str,
) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, prefix_label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, prefix_len as i64);
    append_static_to_string_result(ctx, suffix);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    release_consumed_mixed_preserving_string(ctx);
    super::enums::emit_throw_type_error_from_string_result(ctx);
}

/// Throws an incompatible-object return `TypeError` using its concrete runtime class name.
fn emit_consuming_object_return_type_error(
    ctx: &mut FunctionContext<'_>,
    prefix_label: &str,
    prefix_len: usize,
) {
    let (name_ptr, name_len) = abi::string_result_regs(ctx.emitter);
    let class_id = abi::secondary_scratch_reg(ctx.emitter);
    let table = abi::symbol_scratch_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {}, [x0]", class_id));        // read the returned object's runtime class id
            abi::emit_symbol_address(ctx.emitter, table, "_class_name_entries");
            ctx.emitter.instruction(&format!("lsl {}, {}, #4", class_id, class_id)); // scale the class id to its metadata row
            ctx.emitter.instruction(&format!("add {}, {}, {}", table, table, class_id)); // address the runtime class-name row
            ctx.emitter.instruction(&format!("ldr {}, [{}]", name_ptr, table)); // load the concrete class-name pointer
            ctx.emitter.instruction(&format!("ldr {}, [{}, #8]", name_len, table)); // load the concrete class-name length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [rax]", class_id)); // read the returned object's runtime class id
            abi::emit_symbol_address(ctx.emitter, table, "_class_name_entries");
            ctx.emitter.instruction(&format!("shl {}, 4", class_id));           // scale the class id to its metadata row
            ctx.emitter.instruction(&format!("add {}, {}", table, class_id));   // address the runtime class-name row
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{}]", name_ptr, table)); // load the concrete class-name pointer
            ctx.emitter.instruction(&format!("mov {}, QWORD PTR [{} + 8]", name_len, table)); // load the concrete class-name length
        }
    }
    prepend_static_to_string_result(ctx, prefix_label, prefix_len);
    append_static_to_string_result(ctx, " returned");
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    release_consumed_mixed_preserving_string(ctx);
    super::enums::emit_throw_type_error_from_string_result(ctx);
}

/// Prepends a static string to the current string-result pair.
fn prepend_static_to_string_result(
    ctx: &mut FunctionContext<'_>,
    prefix_label: &str,
    prefix_len: usize,
) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let (right_ptr, right_len) = concat_right_regs(ctx);
    emit_move_reg(ctx, right_ptr, ptr_reg);
    emit_move_reg(ctx, right_len, len_reg);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, prefix_label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, prefix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// Appends a static string to the current string-result pair.
fn append_static_to_string_result(ctx: &mut FunctionContext<'_>, suffix: &str) {
    let (right_ptr, right_len) = concat_right_regs(ctx);
    let (label, len) = ctx.data.add_string(suffix.as_bytes());
    abi::emit_symbol_address(ctx.emitter, right_ptr, &label);
    abi::emit_load_int_immediate(ctx.emitter, right_len, len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// Returns the ABI registers carrying `__rt_concat`'s right-hand string.
fn concat_right_regs(ctx: &FunctionContext<'_>) -> (&'static str, &'static str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ("x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi"),
    }
}

/// Releases the owned boxed input while preserving a built error-message string.
fn release_consumed_mixed_preserving_string(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    release_consumed_mixed_cell(ctx, 16);
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Decrements the consumed Mixed cell stored at `stack_offset` without changing stack depth.
fn release_consumed_mixed_cell(ctx: &mut FunctionContext<'_>, stack_offset: usize) {
    abi::emit_load_temporary_stack_slot(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        stack_offset,
    );
    abi::emit_decref_if_refcounted(ctx.emitter, &crate::types::PhpType::Mixed);
}

/// Moves raw double bits from a general-purpose register into the float-result register.
fn emit_float_bits_to_float_result(ctx: &mut FunctionContext<'_>, bits_reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("fmov d0, {}", bits_reg));         // reinterpret the boxed payload as a double
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("movq xmm0, {}", bits_reg));       // reinterpret the boxed payload as a double
        }
    }
}

/// Truncates the double in the float-result register into the int-result register when it
/// fits a PHP `int`, or jumps to `fail_label`. The fit test is `ZEND_DOUBLE_FITS_LONG`'s:
/// ordered (not NaN), `< 2^63`, and `>= -2^63` — `-2^63` itself is representable.
fn emit_float_result_fits_i64_or_jump(ctx: &mut FunctionContext<'_>, fail_label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d0, d0");                             // NaN compares unordered with itself
            ctx.emitter.instruction(&format!("b.vs {}", fail_label));           // NaN never fits an int boundary
            abi::emit_load_int_immediate(ctx.emitter, "x9", F64_TWO_POW_63_BITS);
            ctx.emitter.instruction("fmov d1, x9");                             // materialize (double)2^63 without a data load
            ctx.emitter.instruction("fcmp d0, d1");                             // compare against the exclusive positive int bound
            ctx.emitter.instruction(&format!("b.ge {}", fail_label));           // d >= 2^63 exceeds PHP_INT_MAX
            abi::emit_load_int_immediate(ctx.emitter, "x9", F64_NEG_TWO_POW_63_BITS);
            ctx.emitter.instruction("fmov d1, x9");                             // materialize (double)-2^63
            ctx.emitter.instruction("fcmp d0, d1");                             // compare against the inclusive negative int bound
            ctx.emitter.instruction(&format!("b.lt {}", fail_label));           // d < -2^63 is below PHP_INT_MIN
            ctx.emitter.instruction("fcvtzs x0, d0");                           // in-range: exact truncation toward zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("ucomisd xmm0, xmm0");                      // NaN compares unordered with itself
            ctx.emitter.instruction(&format!("jp {}", fail_label));             // NaN never fits an int boundary
            abi::emit_load_int_immediate(ctx.emitter, "r10", F64_TWO_POW_63_BITS);
            ctx.emitter.instruction("movq xmm1, r10");                          // materialize (double)2^63 without a data load
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare against the exclusive positive int bound
            ctx.emitter.instruction(&format!("jae {}", fail_label));            // d >= 2^63 exceeds PHP_INT_MAX
            abi::emit_load_int_immediate(ctx.emitter, "r10", F64_NEG_TWO_POW_63_BITS);
            ctx.emitter.instruction("movq xmm1, r10");                          // materialize (double)-2^63
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare against the inclusive negative int bound
            ctx.emitter.instruction(&format!("jb {}", fail_label));             // d < -2^63 is below PHP_INT_MIN
            ctx.emitter.instruction("cvttsd2si rax, xmm0");                     // in-range: exact truncation toward zero
        }
    }
}
