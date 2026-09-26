//! Purpose:
//! Shared string coercion, register placement, truthiness, and result helpers.
//!
//! Called from:
//! - Focused string builtin lowering modules and sibling IO lowerers.
//!
//! Key details:
//! - Mixed and scalar coercions preserve temporary ownership and target ABI placement.

use super::*;

/// Returns the target register holding string-result pointers.
pub(super) fn string_ptr_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rax",
    }
}

/// Returns the target register holding string-result lengths.
pub(super) fn string_len_reg(ctx: &FunctionContext<'_>) -> &'static str {
    match ctx.emitter.target.arch {
        Arch::AArch64 => "x2",
        Arch::X86_64 => "rdx",
    }
}

/// Loads the sole argument for a string-transform builtin into string result registers.
pub(in crate::codegen::lower_inst::builtins) fn load_single_string_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 1 arg, got {}",
            name,
            inst.operands.len()
        )));
    }
    let ptr_reg = string_ptr_reg(ctx);
    let len_reg = string_len_reg(ctx);
    load_string_arg_to_regs(ctx, inst, 0, name, ptr_reg, len_reg)
}
/// Preserves the trim source string while loading the explicit character mask.
pub(super) fn lower_trim_mask_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x1, [sp, #-16]!");                     // preserve the source string pointer while loading the trim mask
            ctx.emitter.instruction("str x2, [sp, #-16]!");                     // preserve the source string length while loading the trim mask
            load_string_arg_to_regs(ctx, inst, 1, name, "x1", "x2")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the trim-mask pointer as the secondary string argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the trim-mask length as the secondary string argument
            ctx.emitter.instruction("ldr x2, [sp], #16");                       // restore the source string length after loading the mask
            ctx.emitter.instruction("ldr x1, [sp], #16");                       // restore the source string pointer after loading the mask
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_arg_to_regs(ctx, inst, 1, name, "rax", "rdx")?;
            ctx.emitter.instruction("mov rdi, rax");                            // pass the trim-mask pointer as the secondary string argument
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the trim-mask length as the secondary string argument
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");
        }
    }
    Ok(())
}

/// Materializes two string operands into the runtime helper's target ABI registers.
pub(super) fn load_binary_string_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    if inst.operands.len() < 2 || inst.operands.len() > 3 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 2 or 3 args, got {}",
            name,
            inst.operands.len()
        )));
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 0, name, "x1", "x2")?;
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve the first string pointer and length while loading the second
            load_string_arg_to_regs(ctx, inst, 1, name, "x1", "x2")?;
            ctx.emitter.instruction("mov x3, x1");                              // pass the second string pointer as the secondary string argument
            ctx.emitter.instruction("mov x4, x2");                              // pass the second string length as the secondary string argument
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the first string pointer and length into primary argument registers
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 0, name, "rax", "rdx")?;
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_arg_to_regs(ctx, inst, 1, name, "rax", "rdx")?;
            ctx.emitter.instruction("mov rcx, rdx");                            // pass the second string length as the fourth SysV string argument
            ctx.emitter.instruction("mov rdx, rax");                            // pass the second string pointer as the third SysV string argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    Ok(())
}

/// Returns a string operand after validating the EIR builtin call shape.
pub(super) fn expect_string_operand(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    index: usize,
    name: &str,
) -> Result<ValueId> {
    let value = expect_operand(inst, index)?;
    let ty = ctx.value_php_type(value)?;
    if ty == PhpType::Str {
        return Ok(value);
    }
    Err(CodegenIrError::unsupported(format!(
        "{} arg {} for PHP type {:?}",
        name,
        index + 1,
        ty
    )))
}

/// Materializes a builtin argument as a PHP string in caller-selected registers.
pub(in crate::codegen::lower_inst::builtins) fn load_string_arg_to_regs(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    index: usize,
    name: &str,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let value = expect_operand(inst, index)?;
    load_value_as_string_to_regs(ctx, value, name, ptr_reg, len_reg)
}

/// Materializes an arbitrary EIR value as a PHP string in caller-selected registers.
pub(in crate::codegen::lower_inst::builtins) fn load_value_as_string_to_regs(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    name: &str,
    ptr_reg: &str,
    len_reg: &str,
) -> Result<()> {
    let raw_ty = ctx.value_php_type(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        ctx.load_value_to_result(value)?;
        abi::emit_call_label(ctx.emitter, "__rt_resource_to_string");
        move_string_result_to_regs(ctx, ptr_reg, len_reg);
        return Ok(());
    }
    let ty = raw_ty.codegen_repr();
    match ty {
        PhpType::Str => ctx.load_string_value_to_regs(value, ptr_reg, len_reg),
        PhpType::Int => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_itoa");
            move_string_result_to_regs(ctx, ptr_reg, len_reg);
            Ok(())
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_ftoa_coerce");
            move_string_result_to_regs(ctx, ptr_reg, len_reg);
            Ok(())
        }
        PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            emit_loaded_bool_string_result(ctx)?;
            move_string_result_to_regs(ctx, ptr_reg, len_reg);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            emit_empty_string_result(ctx);
            move_string_result_to_regs(ctx, ptr_reg, len_reg);
            Ok(())
        }
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            emit_loaded_tagged_scalar_string_result(ctx)?;
            move_string_result_to_regs(ctx, ptr_reg, len_reg);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_borrowed_string_to_regs(ctx, value)?;
            move_string_result_to_regs(ctx, ptr_reg, len_reg);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "{} string coercion for PHP type {:?}",
            name, other
        ))),
    }
}

