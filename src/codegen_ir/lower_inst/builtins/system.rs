//! Purpose:
//! Lowers date/time system builtins for the EIR backend.
//! Marshals already-evaluated EIR operands into the shared runtime helpers.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Time builtins are effectful and must reuse the target-aware runtime
//!   helpers rather than duplicating libc/syscall behavior in the EIR backend.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, load_value_to_first_int_arg, store_if_result};

/// Lowers `date(format, timestamp?)` through the shared formatter runtime helper.
pub(super) fn lower_date(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "date", 1, 2)?;
    let format = expect_operand(inst, 0)?;
    let timestamp = inst
        .operands
        .get(1)
        .copied();

    load_date_timestamp(ctx, timestamp)?;
    load_date_format(ctx, format)?;
    abi::emit_call_label(ctx.emitter, "__rt_date");
    store_if_result(ctx, inst)
}

/// Lowers `microtime()`/`microtime(true)` through the shared runtime helper.
pub(super) fn lower_microtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "microtime", 0, 1)?;
    abi::emit_call_label(ctx.emitter, "__rt_microtime");
    store_if_result(ctx, inst)
}

/// Lowers `mktime(hour, minute, second, month, day, year)` through the runtime helper.
pub(super) fn lower_mktime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "mktime", 6)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            materialize_integer_arg(ctx, expect_operand(inst, 0)?, "x0", "mktime hour")?;
            materialize_integer_arg(ctx, expect_operand(inst, 1)?, "x1", "mktime minute")?;
            materialize_integer_arg(ctx, expect_operand(inst, 2)?, "x2", "mktime second")?;
            materialize_integer_arg(ctx, expect_operand(inst, 3)?, "x3", "mktime month")?;
            materialize_integer_arg(ctx, expect_operand(inst, 4)?, "x4", "mktime day")?;
            materialize_integer_arg(ctx, expect_operand(inst, 5)?, "x5", "mktime year")?;
        }
        Arch::X86_64 => {
            materialize_integer_arg(ctx, expect_operand(inst, 0)?, "rdi", "mktime hour")?;
            materialize_integer_arg(ctx, expect_operand(inst, 1)?, "rsi", "mktime minute")?;
            materialize_integer_arg(ctx, expect_operand(inst, 2)?, "rdx", "mktime second")?;
            materialize_integer_arg(ctx, expect_operand(inst, 3)?, "rcx", "mktime month")?;
            materialize_integer_arg(ctx, expect_operand(inst, 4)?, "r8", "mktime day")?;
            materialize_integer_arg(ctx, expect_operand(inst, 5)?, "r9", "mktime year")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mktime");
    store_if_result(ctx, inst)
}

/// Lowers `sleep(seconds)` through the target's C library symbol.
pub(super) fn lower_sleep(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_blocking_c_call(ctx, inst, "sleep", "sleep seconds")
}

/// Lowers `strtotime(datetime)` through the shared parser runtime helper.
pub(super) fn lower_strtotime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "strtotime", 1)?;
    let datetime = expect_operand(inst, 0)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            require_string(ctx.value_php_type(datetime)?, "strtotime datetime")?;
            ctx.load_string_value_to_regs(datetime, "x1", "x2")?;
        }
        Arch::X86_64 => {
            require_string(ctx.value_php_type(datetime)?, "strtotime datetime")?;
            ctx.load_string_value_to_regs(datetime, "rdi", "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_strtotime");
    store_if_result(ctx, inst)
}

/// Lowers `time()` through the shared wall-clock runtime helper.
pub(super) fn lower_time(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "time", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_time");
    store_if_result(ctx, inst)
}

/// Lowers `usleep(microseconds)` through the target's C library symbol.
pub(super) fn lower_usleep(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_blocking_c_call(ctx, inst, "usleep", "usleep microseconds")
}

/// Lowers `getenv(name)` through the target-aware environment lookup helper.
pub(super) fn lower_getenv(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "getenv", 1)?;
    let name = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(name)?.codegen_repr(), "getenv name")?;
    abi::emit_call_label(ctx.emitter, "__rt_getenv");
    store_if_result(ctx, inst)
}

/// Lowers `putenv(assignment)` by copying the environment string into persistent heap storage.
pub(super) fn lower_putenv(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "putenv", 1)?;
    let assignment = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(assignment)?.codegen_repr(), "putenv assignment")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_putenv_aarch64(ctx),
        Arch::X86_64 => lower_putenv_x86_64(ctx),
    }
    store_if_result(ctx, inst)
}

