//! Purpose:
//! Lowers `pcntl_exec()` into staged target-aware bridge calls over PHP array storage.
//!
//! Called from:
//! - `super::pcntl::lower()` for the typed `PcntlRuntime::Exec` operation.
//!
//! Key details:
//! - Indexed and associative arrays are traversed in PHP insertion order and scalar values
//!   are coerced with PHP string rules before the bridge copies them.
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
const KEY_LOW_OFFSET: usize = 32;
const KEY_HIGH_OFFSET: usize = 40;
const EXEC_MIXED_CELL_OFFSET: usize = 48;
const EXEC_FRAME_BYTES: usize = 80;

/// Lowers one process-replacing `pcntl_exec()` call; only the failure path returns false.
pub(super) fn lower_exec(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "pcntl_exec", 1, 3)?;
    let input_failure = ctx.next_label("pcntl_exec_input_failure");
    let path_nul = ctx.next_label("pcntl_exec_path_nul");
    let argument_nul = ctx.next_label("pcntl_exec_argument_nul");
    let environment_name_nul = ctx.next_label("pcntl_exec_environment_name_nul");
    let environment_value_nul = ctx.next_label("pcntl_exec_environment_value_nul");
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
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbz x0, {input_failure}")), // classify path NUL separately from allocation failure
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // inspect builder allocation success
            ctx.emitter.instruction(&format!("jz {input_failure}"));            // classify path NUL separately from allocation failure
        }
    }
    if let Some(arguments) = inst.operands.get(1).copied() {
        emit_string_array(ctx, arguments, false, &input_failure)?;
    }
    if let Some(environment) = inst.operands.get(2).copied() {
        emit_string_array(ctx, environment, true, &input_failure)?;
    }
    load_builder_argument(ctx);
    ctx.emitter.bl_c("elephc_pcntl_exec_run");
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {os_failure}")),   // warn after the failed process replacement
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {os_failure}")),  // warn after the failed process replacement
    }

    ctx.emitter.label(&input_failure);
    load_builder_argument(ctx);
    ctx.emitter.bl_c("elephc_pcntl_exec_free");
    ctx.emitter.bl_c("elephc_pcntl_exec_input_error");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            for (classification, label) in [
                (1, &path_nul),
                (2, &argument_nul),
                (3, &environment_name_nul),
                (4, &environment_value_nul),
            ] {
                ctx.emitter.instruction(&format!("cmp w0, #{classification}")); // select PHP's precise embedded-NUL ValueError
                ctx.emitter.instruction(&format!("b.eq {label}"));              // throw for the matching source argument
            }
            ctx.emitter.instruction(&format!("b {failure}"));                   // non-validation conversion failures return false
        }
        Arch::X86_64 => {
            for (classification, label) in [
                (1, &path_nul),
                (2, &argument_nul),
                (3, &environment_name_nul),
                (4, &environment_value_nul),
            ] {
                ctx.emitter.instruction(&format!("cmp eax, {classification}")); // select PHP's precise embedded-NUL ValueError
                ctx.emitter.instruction(&format!("je {label}"));                // throw for the matching source argument
            }
            ctx.emitter.instruction(&format!("jmp {failure}"));                 // non-validation conversion failures return false
        }
    }
    emit_exec_input_value_error(
        ctx,
        &path_nul,
        "pcntl_exec(): Argument #1 ($path) must not contain any null bytes",
    );
    emit_exec_input_value_error(
        ctx,
        &argument_nul,
        "pcntl_exec(): Argument #2 ($args) individual argument must not contain null bytes",
    );
    emit_exec_input_value_error(
        ctx,
        &environment_name_nul,
        "pcntl_exec(): Argument #3 ($env_vars) name for environment variable must not contain null bytes",
    );
    emit_exec_input_value_error(
        ctx,
        &environment_value_nul,
        "pcntl_exec(): Argument #3 ($env_vars) value for environment variable must not contain null bytes",
    );
    ctx.emitter.label(&os_failure);
    emit_pcntl_last_error_warning(ctx, PCNTL_WARNING_EXEC);
    ctx.emitter.label(&failure);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_release_temporary_stack(ctx.emitter, EXEC_FRAME_BYTES);
    store_if_result(ctx, inst)
}

/// Releases the exec staging frame and throws one precise embedded-NUL `ValueError`.
fn emit_exec_input_value_error(ctx: &mut FunctionContext<'_>, label: &str, message: &str) {
    ctx.emitter.label(label);
    abi::emit_release_temporary_stack(ctx.emitter, EXEC_FRAME_BYTES);
    super::super::exceptions::emit_value_error(ctx, message);
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
        PhpType::Array(element) if matches!(*element, PhpType::Never) => Ok(()),
        PhpType::Array(element) if exec_array_value_type_supported(&element) => {
            emit_indexed_string_array(ctx, value, &element, environment, failure)
        }
        PhpType::AssocArray { value: element, .. } if matches!(*element, PhpType::Never) => Ok(()),
        PhpType::AssocArray { value: element, .. } if exec_array_value_type_supported(&element) => {
            emit_assoc_string_array(ctx, value, &element, environment, failure)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "pcntl_exec array storage {other:?}",
        ))),
    }
}

