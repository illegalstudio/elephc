//! Purpose:
//! Lowers PHP `array_keys()` calls for the Phase 04 EIR backend.
//! Handles indexed arrays and associative hashes with integer, string, or mixed keys.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::arrays::lower_array_keys()`.
//!
//! Key details:
//! - Associative keys are collected in insertion order through `__rt_hash_iter_next`.
//! - String keys are persisted before storing them in the result indexed array.
//! - Mixed key arrays box each normalized int/string key into an owned Mixed cell.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::{expect_operand, store_if_result};

/// Lowers `array_keys()` for indexed arrays and associative arrays.
pub(super) fn lower_array_keys(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_keys", 1)?;
    let array = expect_operand(inst, 0)?;
    let array_ty = ctx.value_php_type(array)?;
    let result_elem_ty = result_array_element_type(&inst.result_php_type.codegen_repr())?;
    match array_ty.codegen_repr() {
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {
            lower_dynamic_mixed_array_keys(ctx, inst, array, &result_elem_ty)
        }
        PhpType::Array(_) => lower_indexed_array_keys(ctx, inst, array, &result_elem_ty),
        PhpType::AssocArray { key, .. } => {
            lower_assoc_array_keys(ctx, inst, array, &key.codegen_repr(), &result_elem_ty)
        }
        other => Err(CodegenIrError::unsupported(format!(
            "array_keys for PHP type {:?}",
            other
        ))),
    }
}

/// Lowers `array_keys()` for a PHP `array<mixed>` value that may hold indexed or hash storage.
fn lower_dynamic_mixed_array_keys(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    result_elem_ty: &PhpType,
) -> Result<()> {
    require_supported_indexed_result_type(result_elem_ty)?;
    require_supported_assoc_result_type(&PhpType::Mixed, result_elem_ty)?;
    ctx.load_value_to_result(array)?;
    let assoc_label = ctx.next_label("akeys_dynamic_assoc");
    let done_label = ctx.next_label("akeys_dynamic_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp x0, #3");                              // detect associative hash storage hidden behind PHP array<mixed>
            ctx.emitter.instruction(&format!("b.eq {}", assoc_label));          // use insertion-order hash keys when the runtime payload is a hash
            abi::emit_pop_reg(ctx.emitter, "x0");
            lower_indexed_array_keys_aarch64(ctx, result_elem_ty)?;
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the hash-key path after indexed key materialization
            ctx.emitter.label(&assoc_label);
            abi::emit_pop_reg(ctx.emitter, "x0");
            lower_assoc_array_keys_aarch64(ctx, &PhpType::Mixed, result_elem_ty)?;
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp rax, 3");                              // detect associative hash storage hidden behind PHP array<mixed>
            ctx.emitter.instruction(&format!("je {}", assoc_label));            // use insertion-order hash keys when the runtime payload is a hash
            abi::emit_pop_reg(ctx.emitter, "rax");
            lower_indexed_array_keys_x86_64(ctx, result_elem_ty)?;
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the hash-key path after indexed key materialization
            ctx.emitter.label(&assoc_label);
            abi::emit_pop_reg(ctx.emitter, "rax");
            lower_assoc_array_keys_x86_64(ctx, &PhpType::Mixed, result_elem_ty)?;
        }
    }
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers indexed-array `array_keys()` to a freshly allocated `[0, 1, ...]` array.
fn lower_indexed_array_keys(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    result_elem_ty: &PhpType,
) -> Result<()> {
    require_supported_indexed_result_type(result_elem_ty)?;
    ctx.load_value_to_result(array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_indexed_array_keys_aarch64(ctx, result_elem_ty),
        Arch::X86_64 => lower_indexed_array_keys_x86_64(ctx, result_elem_ty),
    }?;
    store_if_result(ctx, inst)
}