/// Lowers `php_uname(mode?)` through the target-aware uname runtime helper.
pub(super) fn lower_php_uname(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "php_uname", 0, 1)?;
    if let Some(mode) = inst.operands.first().copied() {
        require_string(ctx.load_value_to_result(mode)?.codegen_repr(), "php_uname mode")?;
    } else {
        let (label, len) = ctx.data.add_string(b"a");
        let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
        abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    }
    abi::emit_call_label(ctx.emitter, "__rt_php_uname");
    store_if_result(ctx, inst)
}

/// Lowers `exec(command)` by capturing shell stdout through the shared runtime helper.
pub(super) fn lower_exec(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_shell_exec_like(ctx, inst, "exec")
}

/// Lowers `shell_exec(command)` by capturing shell stdout through the shared runtime helper.
pub(super) fn lower_shell_exec(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_shell_exec_like(ctx, inst, "shell_exec")
}

/// Lowers `system(command)` through libc `system()` and returns the legacy empty string result.
pub(super) fn lower_system(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_direct_system_call(ctx, inst, "system", true)
}

/// Lowers `passthru(command)` through libc `system()` for direct stdout passthrough.
pub(super) fn lower_passthru(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_direct_system_call(ctx, inst, "passthru", false)
}

/// Lowers shell-capturing process builtins that return a PHP string.
fn lower_shell_exec_like(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let command = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(command)?.codegen_repr(), "shell command")?;
    abi::emit_call_label(ctx.emitter, "__rt_shell_exec");
    store_if_result(ctx, inst)
}

/// Lowers stdout-passthrough process builtins that execute a command via libc `system()`.
fn lower_direct_system_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    returns_empty_string: bool,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let command = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(command)?.codegen_repr(), "system command")?;
    abi::emit_call_label(ctx.emitter, "__rt_cstr");
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the null-terminated shell command to libc system()
    }
    ctx.emitter.bl_c("system");
    if returns_empty_string {
        emit_empty_string_result(ctx);
    }
    store_if_result(ctx, inst)
}

/// Materializes the legacy empty-string return value used after `system()`.
fn emit_empty_string_result(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
}

/// Emits the AArch64 persistent-copy path for `putenv()`.
fn lower_putenv_aarch64(ctx: &mut FunctionContext<'_>) {
    let copy_loop = ctx.next_label("putenv_copy");
    let copy_done = ctx.next_label("putenv_copy_done");
    ctx.emitter.instruction("add x0, x2, #1");                                  // allocate space for the environment string plus trailing null
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the source string pointer and length across heap allocation
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the source string pointer and length after heap allocation
    ctx.emitter.instruction("mov x3, x0");                                      // keep the persistent destination buffer for copying and putenv()
    ctx.emitter.instruction("mov x4, #0");                                      // start copying at byte offset zero
    ctx.emitter.label(&copy_loop);
    ctx.emitter.instruction("cmp x4, x2");                                      // compare the copied byte count with the source length
    ctx.emitter.instruction(&format!("b.ge {}", copy_done));                    // finish once every source byte has been persisted
    ctx.emitter.instruction("ldrb w5, [x1, x4]");                               // load one byte from the source environment assignment
    ctx.emitter.instruction("strb w5, [x3, x4]");                               // copy the byte into the persistent putenv buffer
    ctx.emitter.instruction("add x4, x4, #1");                                  // advance to the next source byte
    ctx.emitter.instruction(&format!("b {}", copy_loop));                       // continue copying the environment assignment
    ctx.emitter.label(&copy_done);
    ctx.emitter.instruction("strb wzr, [x3, x4]");                              // append the C null terminator required by putenv()
    ctx.emitter.instruction("mov x0, x3");                                      // pass the persistent environment buffer to putenv()
    ctx.emitter.bl_c("putenv");
    ctx.emitter.instruction("cmp x0, #0");                                      // compare libc putenv() status against success
    ctx.emitter.instruction("cset x0, eq");                                     // return true when putenv() accepted the assignment
}

