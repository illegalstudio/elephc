//! Purpose:
//! Lowers PHP diagnostic output builtins for the EIR backend.
//! Handles concrete scalar/resource values and array/hash shells without recursive dumps.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Output must match the legacy backend for the supported concrete types.
//! - Mixed dispatch follows the runtime tag/payload contract from `__rt_mixed_unbox`.
//! - Iterable and full object class-name dispatch stay explicit unsupported work.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `print_r(value)` for concrete scalar/resource values and array/hash shells.
pub(super) fn lower_print_r(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "print_r", 1)?;
    ctx.emitter.blank();
    ctx.emitter.comment("print_r()");
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?.codegen_repr();
    emit_print_r_loaded_value(ctx, &ty)?;
    store_if_result(ctx, inst)
}

/// Lowers `var_dump(value)` for concrete scalar/resource values and array/hash shells.
pub(super) fn lower_var_dump(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "var_dump", 1)?;
    ctx.emitter.blank();
    ctx.emitter.comment("var_dump()");
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?.codegen_repr();
    match &ty {
        PhpType::Int => emit_var_dump_int(ctx),
        PhpType::Float => emit_var_dump_float(ctx),
        PhpType::Str => emit_var_dump_string(ctx),
        PhpType::Bool => emit_var_dump_bool(ctx),
        PhpType::Resource(_) => emit_var_dump_resource(ctx),
        PhpType::Void | PhpType::Never => {
            emit_var_dump_null(ctx);
            Ok(())
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => emit_var_dump_array(ctx),
        PhpType::Mixed | PhpType::Union(_) => emit_var_dump_mixed(ctx),
        other => Err(CodegenIrError::unsupported(format!(
            "var_dump for PHP type {:?}",
            other
        ))),
    }?;
    store_if_result(ctx, inst)
}

/// Emits `print_r` output for the value currently loaded in result register(s).
fn emit_print_r_loaded_value(ctx: &mut FunctionContext<'_>, ty: &PhpType) -> Result<()> {
    match ty {
        PhpType::Void | PhpType::Never => Ok(()),
        PhpType::Bool => {
            let skip_label = ctx.next_label("print_r_skip_false");
            abi::emit_branch_if_int_result_zero(ctx.emitter, &skip_label);
            abi::emit_write_stdout(ctx.emitter, ty);
            ctx.emitter.label(&skip_label);
            Ok(())
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emit_write_literal(ctx, b"Array\n");
            Ok(())
        }
        PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Resource(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_) => {
            abi::emit_write_stdout(ctx.emitter, ty);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "print_r for PHP type {:?}",
            other
        ))),
    }
}