/// Emits AArch64 indexed-array key materialization.
fn lower_indexed_array_keys_aarch64(
    ctx: &mut FunctionContext<'_>,
    result_elem_ty: &PhpType,
) -> Result<()> {
    let elem_size = indexed_key_element_size(result_elem_ty);
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load the source indexed-array length for exact result allocation
    ctx.emitter.instruction("str x9, [sp, #-16]!");                             // preserve the source length for the fill loop and final length stamp
    ctx.emitter.instruction("mov x0, x9");                                      // pass the source length as the result array capacity
    ctx.emitter.instruction(&format!("mov x1, #{}", elem_size));                // choose the indexed-array element size for the key payload representation
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(ctx.emitter, "x0", result_elem_ty);
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("str xzr, [sp, #-16]!");                            // push the initial indexed-key fill counter

    let loop_label = ctx.next_label("akeys_loop");
    let end_label = ctx.next_label("akeys_end");
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("ldr x12, [sp]");                                   // load the current indexed-key fill counter
    ctx.emitter.instruction("ldr x9, [sp, #32]");                               // reload the source indexed-array length from the fixed stack layout
    ctx.emitter.instruction("cmp x12, x9");                                     // check whether every integer key has been written
    ctx.emitter.instruction(&format!("b.ge {}", end_label));                    // finish once the keys array is fully materialized
    emit_indexed_key_payload_aarch64(ctx, result_elem_ty)?;
    ctx.emitter.instruction("ldr x12, [sp]");                                   // reload the fill counter after any key boxing helper calls
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next integer key
    ctx.emitter.instruction("str x12, [sp]");                                   // persist the updated fill counter for the next loop iteration
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue filling sequential integer keys

    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("add sp, sp, #16");                                 // drop the indexed-key fill counter stack slot
    ctx.emitter.instruction("ldr x0, [sp, #0]");                                // reload the result keys array pointer before finalizing it
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // reload the exact source indexed-array length
    ctx.emitter.instruction("str x9, [x0]");                                    // stamp the logical length of the result keys array
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // pop the result keys array pointer into the standard result register
    ctx.emitter.instruction("add sp, sp, #16");                                 // drop the preserved source indexed-array length
    Ok(())
}

/// Emits x86_64 indexed-array key materialization.
fn lower_indexed_array_keys_x86_64(
    ctx: &mut FunctionContext<'_>,
    result_elem_ty: &PhpType,
) -> Result<()> {
    let elem_size = indexed_key_element_size(result_elem_ty);
    ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                        // load the source indexed-array length for exact result allocation
    ctx.emitter.instruction("sub rsp, 16");                                     // reserve a temporary slot for the source indexed-array length
    ctx.emitter.instruction("mov QWORD PTR [rsp], r10");                        // preserve the source length for the fill loop and final length stamp
    ctx.emitter.instruction("mov rdi, r10");                                    // pass the source length as the result array capacity
    ctx.emitter.instruction(&format!("mov rsi, {}", elem_size));                // choose the indexed-array element size for the key payload representation
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(ctx.emitter, "rax", result_elem_ty);
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("sub rsp, 16");                                     // reserve a temporary slot for the indexed-key fill counter
    ctx.emitter.instruction("mov QWORD PTR [rsp], 0");                          // initialize the indexed-key fill counter to zero

    let loop_label = ctx.next_label("akeys_loop");
    let end_label = ctx.next_label("akeys_end");
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                        // load the current indexed-key fill counter
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 32]");                   // reload the source indexed-array length from the fixed stack layout
    ctx.emitter.instruction("cmp r10, r11");                                    // check whether every integer key has been written
    ctx.emitter.instruction(&format!("jge {}", end_label));                     // finish once the keys array is fully materialized
    emit_indexed_key_payload_x86_64(ctx, result_elem_ty)?;
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                        // reload the fill counter after any key boxing helper calls
    ctx.emitter.instruction("add r10, 1");                                      // advance to the next integer key
    ctx.emitter.instruction("mov QWORD PTR [rsp], r10");                        // persist the updated fill counter for the next loop iteration
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue filling sequential integer keys

    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("add rsp, 16");                                     // drop the indexed-key fill counter stack slot
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                        // reload the result keys array pointer before finalizing it
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                   // reload the exact source indexed-array length
    ctx.emitter.instruction("mov QWORD PTR [rax], r10");                        // stamp the logical length of the result keys array
    ctx.emitter.instruction("add rsp, 16");                                     // drop the preserved result keys array pointer
    ctx.emitter.instruction("add rsp, 16");                                     // drop the preserved source indexed-array length
    Ok(())
}

