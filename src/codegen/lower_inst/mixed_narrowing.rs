//! Purpose:
//! Lowers declared-boundary narrowing ops that verify a dynamically-typed value with
//! PHP's coercive-mode rules instead of silently truncating it, and the strict narrowing a
//! fixed-layout `float`, `f32` or `bool` field applies to a boxed value before its store.
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
//! - A fixed-layout field narrowing accepts only the tags its field can hold (a float field
//!   also widens an int); it never casts, so the field gets a raw payload or a `TypeError`.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::Instruction;
use crate::types::PhpType;

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

/// Moves raw double bits from a general-purpose register into the float-result register.
pub(in crate::codegen::lower_inst) fn emit_float_bits_to_float_result(ctx: &mut FunctionContext<'_>, bits_reg: &str) {
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
pub(in crate::codegen::lower_inst) fn emit_float_result_fits_i64_or_jump(
    ctx: &mut FunctionContext<'_>,
    fail_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d0, d0");                             // NaN compares unordered with itself
            ctx.emitter.instruction(&format!("b.vs {}", fail_label));           // NaN never fits an int boundary
            abi::emit_load_int_immediate(ctx.emitter, "x9", F64_TWO_POW_63_BITS);
            ctx.emitter.instruction("fmov d1, x9");                             // materialize (double)2^63 without a data load
            ctx.emitter.instruction("fcmp d0, d1");                             // compare against the exclusive upper bound
            ctx.emitter.instruction(&format!("b.ge {}", fail_label));           // d >= 2^63 exceeds PHP_INT_MAX
            abi::emit_load_int_immediate(ctx.emitter, "x9", F64_NEG_TWO_POW_63_BITS);
            ctx.emitter.instruction("fmov d1, x9");                             // materialize (double)-2^63
            ctx.emitter.instruction("fcmp d0, d1");                             // compare against the inclusive lower bound
            ctx.emitter.instruction(&format!("b.lt {}", fail_label));           // d < -2^63 is below PHP_INT_MIN
            ctx.emitter.instruction("fcvtzs x0, d0");                           // in-range: exact truncation toward zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("ucomisd xmm0, xmm0");                      // NaN compares unordered with itself
            ctx.emitter.instruction(&format!("jp {}", fail_label));             // NaN never fits an int boundary
            abi::emit_load_int_immediate(ctx.emitter, "r10", F64_TWO_POW_63_BITS);
            ctx.emitter.instruction("movq xmm1, r10");                          // materialize (double)2^63 without a data load
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare against the exclusive upper bound
            ctx.emitter.instruction(&format!("jae {}", fail_label));            // d >= 2^63 exceeds PHP_INT_MAX
            abi::emit_load_int_immediate(ctx.emitter, "r10", F64_NEG_TWO_POW_63_BITS);
            ctx.emitter.instruction("movq xmm1, r10");                          // materialize (double)-2^63
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare against the inclusive lower bound
            ctx.emitter.instruction(&format!("jb {}", fail_label));             // d < -2^63 is below PHP_INT_MIN
            ctx.emitter.instruction("cvttsd2si rax, xmm0");                     // in-range: exact truncation toward zero
        }
    }
}

/// The scalar a fixed-layout field narrowing produces from a boxed `Mixed` value.
///
/// Packed class fields and extern struct fields reached through `ptr<T>` store raw bits, so a
/// value lowering still carries boxed is narrowed before the store: the tags the field can
/// hold pass, and every other one throws a catchable `TypeError`. `int` fields keep their own
/// `Op::PackedFieldMixedToInt` lowering.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::codegen::lower_inst) enum FixedFieldScalar {
    /// A `float` or `f32` field: a float passes and an int widens to the nearest double, the
    /// conversion PHP applies to an int written to a float.
    Float,
    /// A `bool` field: only a bool passes.
    Bool,
}

/// The runtime tags `__rt_mixed_unbox` reports and the word a refused one names in the
/// `TypeError`. Tag 7 (nested) is peeled by the helper; a tag outside the table is an object.
const FIXED_FIELD_TAGS: [(i64, &str); 9] = [
    (0, "int given"),
    (1, "string given"),
    (2, "float given"),
    (3, "bool given"),
    (4, "array given"),
    (5, "array given"),
    (8, "null given"),
    (9, "resource given"),
    (10, "Closure given"),
];