/// Returns whether one array storage type can follow PHP's scalar string conversion path.
fn exec_array_value_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Str
            | PhpType::Int
            | PhpType::Float
            | PhpType::Bool
            | PhpType::False
            | PhpType::Void
            | PhpType::TaggedScalar
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Stages values from one packed PHP string array, using numeric environment keys when needed.
fn emit_indexed_string_array(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    element: &PhpType,
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
            let shift = if matches!(element.codegen_repr(), PhpType::Str | PhpType::TaggedScalar) {
                4
            } else {
                3
            };
            ctx.emitter.instruction(&format!("lsl x11, x10, #{shift}"));        // scale by the selected packed element width
            ctx.emitter.instruction("add x11, x9, x11");                        // advance to the selected string slot
            ctx.emitter.instruction("add x11, x11, #24");                       // skip the packed-array header
            if element.codegen_repr() == PhpType::Str {
                ctx.emitter.instruction("ldp x3, x4, [x11]");                   // load the borrowed string pointer and length
            } else {
                ctx.emitter.instruction("ldr x3, [x11]");                       // load one scalar or boxed Mixed array slot
                if element.codegen_repr() == PhpType::TaggedScalar {
                    ctx.emitter.instruction("ldr x5, [x11, #8]");               // load the inline nullable-scalar runtime tag
                }
                emit_loaded_exec_value_as_string(ctx, value, element)?;
            }
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
            ctx.emitter.instruction(&format!("b {loop_label}"));                // stage the next packed entry
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r9", CONTAINER_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", INDEX_OFFSET);
            ctx.emitter.instruction("mov r11, QWORD PTR [r9]");                 // load the packed array's logical length
            ctx.emitter.instruction("cmp r10, r11");                            // compare the current index with the element count
            ctx.emitter.instruction(&format!("jge {done}"));                    // finish after the final packed entry
            ctx.emitter.instruction("mov r11, r10");                            // copy the index before scaling it
            let shift = if matches!(element.codegen_repr(), PhpType::Str | PhpType::TaggedScalar) {
                4
            } else {
                3
            };
            ctx.emitter.instruction(&format!("shl r11, {shift}"));              // scale by the selected packed element width
            ctx.emitter.instruction("add r11, r9");                             // advance to the selected string slot
            ctx.emitter.instruction("add r11, 24");                             // skip the packed-array header
            ctx.emitter.instruction("mov rcx, QWORD PTR [r11]");                // load the borrowed string pointer or scalar payload
            if element.codegen_repr() == PhpType::Str {
                ctx.emitter.instruction("mov r8, QWORD PTR [r11 + 8]");         // load the borrowed string length
            } else {
                if element.codegen_repr() == PhpType::TaggedScalar {
                    ctx.emitter.instruction("mov r9, QWORD PTR [r11 + 8]");     // load the inline nullable-scalar runtime tag
                }
                emit_loaded_exec_value_as_string(ctx, value, element)?;
            }
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
            ctx.emitter.instruction(&format!("jmp {loop_label}"));              // stage the next packed entry
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Stages insertion-ordered values and keys from one associative PHP string array.
fn emit_assoc_string_array(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    element: &PhpType,
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
                abi::emit_store_to_sp(ctx.emitter, "x1", KEY_LOW_OFFSET);
                abi::emit_store_to_sp(ctx.emitter, "x2", KEY_HIGH_OFFSET);
            }
            if element.codegen_repr() != PhpType::Str {
                if matches!(element.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
                    emit_exec_assoc_mixed_string(ctx, value)?;
                } else {
                    emit_loaded_exec_value_as_string(ctx, value, element)?;
                }
            }
            if environment {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", KEY_LOW_OFFSET);
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", KEY_HIGH_OFFSET);
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", BUILDER_OFFSET);
                ctx.emitter.bl_c("elephc_pcntl_exec_add_env");
            } else {
                ctx.emitter.instruction("mov x1, x3");                          // argv value pointer from the hash iterator
                ctx.emitter.instruction("mov x2, x4");                          // argv value length from the hash iterator
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", BUILDER_OFFSET);
                ctx.emitter.bl_c("elephc_pcntl_exec_add_arg");
            }
            ctx.emitter.instruction(&format!("cbz x0, {failure}"));             // abort if copying or NUL validation failed
            ctx.emitter.instruction(&format!("b {loop_label}"));                // stage the next insertion-ordered entry
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", CONTAINER_OFFSET);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", CURSOR_OFFSET);
            abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
            ctx.emitter.instruction("cmp rax, -1");                             // has insertion-order iteration completed?
            ctx.emitter.instruction(&format!("je {done}"));                     // finish after the terminal cursor
            abi::emit_store_to_sp(ctx.emitter, "rax", CURSOR_OFFSET);
            if environment {
                abi::emit_store_to_sp(ctx.emitter, "rdi", KEY_LOW_OFFSET);
                abi::emit_store_to_sp(ctx.emitter, "rdx", KEY_HIGH_OFFSET);
            }
            if element.codegen_repr() != PhpType::Str {
                if matches!(element.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
                    emit_exec_assoc_mixed_string(ctx, value)?;
                } else {
                    emit_loaded_exec_value_as_string(ctx, value, element)?;
                }
            }
            if environment {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", KEY_LOW_OFFSET);
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", KEY_HIGH_OFFSET);
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
            ctx.emitter.instruction(&format!("jmp {loop_label}"));              // stage the next insertion-ordered entry
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Converts a raw array value in `x3`/`rcx` (plus hash tag `x5`/`r9`) to string regs.
fn emit_loaded_exec_value_as_string(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    element: &PhpType,
) -> Result<()> {
    match element.codegen_repr() {
        PhpType::Int => emit_exec_int_string(ctx),
        PhpType::Float => emit_exec_float_string(ctx),
        PhpType::Bool | PhpType::False => emit_exec_bool_string(ctx),
        PhpType::Void => emit_exec_empty_string(ctx),
        PhpType::TaggedScalar => emit_exec_tagged_scalar_string(ctx),
        PhpType::Object(_) => emit_exec_object_string(ctx, value),
        PhpType::Mixed | PhpType::Union(_) => emit_exec_mixed_string(ctx, value),
        other => Err(CodegenIrError::unsupported(format!(
            "pcntl_exec scalar string coercion for {other:?}",
        ))),
    }
}

/// Converts an inline nullable integer, whose tag is in `x5`/`r9`, to PHP string registers.
fn emit_exec_tagged_scalar_string(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let null = ctx.next_label("pcntl_exec_tagged_null");
    let done = ctx.next_label("pcntl_exec_tagged_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x5, #8");                              // check whether the nullable scalar carries PHP null
            ctx.emitter.instruction(&format!("b.eq {null}"));                   // null stringifies to the empty string
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp r9, 8");                               // check whether the nullable scalar carries PHP null
            ctx.emitter.instruction(&format!("je {null}"));                     // null stringifies to the empty string
        }
    }
    emit_exec_int_string(ctx)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&null);
    emit_exec_empty_string(ctx)?;
    ctx.emitter.label(&done);
    Ok(())
}