/// Emits one AArch64 indexed-array key payload in the result array's element representation.
fn emit_indexed_key_payload_aarch64(ctx: &mut FunctionContext<'_>, result_elem_ty: &PhpType) -> Result<()> {
    match result_elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // load the result keys array pointer from the fixed stack layout
            ctx.emitter.instruction("add x10, x0, #24");                        // point at the result keys payload region after the array header
            ctx.emitter.instruction("str x12, [x10, x12, lsl #3]");             // store the loop counter as the next integer key
        }
        PhpType::Mixed => {
            ctx.emitter.instruction("mov x1, x12");                             // use the loop counter as the integer mixed-key payload
            ctx.emitter.instruction("mov x2, xzr");                             // integer mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer mixed key
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            emit_append_word_key_aarch64(ctx, "x0");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_keys indexed result PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Emits one x86_64 indexed-array key payload in the result array's element representation.
fn emit_indexed_key_payload_x86_64(ctx: &mut FunctionContext<'_>, result_elem_ty: &PhpType) -> Result<()> {
    match result_elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");           // load the result keys array pointer from the fixed stack layout
            ctx.emitter.instruction("mov QWORD PTR [rcx + r10 * 8 + 24], r10"); // store the loop counter as the next integer key
        }
        PhpType::Mixed => {
            ctx.emitter.instruction("mov rdi, r10");                            // use the loop counter as the integer mixed-key payload
            ctx.emitter.instruction("xor esi, esi");                            // integer mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // runtime tag 0 = integer mixed key
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            emit_append_word_key_x86_64(ctx, "rax");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_keys indexed result PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Lowers associative-array `array_keys()` by copying normalized keys into a new indexed array.
fn lower_assoc_array_keys(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    key_ty: &PhpType,
    result_elem_ty: &PhpType,
) -> Result<()> {
    require_supported_assoc_result_type(key_ty, result_elem_ty)?;
    ctx.load_value_to_result(array)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_assoc_array_keys_aarch64(ctx, key_ty, result_elem_ty),
        Arch::X86_64 => lower_assoc_array_keys_x86_64(ctx, key_ty, result_elem_ty),
    }?;
    store_if_result(ctx, inst)
}

/// Emits AArch64 associative-array key extraction in insertion order.
fn lower_assoc_array_keys_aarch64(
    ctx: &mut FunctionContext<'_>,
    key_ty: &PhpType,
    result_elem_ty: &PhpType,
) -> Result<()> {
    abi::emit_push_reg(ctx.emitter, "x0");
    let elem_size = indexed_key_element_size(result_elem_ty);
    ctx.emitter.instruction("ldr x0, [x0]");                                    // load the associative-array entry count to size the keys result exactly
    ctx.emitter.instruction(&format!("mov x1, #{}", elem_size));                // choose the indexed-array element size for the key payload representation
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(ctx.emitter, "x0", result_elem_ty);
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("str xzr, [sp, #-16]!");                            // push iter_cursor = 0 to start hash insertion-order iteration

    let loop_label = ctx.next_label("akeys_assoc_loop");
    let end_label = ctx.next_label("akeys_assoc_end");
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("ldr x0, [sp, #32]");                               // load the associative-array hash-table pointer for the next iteration step
    ctx.emitter.instruction("ldr x1, [sp]");                                    // load the current associative-array iterator cursor
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
    ctx.emitter.instruction("cmn x0, #1");                                      // check whether hash iteration reached the done sentinel
    ctx.emitter.instruction(&format!("b.eq {}", end_label));                    // finish after collecting every associative-array key
    ctx.emitter.instruction("str x0, [sp]");                                    // save the updated iterator cursor for the next loop step
    emit_assoc_key_append_aarch64(ctx, key_ty, result_elem_ty)?;
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue collecting associative-array keys

    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("add sp, sp, #16");                                 // drop the associative-array iterator cursor stack slot
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // pop the result keys array pointer into the standard result register
    ctx.emitter.instruction("add sp, sp, #16");                                 // drop the preserved associative-array hash-table pointer
    Ok(())
}

/// Emits x86_64 associative-array key extraction in insertion order.
fn lower_assoc_array_keys_x86_64(
    ctx: &mut FunctionContext<'_>,
    key_ty: &PhpType,
    result_elem_ty: &PhpType,
) -> Result<()> {
    abi::emit_push_reg(ctx.emitter, "rax");
    let elem_size = indexed_key_element_size(result_elem_ty);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rax]");                        // load the associative-array entry count to size the keys result exactly
    ctx.emitter.instruction(&format!("mov rsi, {}", elem_size));                // choose the indexed-array element size for the key payload representation
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(ctx.emitter, "rax", result_elem_ty);
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("sub rsp, 16");                                     // reserve one temporary stack slot for the hash iterator cursor
    ctx.emitter.instruction("mov QWORD PTR [rsp], 0");                          // initialize the iterator cursor to the hash-header head sentinel

    let loop_label = ctx.next_label("akeys_assoc_loop");
    let end_label = ctx.next_label("akeys_assoc_end");
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                   // load the associative-array hash-table pointer for the next iteration step
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp]");                        // load the current associative-array iterator cursor
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
    ctx.emitter.instruction("cmp rax, -1");                                     // check whether hash iteration reached the done sentinel
    ctx.emitter.instruction(&format!("je {}", end_label));                      // finish after collecting every associative-array key
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // save the updated iterator cursor for the next loop step
    emit_assoc_key_append_x86_64(ctx, key_ty, result_elem_ty)?;
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue collecting associative-array keys

    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("add rsp, 16");                                     // drop the associative-array iterator cursor stack slot
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                        // move the result keys array pointer into the standard result register
    ctx.emitter.instruction("add rsp, 16");                                     // drop the preserved result keys array pointer
    ctx.emitter.instruction("add rsp, 16");                                     // drop the preserved associative-array hash-table pointer
    Ok(())
}

