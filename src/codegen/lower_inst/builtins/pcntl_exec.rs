//! Purpose:
//! Lowers `pcntl_exec()` into staged target-aware bridge calls over PHP array storage.
//!
//! Called from:
//! - `super::pcntl::lower()` for the typed `PcntlRuntime::Exec` operation.
//!
//! Key details:
//! - Indexed and associative string arrays are traversed in PHP insertion order.
//! - The third argument's presence selects `execve`; omission preserves the host environment.

use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::strings::load_value_as_string_to_regs;
use super::{ensure_arg_count_between, expect_operand, store_if_result};
use super::pcntl::{emit_pcntl_last_error_warning, PCNTL_WARNING_EXEC};

const BUILDER_OFFSET: usize = 0;
const CONTAINER_OFFSET: usize = 8;
const CURSOR_OFFSET: usize = 16;
const INDEX_OFFSET: usize = 24;
const EXEC_FRAME_BYTES: usize = 64;

/// Lowers one process-replacing `pcntl_exec()` call; only the failure path returns false.
pub(super) fn lower_exec(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_exec", 1, 3)?;
    let conversion_failure = ctx.next_label("pcntl_exec_conversion_failure");
    let failure = ctx.next_label("pcntl_exec_failure");
    let os_failure = ctx.next_label("pcntl_exec_os_failure");
    abi::emit_reserve_temporary_stack(ctx.emitter, EXEC_FRAME_BYTES);
    let path = expect_operand(inst, 0)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_value_as_string_to_regs(ctx, path, "pcntl_exec path", "x0", "x1")?;
            abi::emit_load_int_immediate(ctx.emitter, "x2", i64::from(inst.operands.len() == 3));
        }
        Arch::X86_64 => {
            load_value_as_string_to_regs(ctx, path, "pcntl_exec path", "rdi", "rsi")?;
            abi::emit_load_int_immediate(ctx.emitter, "rdx", i64::from(inst.operands.len() == 3));
        }
    }
    ctx.emitter.bl_c("elephc_pcntl_exec_new");
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        BUILDER_OFFSET,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbz x0, {failure}")),
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {failure}"));
        }
    }
    if let Some(arguments) = inst.operands.get(1).copied() {
        emit_string_array(ctx, arguments, false, &conversion_failure)?;
    }
    if let Some(environment) = inst.operands.get(2).copied() {
        emit_string_array(ctx, environment, true, &conversion_failure)?;
    }
    load_builder_argument(ctx);
    ctx.emitter.bl_c("elephc_pcntl_exec_run");
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {os_failure}")),
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {os_failure}")),
    }

    ctx.emitter.label(&conversion_failure);
    load_builder_argument(ctx);
    ctx.emitter.bl_c("elephc_pcntl_exec_free");
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {failure}")),
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {failure}")),
    }
    ctx.emitter.label(&os_failure);
    emit_pcntl_last_error_warning(ctx, PCNTL_WARNING_EXEC);
    ctx.emitter.label(&failure);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_release_temporary_stack(ctx.emitter, EXEC_FRAME_BYTES);
    store_if_result(ctx, inst)
}

/// Traverses one indexed or associative string array and stages argv or envp entries.
fn emit_string_array(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    environment: bool,
    failure: &str,
) -> Result<()> {
    let ty = ctx.value_php_type(value)?.codegen_repr();
    match ty {
        PhpType::Array(element) if matches!(*element, PhpType::Never | PhpType::Void) => Ok(()),
        PhpType::Array(element) if matches!(*element, PhpType::Str) => {
            emit_indexed_string_array(ctx, value, environment, failure)
        }
        PhpType::AssocArray { value: element, .. }
            if matches!(*element, PhpType::Str | PhpType::Never | PhpType::Void) =>
        {
            emit_assoc_string_array(ctx, value, environment, failure)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "pcntl_exec array storage {other:?}",
        ))),
    }
}