/// Emits `var_dump` output for a boxed Mixed payload in the integer result register.
fn emit_var_dump_mixed(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let int_case = ctx.next_label("var_dump_mixed_int");
    let string_case = ctx.next_label("var_dump_mixed_string");
    let float_case = ctx.next_label("var_dump_mixed_float");
    let bool_case = ctx.next_label("var_dump_mixed_bool");
    let resource_case = ctx.next_label("var_dump_mixed_resource");
    let array_case = ctx.next_label("var_dump_mixed_array");
    let object_case = ctx.next_label("var_dump_mixed_object");
    let null_case = ctx.next_label("var_dump_mixed_null");
    let done = ctx.next_label("var_dump_mixed_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_on_mixed_tag(ctx, 0, &int_case);
    emit_branch_on_mixed_tag(ctx, 1, &string_case);
    emit_branch_on_mixed_tag(ctx, 2, &float_case);
    emit_branch_on_mixed_tag(ctx, 3, &bool_case);
    emit_branch_on_mixed_tag(ctx, 9, &resource_case);
    emit_branch_on_mixed_tag(ctx, 4, &array_case);
    emit_branch_on_mixed_tag(ctx, 5, &array_case);
    emit_branch_on_mixed_tag(ctx, 6, &object_case);
    abi::emit_jump(ctx.emitter, &null_case);

    ctx.emitter.label(&int_case);
    move_mixed_payload_to_int_result(ctx);
    emit_var_dump_int(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&string_case);
    move_mixed_payload_to_string_result(ctx);
    emit_var_dump_string(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&float_case);
    move_mixed_payload_to_float_result(ctx);
    emit_var_dump_float(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&bool_case);
    move_mixed_payload_to_int_result(ctx);
    emit_var_dump_bool(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&resource_case);
    move_mixed_payload_to_int_result(ctx);
    emit_var_dump_resource(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&array_case);
    move_mixed_payload_to_int_result(ctx);
    emit_var_dump_array(ctx)?;
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&object_case);
    emit_write_literal(ctx, b"object\n");
    abi::emit_jump(ctx.emitter, &done);

    ctx.emitter.label(&null_case);
    emit_var_dump_null(ctx);
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits `var_dump` output for an integer payload in the integer result register.
fn emit_var_dump_int(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let not_null = ctx.next_label("var_dump_not_null");
    let done = ctx.next_label("var_dump_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    let scratch_reg = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, scratch_reg, 0x7fff_ffff_ffff_fffe_u64 as i64);
    emit_compare_regs(ctx, result_reg, scratch_reg);
    emit_branch_if_ne(ctx, &not_null);
    emit_var_dump_null(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&not_null);
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_write_literal(ctx, b"int(");
    abi::emit_pop_reg(ctx.emitter, result_reg);
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    emit_write_current_string(ctx);
    emit_write_literal(ctx, b")\n");
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits `var_dump` output for a float payload in the floating result register.
fn emit_var_dump_float(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_call_label(ctx.emitter, "__rt_ftoa");
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_write_literal(ctx, b"float(");
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_write_current_string(ctx);
    emit_write_literal(ctx, b")\n");
    Ok(())
}

/// Emits `var_dump` output for a string payload in the string result register pair.
fn emit_var_dump_string(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_write_literal(ctx, b"string(");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp, #8]");                        // load the preserved string length for decimal formatting
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");            // load the preserved string length for decimal formatting
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    emit_write_current_string(ctx);
    emit_write_literal(ctx, b") \"");
    abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
    emit_write_current_string(ctx);
    emit_write_literal(ctx, b"\"\n");
    Ok(())
}

/// Emits `var_dump` output for a boolean payload in the integer result register.
fn emit_var_dump_bool(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let true_label = ctx.next_label("var_dump_true");
    let done = ctx.next_label("var_dump_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    emit_compare_reg_zero(ctx, result_reg);
    emit_branch_if_nonzero(ctx, &true_label);
    emit_write_literal(ctx, b"bool(false)\n");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&true_label);
    emit_write_literal(ctx, b"bool(true)\n");
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits `var_dump` output for a stream/generic resource payload.
fn emit_var_dump_resource(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_write_literal(ctx, b"resource(");
    abi::emit_pop_reg(ctx.emitter, result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add x0, x0, #1");                          // convert the resource payload into the displayed one-based id
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("add rax, 1");                              // convert the resource payload into the displayed one-based id
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    emit_write_current_string(ctx);
    emit_write_literal(ctx, b") of type (stream)\n");
    Ok(())
}

/// Emits `var_dump` output for null, void, or never payloads.
fn emit_var_dump_null(ctx: &mut FunctionContext<'_>) {
    emit_write_literal(ctx, b"NULL\n");
}

/// Emits `var_dump` output for an array/hash payload in the integer result register.
fn emit_var_dump_array(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result_reg);
    emit_write_literal(ctx, b"array(");
    abi::emit_pop_reg(ctx.emitter, result_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [x0]");                            // load the array or hash element count from the heap header
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // load the array or hash element count from the heap header
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    emit_write_current_string(ctx);
    emit_write_literal(ctx, b") {\n}\n");
    Ok(())
}

/// Writes a compile-time literal byte string to stdout.
fn emit_write_literal(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    emit_write_current_string(ctx);
}

/// Writes the current string result register pair to stdout.
fn emit_write_current_string(ctx: &mut FunctionContext<'_>) {
    abi::emit_write_stdout(ctx.emitter, &PhpType::Str);
}

/// Branches to `label` when the unboxed Mixed tag equals `tag`.
fn emit_branch_on_mixed_tag(ctx: &mut FunctionContext<'_>, tag: u8, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{}", tag));              // compare the unboxed Mixed runtime tag against this formatter case
            ctx.emitter.instruction(&format!("b.eq {}", label));                // branch to the matching Mixed formatter case
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", tag));              // compare the unboxed Mixed runtime tag against this formatter case
            ctx.emitter.instruction(&format!("je {}", label));                  // branch to the matching Mixed formatter case
        }
    }
}

/// Moves the unboxed Mixed low payload word into the integer result register.
fn move_mixed_payload_to_int_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // move the unboxed Mixed low payload into the integer result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed Mixed low payload into the integer result register
        }
    }
}

/// Moves the unboxed Mixed string payload words into the string result register pair.
fn move_mixed_payload_to_string_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {}
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the unboxed Mixed string pointer into the string result register
        }
    }
}

/// Moves the unboxed Mixed float bits into the floating-point result register.
fn move_mixed_payload_to_float_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov d0, x1");                             // reinterpret the unboxed Mixed payload bits as the float result
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("movq xmm0, rdi");                          // reinterpret the unboxed Mixed payload bits as the float result
        }
    }
}

/// Emits a comparison between two general-purpose registers.
fn emit_compare_regs(ctx: &mut FunctionContext<'_>, lhs: &str, rhs: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", lhs, rhs));          // compare two integer-like register payloads
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", lhs, rhs));          // compare two integer-like register payloads
        }
    }
}

/// Emits a comparison between a general-purpose register and zero.
fn emit_compare_reg_zero(ctx: &mut FunctionContext<'_>, reg: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", reg));               // compare the integer-like register payload against zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 0", reg));                // compare the integer-like register payload against zero
        }
    }
}

/// Emits a branch when the previous comparison was non-zero/non-equal.
fn emit_branch_if_nonzero(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b.ne {}", label));                // branch when the compared integer-like payload is non-zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jne {}", label));                 // branch when the compared integer-like payload is non-zero
        }
    }
}

/// Emits a branch when the previous comparison found different values.
fn emit_branch_if_ne(ctx: &mut FunctionContext<'_>, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("b.ne {}", label));                // branch when the compared register payloads are different
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("jne {}", label));                 // branch when the compared register payloads are different
        }
    }
}

/// Verifies that the builtin call has exactly the expected operand count.
fn ensure_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() == expected {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} args, got {}",
        name,
        expected,
        inst.operands.len()
    )))
}