/// Appends the current AArch64 hash iterator key into the result keys array.
fn emit_assoc_key_append_aarch64(
    ctx: &mut FunctionContext<'_>,
    key_ty: &PhpType,
    result_elem_ty: &PhpType,
) -> Result<()> {
    match result_elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable if is_int_like_key_type(key_ty) => {
            emit_append_word_key_aarch64(ctx, "x1");
        }
        PhpType::Str if matches!(key_ty, PhpType::Str) => {
            ctx.emitter.instruction("stp x9, x10, [sp, #-16]!");                // preserve result array state across string-key persistence
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("ldp x9, x10, [sp], #16");                  // restore result array state after string-key persistence
            emit_append_string_key_aarch64(ctx, "x1", "x2");
        }
        PhpType::Mixed => {
            emit_assoc_mixed_key_append_aarch64(ctx, key_ty)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_keys associative key PHP type {:?} into result PHP type {:?}",
                key_ty,
                other
            )));
        }
    }
    Ok(())
}

/// Appends the current x86_64 hash iterator key into the result keys array.
fn emit_assoc_key_append_x86_64(
    ctx: &mut FunctionContext<'_>,
    key_ty: &PhpType,
    result_elem_ty: &PhpType,
) -> Result<()> {
    match result_elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable if is_int_like_key_type(key_ty) => {
            emit_append_word_key_x86_64(ctx, "rdi");
        }
        PhpType::Str if matches!(key_ty, PhpType::Str) => {
            ctx.emitter.instruction("sub rsp, 16");                             // reserve a temporary slot for result array state during key persistence
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 32]");           // load the result keys array pointer from the fixed stack layout
            ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                // load the current result keys array length before persistence
            ctx.emitter.instruction("mov QWORD PTR [rsp], r10");                // preserve the result keys array pointer across key persistence
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r11");            // preserve the current result keys array length across key persistence
            ctx.emitter.instruction("mov rax, rdi");                            // move the borrowed string key pointer into the persist helper input
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                // restore the result keys array pointer after key persistence
            ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 8]");            // restore the result keys array length after key persistence
            ctx.emitter.instruction("add rsp, 16");                             // release the temporary result-array state slot
            emit_append_string_key_x86_64(ctx, "rax", "rdx");
        }
        PhpType::Mixed => {
            emit_assoc_mixed_key_append_x86_64(ctx, key_ty)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_keys associative key PHP type {:?} into result PHP type {:?}",
                key_ty,
                other
            )));
        }
    }
    Ok(())
}