/// Lowers `Op::PackedFieldMixedToFloat` and `Op::PackedFieldMixedToBool`: narrows a boxed
/// `Mixed` value to the raw payload a fixed-layout field of `target` stores, or throws a
/// catchable `TypeError` built from the message prefix plus the runtime type word.
///
/// Like `PackedFieldMixedToInt`, the op is total over every post-fold representation of its
/// operand: constant folding runs after lowering and can retype the boxed operand to a raw
/// scalar under the op, and unboxing a raw scalar as a cell pointer would crash.
pub(in crate::codegen::lower_inst) fn lower_fixed_field_mixed_narrowing(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: FixedFieldScalar,
) -> Result<()> {
    let op_name = inst.op.name();
    let input = *inst.operands.first().ok_or_else(|| {
        CodegenIrError::unsupported(format!("{} without operand", op_name))
    })?;
    let Some(crate::ir::Immediate::Data(data_id)) = inst.immediate else {
        return Err(CodegenIrError::unsupported(format!(
            "{} without a TypeError message prefix",
            op_name
        )));
    };
    let (prefix_label, prefix_len) = ctx.intern_string_data(data_id)?;
    let loaded_ty = ctx.load_value_to_result(input)?.codegen_repr();
    if loaded_ty != PhpType::Mixed {
        emit_raw_fixed_field_narrowing(ctx, target, &loaded_ty, &prefix_label, prefix_len)?;
        return store_if_result(ctx, inst);
    }
    // Unbox the cell: the tag comes back in the int-result register and the payload low word
    // in the target's unbox payload register.
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let tag_reg = abi::int_result_reg(ctx.emitter);
    let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let done = ctx.next_label("fixed_field_narrow_done");
    let arms: Vec<(i64, &str, String)> = FIXED_FIELD_TAGS
        .iter()
        .map(|&(tag, refused)| (tag, refused, ctx.next_label("fixed_field_narrow_tag")))
        .collect();
    for (tag, _, label) in &arms {
        emit_mixed_tag_branch(ctx, tag_reg, *tag, label);
    }
    // Any other tag is an object-like value; every refusing arm throws and never falls through.
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, "object given");
    for (tag, refused, label) in &arms {
        ctx.emitter.label(label);
        if emit_accepted_fixed_field_payload(ctx, target, *tag, payload_reg) {
            abi::emit_jump(ctx.emitter, &done);
        } else {
            emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, refused);
        }
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Moves the payload of an accepted runtime tag into the result register `target` stores from
/// and returns `true`, or emits nothing and returns `false` when the field refuses the tag.
fn emit_accepted_fixed_field_payload(
    ctx: &mut FunctionContext<'_>,
    target: FixedFieldScalar,
    tag: i64,
    payload_reg: &str,
) -> bool {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match (target, tag) {
        // A float's payload is its IEEE-754 bit pattern.
        (FixedFieldScalar::Float, 2) => emit_float_bits_to_float_result(ctx, payload_reg),
        // An int widens to the nearest double, as PHP converts an int stored to a float.
        (FixedFieldScalar::Float, 0) => {
            emit_move_reg(ctx, result_reg, payload_reg);
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        // A bool's payload is already the 0 or 1 the field stores.
        (FixedFieldScalar::Bool, 3) => emit_move_reg(ctx, result_reg, payload_reg),
        _ => return false,
    }
    true
}

/// Narrows an operand constant folding already unboxed, giving a raw scalar the answer the
/// boxed path gives its runtime tag: the field's own type passes, an int widens into a float
/// field, and any other scalar throws the same `TypeError`.
fn emit_raw_fixed_field_narrowing(
    ctx: &mut FunctionContext<'_>,
    target: FixedFieldScalar,
    loaded_ty: &PhpType,
    prefix_label: &str,
    prefix_len: usize,
) -> Result<()> {
    match (target, loaded_ty) {
        (FixedFieldScalar::Float, PhpType::Float) | (FixedFieldScalar::Bool, PhpType::Bool) => {
            return Ok(());
        }
        (FixedFieldScalar::Float, PhpType::Int) => {
            abi::emit_int_result_to_float_result(ctx.emitter);
            return Ok(());
        }
        _ => {}
    }
    let refused = match loaded_ty {
        PhpType::Int => "int given",
        PhpType::Float => "float given",
        PhpType::Bool => "bool given",
        PhpType::Str => "string given",
        PhpType::Void => "null given",
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "fixed-layout field narrowing of raw PHP type {:?}",
                other
            )));
        }
    };
    emit_throw_int_arg_type_error(ctx, prefix_label, prefix_len, refused);
    Ok(())
}
