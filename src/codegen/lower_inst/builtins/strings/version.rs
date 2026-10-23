//! Purpose:
//! Lowers PHP `version_compare` to the `__rt_version_compare` runtime helper.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` dispatch groups via the strings facade.
//!
//! Key details:
//! - Without an operator (or with a null one) the result is the `int` -1/0/1; with one it is
//!   the `bool` predicate. A boxed or tagged-scalar (`?int`) operator is inspected at run
//!   time, and when the call's result is boxed (`int|bool`, callable wrappers) the matching
//!   shape is boxed.
//! - An unknown operator raises php-src's catchable `ValueError` from the helper's error flag.

use super::*;
use crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL;

/// `ValueError` raised for an operator `version_compare()` does not know.
const VERSION_COMPARE_OPERATOR_MESSAGE: &str =
    "version_compare(): Argument #3 ($operator) must be a valid comparison operator";

/// Runtime tag of a boxed string.
const MIXED_TAG_STRING: i64 = 1;

/// How the third `version_compare()` operand is known at compile time.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OperatorShape {
    /// Omitted or statically null: the int comparison form.
    Absent,
    /// A concrete scalar: converted to a string and used as the operator.
    Concrete,
    /// A boxed value: null selects the int form, a string is the operator.
    Boxed,
    /// A tagged scalar (`?int`, `?bool`): null selects the int form, anything else is converted
    /// to a string and used as the operator.
    Tagged,
}