/// Boxes the current AArch64 hash iterator key and appends the Mixed cell pointer.
fn emit_assoc_mixed_key_append_aarch64(ctx: &mut FunctionContext<'_>, key_ty: &PhpType) -> Result<()> {
    match key_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer mixed key
            ctx.emitter.instruction("mov x2, xzr");                             // integer mixed payloads do not use a high word
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            emit_append_word_key_aarch64(ctx, "x0");
        }
        PhpType::Str => {
            ctx.emitter.instruction("mov x0, #1");                              // runtime tag 1 = string mixed key
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            emit_append_word_key_aarch64(ctx, "x0");
        }
        PhpType::Mixed => {
            let key_string = ctx.next_label("akeys_assoc_key_string");
            let key_boxed = ctx.next_label("akeys_assoc_key_boxed");
            ctx.emitter.instruction("cmn x2, #1");                              // check whether this normalized hash key is an integer
            ctx.emitter.instruction(&format!("b.ne {}", key_string));           // branch to string-key boxing when key_hi is not the integer sentinel
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = integer mixed key
            ctx.emitter.instruction("mov x2, xzr");                             // integer mixed payloads do not use a high word
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", key_boxed));               // skip string-key boxing after producing an integer mixed key
            ctx.emitter.label(&key_string);
            ctx.emitter.instruction("mov x0, #1");                              // runtime tag 1 = string mixed key
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&key_boxed);
            emit_append_word_key_aarch64(ctx, "x0");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_keys associative key PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Boxes the current x86_64 hash iterator key and appends the Mixed cell pointer.
fn emit_assoc_mixed_key_append_x86_64(ctx: &mut FunctionContext<'_>, key_ty: &PhpType) -> Result<()> {
    match key_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            ctx.emitter.instruction("xor esi, esi");                            // integer mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 0");                              // runtime tag 0 = integer mixed key
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            emit_append_word_key_x86_64(ctx, "rax");
        }
        PhpType::Str => {
            ctx.emitter.instruction("mov rsi, rdx");                            // move the string key length into the mixed helper high-word register
            ctx.emitter.instruction("mov eax, 1");                              // runtime tag 1 = string mixed key
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            emit_append_word_key_x86_64(ctx, "rax");
        }
        PhpType::Mixed => {
            let key_string = ctx.next_label("akeys_assoc_key_string");
            let key_boxed = ctx.next_label("akeys_assoc_key_boxed");
            ctx.emitter.instruction("cmp rdx, -1");                             // check whether this normalized hash key is an integer
            ctx.emitter.instruction(&format!("jne {}", key_string));            // branch to string-key boxing when key_hi is not the integer sentinel
            ctx.emitter.instruction("xor esi, esi");                            // integer mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 0");                              // runtime tag 0 = integer mixed key
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", key_boxed));             // skip string-key boxing after producing an integer mixed key
            ctx.emitter.label(&key_string);
            ctx.emitter.instruction("mov rsi, rdx");                            // move the string key length into the mixed helper high-word register
            ctx.emitter.instruction("mov eax, 1");                              // runtime tag 1 = string mixed key
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&key_boxed);
            emit_append_word_key_x86_64(ctx, "rax");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array_keys associative key PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Appends a pointer-sized AArch64 key payload into the result keys array.
fn emit_append_word_key_aarch64(ctx: &mut FunctionContext<'_>, value_reg: &str) {
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // load the result keys array pointer from the fixed stack layout
    ctx.emitter.instruction("ldr x10, [x9]");                                   // load the current result keys array length before appending
    ctx.emitter.instruction("add x11, x9, #24");                                // point at the result keys payload region after the fixed header
    ctx.emitter.instruction(&format!("str {}, [x11, x10, lsl #3]", value_reg)); // store the key payload into the next result keys slot
    ctx.emitter.instruction("add x10, x10, #1");                                // increment the result keys length after the append
    ctx.emitter.instruction("str x10, [x9]");                                   // persist the updated result keys length in the array header
}

/// Appends a string AArch64 key payload into the result keys array.
fn emit_append_string_key_aarch64(ctx: &mut FunctionContext<'_>, ptr_reg: &str, len_reg: &str) {
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // load the result keys array pointer from the fixed stack layout
    ctx.emitter.instruction("ldr x10, [x9]");                                   // load the current result keys array length before appending
    ctx.emitter.instruction("lsl x11, x10, #4");                                // convert the result length into a 16-byte string-slot offset
    ctx.emitter.instruction("add x11, x9, x11");                                // advance from the array base to the selected string slot
    ctx.emitter.instruction("add x11, x11, #24");                               // skip the fixed array header to reach the string payload region
    ctx.emitter.instruction(&format!("str {}, [x11]", ptr_reg));                // store the owned key string pointer into the next result keys slot
    ctx.emitter.instruction(&format!("str {}, [x11, #8]", len_reg));            // store the owned key string length into the next result keys slot
    ctx.emitter.instruction("add x10, x10, #1");                                // increment the result keys length after the append
    ctx.emitter.instruction("str x10, [x9]");                                   // persist the updated result keys length in the array header
}