/// Materializes a `Mixed`/union value as a borrowed PHP string for builtin arguments.
///
/// String payloads are borrowed directly from the boxed cell instead of being
/// persisted. Scalar payloads stringify into concat scratch storage, which is
/// reset by the usual request/function concat-base cleanup.
pub(super) fn emit_mixed_borrowed_string_to_regs(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_mixed_borrowed_string_aarch64(ctx, value),
        Arch::X86_64 => emit_mixed_borrowed_string_x86_64(ctx, value),
    }
}

/// Emits AArch64 borrowed string coercion for a boxed `Mixed` value.
pub(super) fn emit_mixed_borrowed_string_aarch64(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let from_int = ctx.next_label("mixed_arg_string_from_int");
    let from_string = ctx.next_label("mixed_arg_string_from_string");
    let from_float = ctx.next_label("mixed_arg_string_from_float");
    let from_bool = ctx.next_label("mixed_arg_string_from_bool");
    let false_bool = ctx.next_label("mixed_arg_string_false_bool");
    let done = ctx.next_label("mixed_arg_string_done");
    load_value_to_first_int_arg(ctx, value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp x0, #0");                                      // check whether the boxed argument is an integer payload
    ctx.emitter.instruction(&format!("b.eq {}", from_int));                     // stringify integer payloads through the concat-backed itoa helper
    ctx.emitter.instruction("cmp x0, #1");                                      // check whether the boxed argument already holds a string payload
    ctx.emitter.instruction(&format!("b.eq {}", from_string));                  // borrow string payloads directly from the boxed cell
    ctx.emitter.instruction("cmp x0, #2");                                      // check whether the boxed argument is a float payload
    ctx.emitter.instruction(&format!("b.eq {}", from_float));                   // stringify float payloads through the concat-backed ftoa helper
    ctx.emitter.instruction("cmp x0, #3");                                      // check whether the boxed argument is a boolean payload
    ctx.emitter.instruction(&format!("b.eq {}", from_bool));                    // stringify boolean payloads using PHP scalar rules
    ctx.emitter.instruction("mov x1, xzr");                                     // use an empty string pointer for null or unsupported boxed payloads
    ctx.emitter.instruction("mov x2, xzr");                                     // use zero length for null or unsupported boxed payloads
    ctx.emitter.instruction(&format!("b {}", done));                            // finish with the normalized empty string

    ctx.emitter.label(&from_int);
    ctx.emitter.instruction("mov x0, x1");                                      // pass the unboxed integer payload to itoa
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    ctx.emitter.instruction(&format!("b {}", done));                            // finish with the concat-backed integer string

    ctx.emitter.label(&from_string);
    ctx.emitter.instruction(&format!("b {}", done));                            // x1/x2 already hold the borrowed string payload

    ctx.emitter.label(&from_float);
    ctx.emitter.instruction("fmov d0, x1");                                     // move unboxed float bits into the FP argument register
    abi::emit_call_label(ctx.emitter, "__rt_ftoa_coerce");
    ctx.emitter.instruction(&format!("b {}", done));                            // finish with the concat-backed float string

    ctx.emitter.label(&from_bool);
    ctx.emitter.instruction(&format!("cbz x1, {}", false_bool));                // false stringifies to an empty string
    ctx.emitter.instruction("mov x0, x1");                                      // pass true as integer 1 to itoa
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    ctx.emitter.instruction(&format!("b {}", done));                            // finish with the concat-backed true string

    ctx.emitter.label(&false_bool);
    ctx.emitter.instruction("mov x1, xzr");                                     // false uses an empty string pointer
    ctx.emitter.instruction("mov x2, xzr");                                     // false uses zero string length

    ctx.emitter.label(&done);
    Ok(())
}

/// Emits x86_64 borrowed string coercion for a boxed `Mixed` value.
pub(super) fn emit_mixed_borrowed_string_x86_64(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let from_int = ctx.next_label("mixed_arg_string_from_int");
    let from_string = ctx.next_label("mixed_arg_string_from_string");
    let from_float = ctx.next_label("mixed_arg_string_from_float");
    let from_bool = ctx.next_label("mixed_arg_string_from_bool");
    let false_bool = ctx.next_label("mixed_arg_string_false_bool");
    let done = ctx.next_label("mixed_arg_string_done");
    load_value_to_first_int_arg(ctx, value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    ctx.emitter.instruction("cmp rax, 0");                                      // check whether the boxed argument is an integer payload
    ctx.emitter.instruction(&format!("je {}", from_int));                       // stringify integer payloads through the concat-backed itoa helper
    ctx.emitter.instruction("cmp rax, 1");                                      // check whether the boxed argument already holds a string payload
    ctx.emitter.instruction(&format!("je {}", from_string));                    // borrow string payloads directly from the boxed cell
    ctx.emitter.instruction("cmp rax, 2");                                      // check whether the boxed argument is a float payload
    ctx.emitter.instruction(&format!("je {}", from_float));                     // stringify float payloads through the concat-backed ftoa helper
    ctx.emitter.instruction("cmp rax, 3");                                      // check whether the boxed argument is a boolean payload
    ctx.emitter.instruction(&format!("je {}", from_bool));                      // stringify boolean payloads using PHP scalar rules
    ctx.emitter.instruction("xor eax, eax");                                    // use an empty string pointer for null or unsupported boxed payloads
    ctx.emitter.instruction("xor edx, edx");                                    // use zero length for null or unsupported boxed payloads
    ctx.emitter.instruction(&format!("jmp {}", done));                          // finish with the normalized empty string

    ctx.emitter.label(&from_int);
    ctx.emitter.instruction("mov rax, rdi");                                    // pass the unboxed integer payload to itoa
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // finish with the concat-backed integer string

    ctx.emitter.label(&from_string);
    ctx.emitter.instruction("mov rax, rdi");                                    // return the borrowed string pointer from the boxed payload
    ctx.emitter.instruction(&format!("jmp {}", done));                          // rdx already holds the borrowed string length

    ctx.emitter.label(&from_float);
    ctx.emitter.instruction("movq xmm0, rdi");                                  // move unboxed float bits into the FP argument register
    abi::emit_call_label(ctx.emitter, "__rt_ftoa_coerce");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // finish with the concat-backed float string

    ctx.emitter.label(&from_bool);
    ctx.emitter.instruction("test rdi, rdi");                                   // false stringifies to an empty string
    ctx.emitter.instruction(&format!("je {}", false_bool));                     // branch to the empty-string result for false
    ctx.emitter.instruction("mov rax, rdi");                                    // pass true as integer 1 to itoa
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    ctx.emitter.instruction(&format!("jmp {}", done));                          // finish with the concat-backed true string

    ctx.emitter.label(&false_bool);
    ctx.emitter.instruction("xor eax, eax");                                    // false uses an empty string pointer
    ctx.emitter.instruction("xor edx, edx");                                    // false uses zero string length

    ctx.emitter.label(&done);
    Ok(())
}

/// Converts the loaded boolean result to PHP string ABI registers.
pub(super) fn emit_loaded_bool_string_result(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let false_label = ctx.next_label("bool_arg_to_str_false");
    let done_label = ctx.next_label("bool_arg_to_str_done");
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
    emit_empty_string_result(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Converts the loaded tagged scalar result to PHP string ABI registers.
pub(super) fn emit_loaded_tagged_scalar_string_result(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let null_label = ctx.next_label("tagged_arg_to_str_null");
    let done_label = ctx.next_label("tagged_arg_to_str_done");
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &null_label);
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    emit_empty_string_result(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Materializes a valid empty PHP string in the target ABI string-result registers.
pub(super) fn emit_empty_string_result(ctx: &mut FunctionContext<'_>) {
    let (label, _) = ctx.data.add_string(b"");
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
}

/// Moves the target ABI string result pair into caller-selected registers when needed.
pub(super) fn move_string_result_to_regs(ctx: &mut FunctionContext<'_>, ptr_reg: &str, len_reg: &str) {
    let (result_ptr_reg, result_len_reg) = abi::string_result_regs(ctx.emitter);
    if ptr_reg != result_ptr_reg {
        ctx.emitter.instruction(
            &format!("mov {}, {}", ptr_reg, result_ptr_reg)
        );                                                                      // move the cast string pointer into the requested argument register
    }
    if len_reg != result_len_reg {
        ctx.emitter.instruction(
            &format!("mov {}, {}", len_reg, result_len_reg)
        );                                                                      // move the cast string length into the requested argument register
    }
}

/// Materializes an optional PHP truthiness flag into the integer result register.
pub(in crate::codegen::lower_inst::builtins) fn materialize_truthy_flag(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    index: usize,
    name: &str,
) -> Result<()> {
    if inst.operands.len() <= index {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        return Ok(());
    }
    let value = expect_operand(inst, index)?;
    let raw_ty = ctx.raw_value_php_type(value)?;
    if matches!(raw_ty, PhpType::Resource(_)) {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        return Ok(());
    }
    match raw_ty.codegen_repr() {
        PhpType::Bool | PhpType::Int | PhpType::Pointer(_) => {
            ctx.load_value_to_result(value)?;
            predicates::emit_int_result_nonzero_bool(ctx);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            predicates::emit_float_result_nonzero_bool(ctx);
        }
        PhpType::Str => {
            predicates::emit_string_truthiness(ctx, value)?;
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            predicates::emit_array_truthiness(ctx, value)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_bool");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "{} truthiness flag for PHP type {:?}",
                name,
                other
            )))
        }
    }
    Ok(())
}
