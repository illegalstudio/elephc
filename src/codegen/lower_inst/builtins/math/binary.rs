//! Purpose:
//! Lowers binary numeric PHP builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::math`.
//!
//! Key details:
//! - Preserves PHP source evaluation order before arranging libc/ABI argument
//!   registers for integer division and floating-point helpers.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Lowers `intdiv()` for concrete integer-like numeric operands.
pub(crate) fn lower_intdiv(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "intdiv", 2)?;
    let zero_label = ctx.next_label("intdiv_zero");
    let overflow_label = ctx.next_label("intdiv_overflow");
    let done_label = ctx.next_label("intdiv_done");
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_numeric_as_int(ctx, lhs, "intdiv")?;
            abi::emit_push_reg(ctx.emitter, "x0");
            load_numeric_as_int(ctx, rhs, "intdiv")?;
            abi::emit_pop_reg(ctx.emitter, "x1");
            ctx.emitter.instruction(&format!("cbz x0, {}", zero_label));        // branch to the fatal path when the divisor is zero
            emit_intdiv_overflow_check_arm64(ctx, "x1", "x0", &overflow_label);
            ctx.emitter.instruction("sdiv x0, x1, x0");                         // divide the saved dividend by the current divisor
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the fatal path after successful integer division
        }
        Arch::X86_64 => {
            load_numeric_as_int(ctx, lhs, "intdiv")?;
            abi::emit_push_reg(ctx.emitter, "rax");
            load_numeric_as_int(ctx, rhs, "intdiv")?;
            abi::emit_pop_reg(ctx.emitter, "r11");
            ctx.emitter.instruction("test rax, rax");                           // check whether the divisor is zero
            ctx.emitter.instruction(&format!("je {}", zero_label));             // branch to the fatal path when the divisor is zero
            emit_intdiv_overflow_check_x86_64(ctx, "r11", "rax", &overflow_label);
            ctx.emitter.instruction("mov r10, rax");                            // preserve the divisor before idiv uses rax
            ctx.emitter.instruction("mov rax, r11");                            // move the saved dividend into the idiv accumulator
            ctx.emitter.instruction("cqo");                                     // sign-extend the dividend across rdx:rax
            ctx.emitter.instruction("idiv r10");                                // divide the saved dividend by the preserved divisor
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the fatal path after successful integer division
        }
    }
    emit_intdiv_zero_fatal(ctx, &zero_label);
    ctx.emitter.label(&overflow_label);
    emit_intdiv_overflow_throw(ctx);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers `fdiv()` for concrete integer-like and floating operands.
pub(crate) fn lower_fdiv(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "fdiv", 2)?;
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    super::load_numeric_as_float(ctx, lhs, "fdiv")?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
    super::load_numeric_as_float(ctx, rhs, "fdiv")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_float_reg(ctx.emitter, "d1");
            ctx.emitter.instruction("fdiv d0, d1, d0");                         // compute dividend divided by divisor in the result register
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("divsd xmm1, xmm0"); // compute dividend divided by divisor in the scratch register
            ctx.emitter.instruction("movsd xmm0, xmm1"); // move the floating quotient into the result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `fmod()` for concrete integer-like and floating operands.
pub(crate) fn lower_fmod(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "fmod", 2)?;
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    super::load_numeric_as_float(ctx, lhs, "fmod")?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
    super::load_numeric_as_float(ctx, rhs, "fmod")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_float_reg(ctx.emitter, "d1");
            ctx.emitter.instruction("fdiv d2, d1, d0");                         // compute dividend divided by divisor for fmod truncation
            ctx.emitter.instruction("frintz d2, d2");                           // truncate the quotient toward zero
            ctx.emitter.instruction("fmsub d0, d2, d0, d1");                    // compute dividend minus truncated quotient times divisor
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("movapd xmm2, xmm0");                       // preserve the divisor while ordering libc fmod arguments
            ctx.emitter.instruction("movapd xmm0, xmm1");                       // move the dividend into the first libc fmod argument
            ctx.emitter.instruction("movapd xmm1, xmm2");                       // move the divisor into the second libc fmod argument
            ctx.emitter.bl_c("fmod");
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `pow()` for concrete integer-like and floating operands.
pub(crate) fn lower_pow(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "pow", 2)?;
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    super::load_numeric_as_float(ctx, lhs, "pow")?;
    abi::emit_push_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
    super::load_numeric_as_float(ctx, rhs, "pow")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov d1, d0");                             // move the exponent into the second libc pow argument
            abi::emit_pop_float_reg(ctx.emitter, "d0");
            ctx.emitter.bl_c("pow");
        }
        Arch::X86_64 => {
            abi::emit_pop_float_reg(ctx.emitter, "xmm1");
            ctx.emitter.instruction("movapd xmm2, xmm0");                       // preserve the exponent while ordering libc pow arguments
            ctx.emitter.instruction("movapd xmm0, xmm1");                       // move the base into the first libc pow argument
            ctx.emitter.instruction("movapd xmm1, xmm2");                       // move the exponent into the second libc pow argument
            ctx.emitter.bl_c("pow");
        }
    }
    store_if_result(ctx, inst)
}