/// Classifies the third operand.
fn operator_shape(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<OperatorShape> {
    let Some(&operator) = inst.operands.get(2) else {
        return Ok(OperatorShape::Absent);
    };
    Ok(match ctx.value_php_type(operator)?.codegen_repr() {
        PhpType::Void | PhpType::Never => OperatorShape::Absent,
        PhpType::Mixed | PhpType::Union(_) => OperatorShape::Boxed,
        PhpType::TaggedScalar => OperatorShape::Tagged,
        _ => OperatorShape::Concrete,
    })
}

/// Lowers `version_compare(version1, version2, operator?)`.
pub(crate) fn lower_version_compare(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "version_compare expected 2 or 3 args, got {}",
            inst.operands.len()
        )));
    }
    let shape = operator_shape(ctx, inst)?;
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    let (v1_ptr, v1_len, v2_ptr, v2_len, op_ptr, op_len, has_reg) = if arm {
        ("x1", "x2", "x3", "x4", "x5", "x6", "x7")
    } else {
        ("rdi", "rsi", "rdx", "rcx", "r8", "r9", "r10")
    };
    // Both versions are staged through the string result pair and parked on the stack: moving
    // a coerced (non-`Str`) second version straight into `rdx`/`rcx` would overwrite the result
    // length in `rdx` with the pointer before copying it.
    let (str_ptr, str_len) = abi::string_result_regs(ctx.emitter);
    load_string_arg_to_regs(ctx, inst, 0, "version_compare", str_ptr, str_len)?;
    abi::emit_push_reg_pair(ctx.emitter, str_ptr, str_len);
    load_string_arg_to_regs(ctx, inst, 1, "version_compare", str_ptr, str_len)?;
    abi::emit_push_reg_pair(ctx.emitter, str_ptr, str_len);
    match shape {
        OperatorShape::Absent => {
            abi::emit_load_int_immediate(ctx.emitter, op_ptr, 0);
            abi::emit_load_int_immediate(ctx.emitter, op_len, 0);
            abi::emit_load_int_immediate(ctx.emitter, has_reg, 0);
        }
        OperatorShape::Concrete => {
            load_string_arg_to_regs(ctx, inst, 2, "version_compare", op_ptr, op_len)?;
            abi::emit_load_int_immediate(ctx.emitter, has_reg, 1);
        }
        OperatorShape::Tagged => {
            let operator = expect_operand(inst, 2)?;
            // -- stage a tagged-scalar operator: null selects the int form --
            ctx.load_value_to_result(operator)?;
            let is_null = ctx.next_label("version_compare_operator_null");
            let ready = ctx.next_label("version_compare_operator_ready");
            if arm {
                ctx.emitter.instruction(&format!("cmp x1, #{TAGGED_SCALAR_TAG_NULL}")); // does the tagged operator carry the null tag?
                ctx.emitter.instruction(&format!("b.eq {is_null}"));            // null keeps the int comparison form
            } else {
                ctx.emitter.instruction(&format!("cmp rdx, {TAGGED_SCALAR_TAG_NULL}")); // does the tagged operator carry the null tag?
                ctx.emitter.instruction(&format!("je {is_null}"));              // null keeps the int comparison form
            }
            load_string_arg_to_regs(ctx, inst, 2, "version_compare", op_ptr, op_len)?;
            abi::emit_load_int_immediate(ctx.emitter, has_reg, 1);
            abi::emit_jump(ctx.emitter, &ready);
            ctx.emitter.label(&is_null);
            abi::emit_load_int_immediate(ctx.emitter, op_ptr, 0);
            abi::emit_load_int_immediate(ctx.emitter, op_len, 0);
            abi::emit_load_int_immediate(ctx.emitter, has_reg, 0);
            ctx.emitter.label(&ready);
        }
        OperatorShape::Boxed => {
            let operator = expect_operand(inst, 2)?;
            // -- stage a boxed operator: string text, null selects the int form --
            ctx.load_value_to_result(operator)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            let not_string = ctx.next_label("version_compare_operator_not_string");
            let ready = ctx.next_label("version_compare_operator_ready");
            if arm {
                ctx.emitter.instruction(&format!("cmp x0, #{MIXED_TAG_STRING}")); // is the boxed operator a string?
                ctx.emitter.instruction(&format!("b.ne {not_string}"));         // null or another type
                ctx.emitter.instruction("mov x5, x1");                          // the operator string pointer
                ctx.emitter.instruction("mov x6, x2");                          // the operator string length
                ctx.emitter.instruction("mov x7, #1");                          // the predicate form
                ctx.emitter.instruction(&format!("b {ready}"));                 // the operator is staged
                ctx.emitter.label(&not_string);
                ctx.emitter.instruction("mov x5, #0");                          // no operator text
                ctx.emitter.instruction("mov x6, #0");                          // no operator text
                ctx.emitter.instruction(&format!("cmp x0, #{TAGGED_SCALAR_TAG_NULL}")); // is the boxed operator null?
                ctx.emitter.instruction("cset x7, ne");                         // null selects the int form; anything else is invalid
            } else {
                ctx.emitter.instruction(&format!("cmp rax, {MIXED_TAG_STRING}")); // is the boxed operator a string?
                ctx.emitter.instruction(&format!("jne {not_string}"));          // null or another type
                ctx.emitter.instruction("mov r8, rdi");                         // the operator string pointer
                ctx.emitter.instruction("mov r9, rdx");                         // the operator string length
                ctx.emitter.instruction("mov r10d, 1");                         // the predicate form
                ctx.emitter.instruction(&format!("jmp {ready}"));               // the operator is staged
                ctx.emitter.label(&not_string);
                ctx.emitter.instruction("xor r8d, r8d");                        // no operator text
                ctx.emitter.instruction("xor r9d, r9d");                        // no operator text
                ctx.emitter.instruction(&format!("cmp rax, {TAGGED_SCALAR_TAG_NULL}")); // is the boxed operator null?
                ctx.emitter.instruction("setne r10b");                          // null selects the int form; anything else is invalid
                ctx.emitter.instruction("movzx r10d, r10b");                    // widen the predicate flag
            }
            ctx.emitter.label(&ready);
        }
    }
    // The predicate flag decides how a boxed result is shaped after the call.
    // -- park the predicate flag and reload both versions for the helper --
    abi::emit_push_reg(ctx.emitter, has_reg);
    if arm {
        ctx.emitter.instruction("ldp x3, x4, [sp, #16]");                       // reload the second version under the parked flag
        ctx.emitter.instruction("ldp x1, x2, [sp, #32]");                       // reload the first version under the parked flag
    } else {
        ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");               // reload the second version pointer under the parked flag
        ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 24]");               // reload the second version length
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");               // reload the first version pointer
        ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 40]");               // reload the first version length
    }
    // -- compare, raising ValueError for an unknown operator --
    abi::emit_call_label(ctx.emitter, "__rt_version_compare");
    let ok = ctx.next_label("version_compare_operator_ok");
    if arm {
        ctx.emitter.instruction(&format!("cbz x1, {ok}"));                      // a known (or absent) operator produced a result
    } else {
        ctx.emitter.instruction("test rdx, rdx");                               // did the helper reject the operator?
        ctx.emitter.instruction(&format!("jz {ok}"));                           // a known (or absent) operator produced a result
    }
    super::super::exceptions::emit_value_error(ctx, VERSION_COMPARE_OPERATOR_MESSAGE);
    ctx.emitter.label(&ok);
    // -- restore the parked flag and versions, then shape the result --
    let flag = if arm { "x9" } else { "r11" };
    abi::emit_pop_reg(ctx.emitter, flag);
    abi::emit_pop_reg_pair(ctx.emitter, v2_ptr, v2_len);
    abi::emit_pop_reg_pair(ctx.emitter, v1_ptr, v1_len);
    if inst.result.is_some() && inst.result_php_type.codegen_repr() == PhpType::Mixed {
        let as_bool = ctx.next_label("version_compare_box_bool");
        let boxed = ctx.next_label("version_compare_boxed");
        if arm {
            ctx.emitter.instruction(&format!("cbnz x9, {as_bool}"));            // the predicate form answers a bool
        } else {
            ctx.emitter.instruction("test r11, r11");                           // did the call use an operator?
            ctx.emitter.instruction(&format!("jnz {as_bool}"));                 // the predicate form answers a bool
        }
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
        abi::emit_jump(ctx.emitter, &boxed);
        ctx.emitter.label(&as_bool);
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
        ctx.emitter.label(&boxed);
    }
    store_if_result(ctx, inst)
}