/// Materializes PHP's empty-string conversion into the exec staging value registers.
fn emit_exec_empty_string(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let (label, _) = ctx.data.add_string(b"");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x3", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x4", 0);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rcx", &label);
            abi::emit_load_int_immediate(ctx.emitter, "r8", 0);
        }
    }
    Ok(())
}

/// Converts the loaded integer payload to the target's temporary string pair.
fn emit_exec_int_string(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x3"),                 // pass the loaded integer to the formatter
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rcx"),                // pass the loaded integer to the formatter
    }
    abi::emit_call_label(ctx.emitter, "__rt_itoa");
    move_exec_string_result(ctx);
    Ok(())
}

/// Converts the loaded float bits to the target's temporary string pair.
fn emit_exec_float_string(ctx: &mut FunctionContext<'_>) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("fmov d0, x3"),                // move the loaded float bits to the formatter register
        Arch::X86_64 => ctx.emitter.instruction("movq xmm0, rcx"),              // move the loaded float bits to the formatter register
    }
    abi::emit_call_label(ctx.emitter, "__rt_ftoa");
    move_exec_string_result(ctx);
    Ok(())
}

/// Converts the loaded boolean payload using PHP's `true => "1"`, `false => ""` rule.
fn emit_exec_bool_string(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let false_label = ctx.next_label("pcntl_exec_bool_false");
    let done = ctx.next_label("pcntl_exec_bool_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbz x3, {false_label}")), // false stringifies to the empty string
        Arch::X86_64 => {
            ctx.emitter.instruction("test rcx, rcx");                           // inspect the loaded boolean payload
            ctx.emitter.instruction(&format!("jz {false_label}"));              // false stringifies to the empty string
        }
    }
    emit_exec_int_string(ctx)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&false_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, xzr");                             // empty-string pointer sentinel
            ctx.emitter.instruction("mov x4, xzr");                             // empty-string byte length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor ecx, ecx");                            // empty-string pointer sentinel
            ctx.emitter.instruction("xor r8d, r8d");                            // empty-string byte length
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Converts one boxed Mixed array payload or throws when PHP cannot stringify its runtime tag.
fn emit_exec_mixed_string(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x3");                              // pass the borrowed Mixed cell to shared string dispatch
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rcx");                            // pass the borrowed Mixed cell to shared string dispatch
        }
    }
    super::super::conversions::emit_mixed_string_dispatch_from_result(
        ctx,
        value,
        super::super::conversions::MixedStringContextMode::Result,
    )?;
    move_exec_string_result(ctx);
    Ok(())
}