/// Emits the x86_64 persistent-copy path for `putenv()`.
fn lower_putenv_x86_64(ctx: &mut FunctionContext<'_>) {
    let copy_loop = ctx.next_label("putenv_copy");
    let copy_done = ctx.next_label("putenv_copy_done");
    ctx.emitter.instruction("sub rsp, 16");                                     // reserve aligned spill space for the source string across heap allocation
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // save the source environment string pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                    // save the source environment string length
    ctx.emitter.instruction("mov rax, rdx");                                    // seed the heap allocation size from the source length
    ctx.emitter.instruction("add rax, 1");                                      // allocate space for the environment string plus trailing null
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp]");                        // restore the source environment string pointer
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 8]");                     // restore the source environment string length
    ctx.emitter.instruction("add rsp, 16");                                     // release the temporary source string spill space
    ctx.emitter.instruction("mov r9, rax");                                     // keep the persistent destination buffer for copying and putenv()
    ctx.emitter.instruction("mov r10, 0");                                      // start copying at byte offset zero
    ctx.emitter.label(&copy_loop);
    ctx.emitter.instruction("cmp r10, r8");                                     // compare the copied byte count with the source length
    ctx.emitter.instruction(&format!("jae {}", copy_done));                     // finish once every source byte has been persisted
    ctx.emitter.instruction("mov r11b, BYTE PTR [rcx + r10]");                  // load one byte from the source environment assignment
    ctx.emitter.instruction("mov BYTE PTR [r9 + r10], r11b");                   // copy the byte into the persistent putenv buffer
    ctx.emitter.instruction("add r10, 1");                                      // advance to the next source byte
    ctx.emitter.instruction(&format!("jmp {}", copy_loop));                     // continue copying the environment assignment
    ctx.emitter.label(&copy_done);
    ctx.emitter.instruction("mov BYTE PTR [r9 + r10], 0");                      // append the C null terminator required by putenv()
    ctx.emitter.instruction("mov rdi, r9");                                     // pass the persistent environment buffer to putenv()
    ctx.emitter.bl_c("putenv");
    ctx.emitter.instruction("cmp rax, 0");                                      // compare libc putenv() status against success
    ctx.emitter.instruction("sete al");                                         // return true when putenv() accepted the assignment
    ctx.emitter.instruction("movzx rax, al");                                   // widen the boolean byte into the integer result register
}

/// Lowers a one-argument blocking libc call that receives an integer duration.
fn lower_unary_blocking_c_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    context: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let duration = expect_operand(inst, 0)?;
    require_integer_like(load_value_to_first_int_arg(ctx, duration)?, context)?;
    ctx.emitter.bl_c(name);
    store_if_result(ctx, inst)
}

/// Loads a `date()` timestamp or the `-1` current-time sentinel into the integer result register.
fn load_date_timestamp(
    ctx: &mut FunctionContext<'_>,
    timestamp: Option<ValueId>,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let Some(timestamp) = timestamp else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, -1);
        return Ok(());
    };
    match ctx.value_php_type(timestamp)? {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, result_reg, -1);
            Ok(())
        }
        ty => {
            require_integer_like(ty, "date timestamp")?;
            ctx.load_value_to_result(timestamp)?;
            Ok(())
        }
    }
}

/// Loads a `date()` format string into the runtime helper's string argument registers.
fn load_date_format(ctx: &mut FunctionContext<'_>, format: ValueId) -> Result<()> {
    require_string(ctx.value_php_type(format)?, "date format")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.load_string_value_to_regs(format, "x1", "x2"),
        Arch::X86_64 => ctx.load_string_value_to_regs(format, "rdi", "rsi"),
    }
}

/// Loads one integer-like runtime argument into a caller-selected register.
fn materialize_integer_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    reg: &str,
    context: &str,
) -> Result<()> {
    require_integer_like(ctx.load_value_to_reg(value, reg)?, context)
}

/// Verifies a value can be passed as a date/time integer option.
fn require_integer_like(ty: PhpType, context: &str) -> Result<()> {
    if matches!(ty, PhpType::Int | PhpType::Bool) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        context,
        ty
    )))
}

/// Verifies a value can be passed as a date/time string argument.
fn require_string(ty: PhpType, context: &str) -> Result<()> {
    if ty == PhpType::Str {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        context,
        ty
    )))
}

/// Verifies that the builtin call has between the expected lowered operand counts.
fn ensure_arg_count_between(
    inst: &Instruction,
    name: &str,
    min: usize,
    max: usize,
) -> Result<()> {
    if (min..=max).contains(&inst.operands.len()) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} to {} args, got {}",
        name,
        min,
        max,
        inst.operands.len()
    )))
}