/// Emits the legacy fatal diagnostic for `intdiv()` division by zero.
fn emit_intdiv_zero_fatal(ctx: &mut FunctionContext<'_>, zero_label: &str) {
    ctx.emitter.label(zero_label);
    let (err_label, err_len) = ctx.data.add_string(b"Fatal error: division by zero\n");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // select stderr as the fatal diagnostic destination
            abi::emit_symbol_address(ctx.emitter, "x1", &err_label);               // resolve the fatal diagnostic message address
            ctx.emitter.instruction(&format!("mov x2, #{}", err_len));          // pass the fatal diagnostic byte length to write()
            ctx.emitter.syscall(4);
            ctx.emitter.instruction("mov x0, #1");                              // select process exit code 1 after the fatal diagnostic
            ctx.emitter.syscall(1);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("lea rsi, [rip + {}]", err_label)); // pass the fatal diagnostic buffer to write()
            ctx.emitter.instruction(&format!("mov edx, {}", err_len));          // pass the fatal diagnostic byte length to write()
            ctx.emitter.instruction("mov edi, 2");                              // select stderr as the fatal diagnostic destination
            ctx.emitter.instruction("mov eax, 1");                              // select Linux write syscall
            ctx.emitter.instruction("syscall");                                 // write the fatal division-by-zero diagnostic
            ctx.emitter.instruction("mov edi, 1");                              // select process exit code 1 after the fatal diagnostic
            ctx.emitter.instruction("mov eax, 60");                             // select Linux exit syscall
            ctx.emitter.instruction("syscall");                                 // terminate after reporting division by zero
        }
    }
}

/// Emits the AArch64 overflow check for `intdiv(PHP_INT_MIN, -1)`.
/// Branches to `overflow_label` when `dividend == i64::MIN && divisor == -1`.
fn emit_intdiv_overflow_check_arm64(
    ctx: &mut FunctionContext<'_>,
    dividend_reg: &str,
    divisor_reg: &str,
    overflow_label: &str,
) {
    let int_min_label = ctx.next_label("intdiv_int_min");
    let neg_one = abi::symbol_scratch_reg(ctx.emitter);
    ctx.emitter.instruction(&format!("mov {}, #-1", neg_one));                    // materialize -1 for comparison
    ctx.emitter.instruction(&format!("cmp {}, {}", divisor_reg, neg_one));        // check whether the divisor is -1
    ctx.emitter.instruction(&format!("b.ne {}", int_min_label));                  // skip overflow check when the divisor is not -1
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch, i64::MIN);
    ctx.emitter.instruction(&format!("cmp {}, {}", dividend_reg, scratch));      // check whether the dividend is PHP_INT_MIN
    ctx.emitter.instruction(&format!("b.eq {}", overflow_label));                // branch to the overflow throw when INT_MIN / -1
    ctx.emitter.label(&int_min_label);
}

/// Emits the x86_64 overflow check for `intdiv(PHP_INT_MIN, -1)`.
/// Branches to `overflow_label` when `dividend == i64::MIN && divisor == -1`.
fn emit_intdiv_overflow_check_x86_64(
    ctx: &mut FunctionContext<'_>,
    dividend_reg: &str,
    divisor_reg: &str,
    overflow_label: &str,
) {
    let not_neg_one_label = ctx.next_label("intdiv_not_neg_one");
    ctx.emitter.instruction(&format!("cmp {}, -1", divisor_reg));                 // check whether the divisor is -1
    ctx.emitter.instruction(&format!("jne {}", not_neg_one_label));               // skip overflow check when the divisor is not -1
    ctx.emitter.instruction(&format!("mov r10, 0x8000000000000000"));             // materialize i64::MIN (PHP_INT_MIN)
    ctx.emitter.instruction(&format!("cmp {}, r10", dividend_reg));               // check whether the dividend is PHP_INT_MIN
    ctx.emitter.instruction(&format!("je {}", overflow_label));                   // branch to the overflow throw when INT_MIN / -1
    ctx.emitter.label(&not_neg_one_label);
}