/// Converts one hash-iterator Mixed payload whose tag is already in `x5`/`r9`.
fn emit_exec_assoc_mixed_string(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let from_int = ctx.next_label("pcntl_exec_assoc_mixed_int");
    let from_string = ctx.next_label("pcntl_exec_assoc_mixed_string");
    let from_float = ctx.next_label("pcntl_exec_assoc_mixed_float");
    let from_bool = ctx.next_label("pcntl_exec_assoc_mixed_bool");
    let from_null = ctx.next_label("pcntl_exec_assoc_mixed_null");
    let from_object = ctx.next_label("pcntl_exec_assoc_mixed_object");
    let done = ctx.next_label("pcntl_exec_assoc_mixed_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            for (tag, label) in [(0, &from_int), (1, &from_string), (2, &from_float), (3, &from_bool), (6, &from_object), (8, &from_null)] {
                ctx.emitter.instruction(&format!("cmp x5, #{tag}"));            // compare the associative payload's runtime tag
                ctx.emitter.instruction(&format!("b.eq {label}"));              // dispatch its PHP string conversion
            }
        }
        Arch::X86_64 => {
            for (tag, label) in [(0, &from_int), (1, &from_string), (2, &from_float), (3, &from_bool), (6, &from_object), (8, &from_null)] {
                ctx.emitter.instruction(&format!("cmp r9, {tag}"));             // compare the associative payload's runtime tag
                ctx.emitter.instruction(&format!("je {label}"));                // dispatch its PHP string conversion
            }
        }
    }
    super::super::exceptions::emit_type_error(
        ctx,
        "pcntl_exec(): array value could not be converted to string",
    );
    ctx.emitter.label(&from_int);
    emit_exec_int_string(ctx)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&from_string);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&from_float);
    emit_exec_float_string(ctx)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&from_bool);
    emit_exec_bool_string(ctx)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&from_object);
    emit_exec_object_string(ctx, value)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&from_null);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, xzr");                             // null stringifies to an empty pointer sentinel
            ctx.emitter.instruction("mov x4, xzr");                             // null stringifies to zero bytes
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor ecx, ecx");                            // null stringifies to an empty pointer sentinel
            ctx.emitter.instruction("xor r8d, r8d");                            // null stringifies to zero bytes
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Stringifies a raw object payload by presenting a borrowed Mixed-shaped cell to shared dispatch.
fn emit_exec_object_string(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_temporary_stack_address(ctx.emitter, "x9", EXEC_MIXED_CELL_OFFSET);
            ctx.emitter.instruction("mov x10, #6");                             // runtime tag 6 = object
            ctx.emitter.instruction("str x10, [x9]");                           // stage the borrowed object tag
            ctx.emitter.instruction("str x3, [x9, #8]");                        // stage the borrowed object pointer
            ctx.emitter.instruction("str xzr, [x9, #16]");                      // objects have no high payload word
            ctx.emitter.instruction("mov x0, x9");                              // pass the borrowed Mixed-shaped cell to string dispatch
        }
        Arch::X86_64 => {
            abi::emit_temporary_stack_address(ctx.emitter, "r10", EXEC_MIXED_CELL_OFFSET);
            ctx.emitter.instruction("mov QWORD PTR [r10], 6");                  // runtime tag 6 = object
            ctx.emitter.instruction("mov QWORD PTR [r10 + 8], rcx");            // stage the borrowed object pointer
            ctx.emitter.instruction("mov QWORD PTR [r10 + 16], 0");             // objects have no high payload word
            ctx.emitter.instruction("mov rax, r10");                            // pass the borrowed Mixed-shaped cell to string dispatch
        }
    }
    super::super::conversions::emit_mixed_string_dispatch_from_result(
        ctx,
        value,
        super::super::conversions::MixedStringContextMode::Result,
    )?;
    move_exec_string_result(ctx);
    Ok(())
}

/// Moves the canonical string result into the bridge's value pointer/length registers.
fn move_exec_string_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, x1");                              // move the string result pointer to exec staging
            ctx.emitter.instruction("mov x4, x2");                              // move the string result length to exec staging
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rcx, rax");                            // move the string result pointer to exec staging
            ctx.emitter.instruction("mov r8, rdx");                             // move the string result length to exec staging
        }
    }
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