/// Stages values from one packed PHP string array, using numeric environment keys when needed.
fn emit_indexed_string_array(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    environment: bool,
    failure: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("pcntl_exec_indexed_loop");
    let done = ctx.next_label("pcntl_exec_indexed_done");
    ctx.load_value_to_result(value)?;
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CONTAINER_OFFSET,
    );
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        INDEX_OFFSET,
    );
    ctx.emitter.label(&loop_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", CONTAINER_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", INDEX_OFFSET);
            ctx.emitter.instruction("ldr x11, [x9]");                           // load the packed array's logical length
            ctx.emitter.instruction("cmp x10, x11");                            // compare the current index with the element count
            ctx.emitter.instruction(&format!("b.ge {done}"));                   // finish after the final packed entry
            ctx.emitter.instruction("lsl x11, x10, #4");                        // string slots occupy sixteen bytes
            ctx.emitter.instruction("add x11, x9, x11");                        // advance to the selected string slot
            ctx.emitter.instruction("add x11, x11, #24");                       // skip the packed-array header
            ctx.emitter.instruction("ldp x3, x4, [x11]");                       // load the borrowed value pointer and length
            if environment {
                ctx.emitter.instruction("mov x1, x10");                         // numeric environment key = packed index
                ctx.emitter.instruction("mov x2, #-1");                         // key-high -1 marks an integer key
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", BUILDER_OFFSET);
                ctx.emitter.bl_c("elephc_pcntl_exec_add_env");
            } else {
                ctx.emitter.instruction("mov x1, x3");                          // argv value pointer
                ctx.emitter.instruction("mov x2, x4");                          // argv value length
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", BUILDER_OFFSET);
                ctx.emitter.bl_c("elephc_pcntl_exec_add_arg");
            }
            ctx.emitter.instruction(&format!("cbz x0, {failure}"));             // abort if copying or NUL validation failed
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x10", INDEX_OFFSET);
            ctx.emitter.instruction("add x10, x10, #1");                        // advance to the next packed entry
            abi::emit_store_to_sp(ctx.emitter, "x10", INDEX_OFFSET);
            ctx.emitter.instruction(&format!("b {loop_label}"));
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r9", CONTAINER_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", INDEX_OFFSET);
            ctx.emitter.instruction("mov r11, QWORD PTR [r9]");                 // load the packed array's logical length
            ctx.emitter.instruction("cmp r10, r11");                            // compare the current index with the element count
            ctx.emitter.instruction(&format!("jge {done}"));                    // finish after the final packed entry
            ctx.emitter.instruction("mov r11, r10");                            // copy the index before scaling it
            ctx.emitter.instruction("shl r11, 4");                              // string slots occupy sixteen bytes
            ctx.emitter.instruction("add r11, r9");                             // advance to the selected string slot
            ctx.emitter.instruction("add r11, 24");                             // skip the packed-array header
            ctx.emitter.instruction("mov rcx, QWORD PTR [r11]");                // load the borrowed value pointer
            ctx.emitter.instruction("mov r8, QWORD PTR [r11 + 8]");             // load the borrowed value length
            if environment {
                ctx.emitter.instruction("mov rsi, r10");                        // numeric environment key = packed index
                ctx.emitter.instruction("mov rdx, -1");                         // key-high -1 marks an integer key
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", BUILDER_OFFSET);
                ctx.emitter.bl_c("elephc_pcntl_exec_add_env");
            } else {
                ctx.emitter.instruction("mov rsi, rcx");                        // argv value pointer
                ctx.emitter.instruction("mov rdx, r8");                         // argv value length
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", BUILDER_OFFSET);
                ctx.emitter.bl_c("elephc_pcntl_exec_add_arg");
            }
            ctx.emitter.instruction("test rax, rax");                           // did the bridge copy this entry?
            ctx.emitter.instruction(&format!("jz {failure}"));                  // abort if copying or NUL validation failed
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", INDEX_OFFSET);
            ctx.emitter.instruction("add r10, 1");                              // advance to the next packed entry
            abi::emit_store_to_sp(ctx.emitter, "r10", INDEX_OFFSET);
            ctx.emitter.instruction(&format!("jmp {loop_label}"));
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Stages insertion-ordered values and keys from one associative PHP string array.
fn emit_assoc_string_array(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    environment: bool,
    failure: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("pcntl_exec_assoc_loop");
    let done = ctx.next_label("pcntl_exec_assoc_done");
    ctx.load_value_to_result(value)?;
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CONTAINER_OFFSET,
    );
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_store_to_sp(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        CURSOR_OFFSET,
    );
    ctx.emitter.label(&loop_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", CONTAINER_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", CURSOR_OFFSET);
            abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
            ctx.emitter.instruction("cmp x0, #-1");                             // has insertion-order iteration completed?
            ctx.emitter.instruction(&format!("b.eq {done}"));                   // finish after the terminal cursor
            abi::emit_store_to_sp(ctx.emitter, "x0", CURSOR_OFFSET);
            if environment {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", BUILDER_OFFSET);
                ctx.emitter.bl_c("elephc_pcntl_exec_add_env");
            } else {
                ctx.emitter.instruction("mov x1, x3");                          // argv value pointer from the hash iterator
                ctx.emitter.instruction("mov x2, x4");                          // argv value length from the hash iterator
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", BUILDER_OFFSET);
                ctx.emitter.bl_c("elephc_pcntl_exec_add_arg");
            }
            ctx.emitter.instruction(&format!("cbz x0, {failure}"));             // abort if copying or NUL validation failed
            ctx.emitter.instruction(&format!("b {loop_label}"));
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", CONTAINER_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", CURSOR_OFFSET);
            abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
            ctx.emitter.instruction("cmp rax, -1");                             // has insertion-order iteration completed?
            ctx.emitter.instruction(&format!("je {done}"));                     // finish after the terminal cursor
            abi::emit_store_to_sp(ctx.emitter, "rax", CURSOR_OFFSET);
            if environment {
                ctx.emitter.instruction("mov rsi, rdi");                        // move the iterator key into bridge key-low
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", BUILDER_OFFSET);
                ctx.emitter.bl_c("elephc_pcntl_exec_add_env");
            } else {
                ctx.emitter.instruction("mov rsi, rcx");                        // argv value pointer from the hash iterator
                ctx.emitter.instruction("mov rdx, r8");                         // argv value length from the hash iterator
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", BUILDER_OFFSET);
                ctx.emitter.bl_c("elephc_pcntl_exec_add_arg");
            }
            ctx.emitter.instruction("test rax, rax");                           // did the bridge copy this entry?
            ctx.emitter.instruction(&format!("jz {failure}"));                  // abort if copying or NUL validation failed
            ctx.emitter.instruction(&format!("jmp {loop_label}"));
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Loads the staged builder into the target's first C argument register.
fn load_builder_argument(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", BUILDER_OFFSET)
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", BUILDER_OFFSET)
        }
    }
}
