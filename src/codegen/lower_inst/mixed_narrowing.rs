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
    emit_mixed_tag_branch, emit_move_reg, emit_throw_int_arg_type_error,
    emit_throw_type_error_from_string_result,
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
    // `ProfiledData { strict_php }` marks a `declare(strict_types=1)` return: only an int passes,
    // and a float or bool throws instead of converting.
    let (data_id, strict) = match inst.immediate {
        Some(crate::ir::Immediate::Data(data_id)) => (data_id, false),
        Some(crate::ir::Immediate::ProfiledData { data, strict_php }) => (data, strict_php),
        _ => {
            return Err(CodegenIrError::unsupported(
                "return_boundary_mixed_to_int without a TypeError message prefix".to_string(),
            ))
        }
    };
    // The same verification guards a family-boxed `int` PARAMETER on method entry
    // (`ir_lower::stmt::coerce_boxed_family_param`), whose PHP message says "given".
    let prefix_text = ctx
        .module
        .data
        .strings
        .get(data_id.as_raw() as usize)
        .cloned()
        .unwrap_or_default();
    let is_argument = prefix_text.contains("(): Argument #");
    let word = |ty: &str| -> String {
        format!("{} {}", ty, if is_argument { "given" } else { "returned" })
    };
    let (prefix_label, prefix_len) = ctx.intern_string_data(data_id)?;
    let done = ctx.next_label("ret_boundary_int_done");
    let l_float_fail = ctx.next_label("ret_boundary_int_float_fail");
    let loaded_ty = ctx.load_value_to_result(input)?.codegen_repr();
    // Constant folding runs AFTER lowering and can retype the operand under the op: a
    // checker-Mixed value becomes a raw I64 or F64. The op must be total over every
    // post-fold representation — unboxing a raw scalar as a pointer is a segfault.
    if matches!(loaded_ty, crate::types::PhpType::Int)
        || (!strict && matches!(loaded_ty, crate::types::PhpType::Bool))
    {
        // Already the integer word; nothing to verify.
        return store_if_result(ctx, inst);
    }
    if strict && matches!(loaded_ty, crate::types::PhpType::Bool) {
        let l_true = ctx.next_label("ret_boundary_int_strict_true");
        emit_branch_if_nonzero(ctx, abi::int_result_reg(ctx.emitter), &l_true);
        emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, &word("false"));
        ctx.emitter.label(&l_true);
        emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, &word("true"));
        ctx.emitter.label(&done);
        return store_if_result(ctx, inst);
    }
    if matches!(loaded_ty, crate::types::PhpType::Float) {
        // A raw F64 (the promoted overflow result) without a box; the float-result
        // register already holds it. Strict mode rejects every float.
        if strict {
            abi::emit_jump(ctx.emitter, &l_float_fail);
        } else {
            emit_float_result_fits_i64_or_jump(ctx, &l_float_fail);
        }
        abi::emit_jump(ctx.emitter, &done);
        ctx.emitter.label(&l_float_fail);
        emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, &word("float"));
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
    let l_bool_strict = ctx.next_label("ret_boundary_int_bool_strict");
    let l_string_strict = ctx.next_label("ret_boundary_int_string_strict");
    emit_mixed_tag_branch(ctx, tag_reg, 0, &l_scalar);
    if strict {
        // strict_types: an int is the only accepted tag; a float, bool or string throws.
        emit_mixed_tag_branch(ctx, tag_reg, 3, &l_bool_strict);
        emit_mixed_tag_branch(ctx, tag_reg, 2, &l_float_fail);
        emit_mixed_tag_branch(ctx, tag_reg, 1, &l_string_strict);
    } else {
        emit_mixed_tag_branch(ctx, tag_reg, 3, &l_scalar);
        emit_mixed_tag_branch(ctx, tag_reg, 2, &l_float);
        emit_mixed_tag_branch(ctx, tag_reg, 1, &l_string);
    }
    emit_mixed_tag_branch(ctx, tag_reg, 8, &l_null);
    emit_mixed_tag_branch(ctx, tag_reg, 4, &l_array);
    emit_mixed_tag_branch(ctx, tag_reg, 5, &l_array);
    emit_mixed_tag_branch(ctx, tag_reg, 9, &l_resource);
    emit_mixed_tag_branch(ctx, tag_reg, 10, &l_callable);
    // Any other tag is an object; php-src names its class ("C returned"), read from the same
    // class-name table `get_class()` uses. Each arm throws and never falls through.
    emit_move_reg(ctx, tag_reg, lo_reg);
    super::runtime_class_messages::emit_runtime_class_name_to_string_result(ctx, tag_reg, "object");
    super::runtime_class_messages::emit_concat_static_prefix(ctx, &prefix_text);
    super::runtime_class_messages::emit_concat_static_suffix(ctx, &word(""));
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    emit_throw_type_error_from_string_result(ctx);
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
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, &word("float"));
    // string: php-src's `is_numeric_string` verdict. `__rt_str_numeric_value` answers the tag
    // (0 int, 2 float) in the int-result register, the exact int or the double bits in the
    // payload-low register, and a whole-string status in the payload-high register. An integer
    // spelling that overflows comes back as a float, so `"9223372036854775808"` and `"1e100"`
    // fail the float range check and throw instead of saturating.
    ctx.emitter.label(&l_string);
    let arg0 = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(ctx.emitter.target, 1);
    // The pointer moves first: on AArch64 the length argument register is the payload register.
    emit_move_reg(ctx, arg0, lo_reg);
    emit_move_reg(ctx, arg1, hi_reg);
    abi::emit_call_label(ctx.emitter, "__rt_str_numeric_value");
    let string_invalid = ctx.next_label("ret_boundary_int_string_invalid");
    let string_int = ctx.next_label("ret_boundary_int_string_int");
    emit_branch_if_nonzero(ctx, hi_reg, &string_invalid);
    emit_mixed_tag_branch(ctx, tag_reg, 0, &string_int);
    emit_float_bits_to_float_result(ctx, lo_reg);
    emit_float_result_fits_i64_or_jump(ctx, &string_invalid);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&string_int);
    emit_move_reg(ctx, tag_reg, lo_reg);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&string_invalid);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, &word("string"));
    if strict {
        ctx.emitter.label(&l_bool_strict);
        let l_true = ctx.next_label("ret_boundary_int_strict_true");
        emit_branch_if_nonzero(ctx, lo_reg, &l_true);
        emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, &word("false"));
        ctx.emitter.label(&l_true);
        emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, &word("true"));
        ctx.emitter.label(&l_string_strict);
        emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, &word("string"));
    }
    // Non-coercible types throw a TypeError naming the runtime type, like PHP.
    ctx.emitter.label(&l_null);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, &word("null"));
    ctx.emitter.label(&l_array);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, &word("array"));
    ctx.emitter.label(&l_resource);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, &word("resource"));
    ctx.emitter.label(&l_callable);
    emit_throw_int_arg_type_error(ctx, &prefix_label, prefix_len, &word("Closure"));
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

/// Branches to `target` when `reg` holds a non-zero word (a `true` boolean payload).
fn emit_branch_if_nonzero(ctx: &mut FunctionContext<'_>, reg: &str, target: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz {}, {}", reg, target));      // a non-zero boolean payload is `true`
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", reg, reg));         // set flags from the boolean payload
            ctx.emitter.instruction(&format!("jnz {}", target));                // a non-zero boolean payload is `true`
        }
    }
}