/// Appends a pointer-sized x86_64 key payload into the result keys array.
fn emit_append_word_key_x86_64(ctx: &mut FunctionContext<'_>, value_reg: &str) {
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                   // load the result keys array pointer from the fixed stack layout
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load the current result keys array length before appending
    ctx.emitter.instruction(&format!("mov QWORD PTR [r10 + r11 * 8 + 24], {}", value_reg)); //store the key payload into the next result keys slot
    ctx.emitter.instruction("add r11, 1");                                      // increment the result keys length after the append
    ctx.emitter.instruction("mov QWORD PTR [r10], r11");                        // persist the updated result keys length in the array header
}

/// Appends a string x86_64 key payload into the result keys array.
fn emit_append_string_key_x86_64(ctx: &mut FunctionContext<'_>, ptr_reg: &str, len_reg: &str) {
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                   // load the result keys array pointer from the fixed stack layout
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load the current result keys array length before appending
    ctx.emitter.instruction("mov rcx, r11");                                    // copy the result length before scaling it into a string-slot offset
    ctx.emitter.instruction("shl rcx, 4");                                      // convert the result length into a 16-byte string-slot offset
    ctx.emitter.instruction("add rcx, r10");                                    // advance from the array base to the selected string slot
    ctx.emitter.instruction("add rcx, 24");                                     // skip the fixed array header to reach the string payload region
    ctx.emitter.instruction(&format!("mov QWORD PTR [rcx], {}", ptr_reg));      // store the owned key string pointer into the next result keys slot
    ctx.emitter.instruction(&format!("mov QWORD PTR [rcx + 8], {}", len_reg));  // store the owned key string length into the next result keys slot
    ctx.emitter.instruction("add r11, 1");                                      // increment the result keys length after the append
    ctx.emitter.instruction("mov QWORD PTR [r10], r11");                        // persist the updated result keys length in the array header
}

/// Returns the indexed-array element width needed for this key type.
fn indexed_key_element_size(key_ty: &PhpType) -> usize {
    if matches!(key_ty, PhpType::Str) {
        16
    } else {
        8
    }
}

/// Returns the result array element type that controls the physical keys layout.
fn result_array_element_type(result_ty: &PhpType) -> Result<PhpType> {
    match result_ty {
        PhpType::Array(elem) => Ok(elem.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_keys result PHP type {:?}",
            other
        ))),
    }
}

/// Verifies indexed key extraction can materialize the requested result element representation.
fn require_supported_indexed_result_type(result_elem_ty: &PhpType) -> Result<()> {
    match result_elem_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Mixed => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_keys indexed result PHP type {:?}",
            other
        ))),
    }
}

/// Verifies associative key extraction supports both source key and result element layouts.
fn require_supported_assoc_result_type(key_ty: &PhpType, result_elem_ty: &PhpType) -> Result<()> {
    require_supported_source_key_type(key_ty)?;
    match result_elem_ty {
        PhpType::Mixed => Ok(()),
        PhpType::Str if matches!(key_ty, PhpType::Str) => Ok(()),
        PhpType::Int | PhpType::Bool | PhpType::Callable if is_int_like_key_type(key_ty) => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_keys associative key PHP type {:?} into result PHP type {:?}",
            key_ty,
            other
        ))),
    }
}

/// Verifies associative key extraction supports the statically known source key representation.
fn require_supported_source_key_type(key_ty: &PhpType) -> Result<()> {
    match key_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Str | PhpType::Mixed => Ok(()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_keys associative key PHP type {:?}",
            other
        ))),
    }
}

/// Returns true when a source hash key is represented as a normalized integer key.
fn is_int_like_key_type(key_ty: &PhpType) -> bool {
    matches!(key_ty, PhpType::Int | PhpType::Bool | PhpType::Callable)
}