/// Emits a catchable `ArithmeticError` throw for `intdiv(PHP_INT_MIN, -1)`.
fn emit_intdiv_overflow_throw(ctx: &mut FunctionContext<'_>) {
    let (msg_label, msg_len) = ctx
        .data
        .add_string(b"Division of PHP_INT_MIN by -1 is not an integer");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #32");                                // request Throwable payload storage for the ArithmeticError
            ctx.emitter.instruction("bl __rt_heap_alloc");                         // allocate the ArithmeticError object payload
            ctx.emitter.instruction("mov x9, #6");                                  // heap kind 6 marks an object instance allocation
            ctx.emitter.instruction("str x9, [x0, #-8]");                           // stamp the allocation header as a runtime object
            abi::emit_symbol_address(ctx.emitter, "x9", "_spl_arithmetic_error_class_id");
            ctx.emitter.instruction("ldr x9, [x9]");                                // load ArithmeticError's runtime class id for this program
            ctx.emitter.instruction("str x9, [x0]");                                // store the ArithmeticError class id in the Throwable header
            abi::emit_symbol_address(ctx.emitter, "x9", &msg_label);               // resolve the ArithmeticError message address
            ctx.emitter.instruction("str x9, [x0, #8]");                            // store the static ArithmeticError message pointer
            ctx.emitter.instruction(&format!("mov x9, #{}", msg_len));              // materialize the static ArithmeticError message length
            ctx.emitter.instruction("str x9, [x0, #16]");                           // store the exception message length
            ctx.emitter.instruction("str xzr, [x0, #24]");                          // store the default zero exception code
            abi::emit_symbol_address(ctx.emitter, "x9", "_exc_value");
            ctx.emitter.instruction("str x0, [x9]");                               // publish the active ArithmeticError object
            ctx.emitter.instruction("b __rt_throw_current");                        // enter the standard exception unwinder
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rbp");                                    // preserve caller frame pointer for exception allocation
            ctx.emitter.instruction("mov rbp, rsp");                                // establish an aligned helper frame for heap allocation
            ctx.emitter.instruction("sub rsp, 16");                                 // keep the nested heap allocation call 16-byte aligned
            ctx.emitter.instruction("mov rax, 32");                                  // request Throwable payload storage for the ArithmeticError
            ctx.emitter.instruction("call __rt_heap_alloc");                        // allocate the ArithmeticError object payload
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 6)); // materialize the x86_64 object heap-kind header
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");                 // stamp the allocation header as a runtime object
            ctx.emitter.instruction("mov r10, QWORD PTR [rip + _spl_arithmetic_error_class_id]"); // load ArithmeticError's runtime class id for this program
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                    // store the ArithmeticError class id in the Throwable header
            ctx.emitter.instruction(&format!("lea r10, [rip + {}]", msg_label));    // materialize the static ArithmeticError message pointer
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store the static ArithmeticError message pointer
            ctx.emitter.instruction(&format!("mov QWORD PTR [rax + 16], {}", msg_len)); // store the exception message length
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");                  // store the default zero exception code
            ctx.emitter.instruction("mov QWORD PTR [rip + _exc_value], rax");        // publish the active ArithmeticError object
            ctx.emitter.instruction("mov rsp, rbp");                                 // release the helper frame before throwing
            ctx.emitter.instruction("pop rbp");                                      // restore caller frame pointer before throwing
            ctx.emitter.instruction("jmp __rt_throw_current");                      // enter the standard exception unwinder
        }
    }
}

/// Loads a numeric operand and normalizes values into the integer result register.
fn load_numeric_as_int(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    name: &str,
) -> Result<()> {
    match ctx.load_value_to_result(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => Ok(()),
        PhpType::TaggedScalar => {
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            Ok(())
        }
        PhpType::Float => {
            abi::emit_float_result_to_int_result(ctx.emitter);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            name, other
        ))),
    }
}
