//! Purpose:
//! Lowers PHP `array_values()` calls for the Phase 04 EIR backend.
//! Handles indexed aliases and associative hash-to-indexed value extraction.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::arrays::lower_array_values()`.
//!
//! Key details:
//! - Associative arrays are copied in insertion order using `__rt_hash_iter_next`.
//! - Refcounted payloads are retained before storing them in the result array.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::{expect_operand, store_if_result};

/// Lowers `array_values()` for indexed arrays as an alias or associative arrays as a new values array.
pub(super) fn lower_array_values(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::super::ensure_arg_count(inst, "array_values", 1)?;
    let array = expect_operand(inst, 0)?;
    let array_ty = ctx.value_php_type(array)?;
    match array_ty.codegen_repr() {
        PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {
            lower_dynamic_mixed_array_values(ctx, inst, array)
        }
        PhpType::Array(_) => {
            ctx.load_value_to_result(array)?;
            abi::emit_incref_if_refcounted(ctx.emitter, &array_ty);
            store_if_result(ctx, inst)
        }
        PhpType::AssocArray { value, .. } => lower_assoc_array_values(ctx, inst, array, &value.codegen_repr()),
        other => Err(CodegenIrError::unsupported(format!(
            "array_values for PHP type {:?}",
            other
        ))),
    }
}

/// Lowers `array_values()` for a PHP `array<mixed>` value that may hold indexed or hash storage.
fn lower_dynamic_mixed_array_values(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
) -> Result<()> {
    ctx.load_value_to_result(array)?;
    let assoc_label = ctx.next_label("avals_dynamic_assoc");
    let done_label = ctx.next_label("avals_dynamic_done");
    let mixed_array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp x0, #3");                              // detect associative hash storage hidden behind PHP array<mixed>
            ctx.emitter.instruction(&format!("b.eq {}", assoc_label));          // copy hash values when the runtime payload is a hash
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_incref_if_refcounted(ctx.emitter, &mixed_array_ty);
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the hash-value copy after retaining an indexed array
            ctx.emitter.label(&assoc_label);
            abi::emit_pop_reg(ctx.emitter, "x0");
            lower_assoc_array_values_aarch64(ctx, &PhpType::Mixed)?;
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
            ctx.emitter.instruction("cmp rax, 3");                              // detect associative hash storage hidden behind PHP array<mixed>
            ctx.emitter.instruction(&format!("je {}", assoc_label));            // copy hash values when the runtime payload is a hash
            abi::emit_pop_reg(ctx.emitter, "rax");
            abi::emit_incref_if_refcounted(ctx.emitter, &mixed_array_ty);
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the hash-value copy after retaining an indexed array
            ctx.emitter.label(&assoc_label);
            abi::emit_pop_reg(ctx.emitter, "rax");
            lower_assoc_array_values_x86_64(ctx, &PhpType::Mixed)?;
        }
    }
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Lowers associative-array `array_values()` by copying values into a new indexed array.
fn lower_assoc_array_values(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    array: ValueId,
    value_ty: &PhpType,
) -> Result<()> {
    ctx.load_value_to_result(array)?;
    emit_loaded_assoc_array_values(ctx, value_ty)?;
    store_if_result(ctx, inst)
}

/// Copies the currently loaded associative array values into a new indexed array.
pub(in crate::codegen_ir::lower_inst::builtins) fn emit_loaded_assoc_array_values(
    ctx: &mut FunctionContext<'_>,
    value_ty: &PhpType,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_assoc_array_values_aarch64(ctx, value_ty),
        Arch::X86_64 => lower_assoc_array_values_x86_64(ctx, value_ty),
    }
}

/// Emits AArch64 associative-array value extraction into a freshly allocated indexed array.
fn lower_assoc_array_values_aarch64(ctx: &mut FunctionContext<'_>, value_ty: &PhpType) -> Result<()> {
    abi::emit_push_reg(ctx.emitter, "x0");
    let elem_size = indexed_array_element_size(value_ty);
    ctx.emitter.instruction("ldr x0, [x0]");                                    // load the associative-array entry count to size the result values array exactly
    ctx.emitter.instruction(&format!("mov x1, #{}", elem_size));                // choose the indexed-array element size for the associative-array value layout
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    emit_indexed_array_value_type_stamp(ctx, "x0", value_ty);
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("str xzr, [sp, #-16]!");                            // push iter_cursor = 0 to start from the associative-array head slot

    let loop_label = ctx.next_label("avals_assoc_loop");
    let end_label = ctx.next_label("avals_assoc_end");
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("ldr x0, [sp, #32]");                               // load the associative-array hash-table pointer for the next iteration step
    ctx.emitter.instruction("ldr x1, [sp]");                                    // load the current associative-array iterator cursor
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
    ctx.emitter.instruction("cmn x0, #1");                                      // has associative-array iteration reached the done sentinel?
    ctx.emitter.instruction(&format!("b.eq {}", end_label));                    // stop once every associative-array value has been collected
    ctx.emitter.instruction("str x0, [sp]");                                    // save the updated associative-array iterator cursor for the next loop step
    emit_assoc_array_value_append_aarch64(ctx, value_ty)?;
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue collecting associative-array values until iteration completes

    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("add sp, sp, #16");                                 // drop the associative-array iterator cursor stack slot
    ctx.emitter.instruction("ldr x0, [sp], #16");                               // pop the result values array pointer into the standard result register
    ctx.emitter.instruction("add sp, sp, #16");                                 // drop the preserved associative-array hash-table pointer stack slot
    Ok(())
}

/// Emits x86_64 associative-array value extraction into a freshly allocated indexed array.
fn lower_assoc_array_values_x86_64(ctx: &mut FunctionContext<'_>, value_ty: &PhpType) -> Result<()> {
    abi::emit_push_reg(ctx.emitter, "rax");
    let elem_size = indexed_array_element_size(value_ty);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rax]");                        // load the associative-array entry count to size the result values array exactly
    ctx.emitter.instruction(&format!("mov rsi, {}", elem_size));                // choose the indexed-array element size for the associative-array value layout
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    emit_indexed_array_value_type_stamp(ctx, "rax", value_ty);
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("sub rsp, 16");                                     // reserve one temporary stack slot for the associative-array iterator cursor
    ctx.emitter.instruction("mov QWORD PTR [rsp], 0");                          // initialize the associative-array iterator cursor to the hash-header head sentinel

    let loop_label = ctx.next_label("avals_assoc_loop");
    let end_label = ctx.next_label("avals_assoc_end");
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                   // load the associative-array hash-table pointer for the next iteration step
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp]");                        // load the current associative-array iterator cursor
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
    ctx.emitter.instruction("cmp rax, -1");                                     // has associative-array iteration reached the done sentinel?
    ctx.emitter.instruction(&format!("je {}", end_label));                      // stop once every associative-array value has been collected
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // save the updated associative-array iterator cursor for the next loop step
    emit_assoc_array_value_append_x86_64(ctx, value_ty)?;
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue collecting associative-array values until iteration completes

    ctx.emitter.label(&end_label);
    ctx.emitter.instruction("add rsp, 16");                                     // drop the associative-array iterator cursor stack slot
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                        // move the result values array pointer into the standard result register
    ctx.emitter.instruction("add rsp, 16");                                     // drop the preserved result values array pointer after loading it
    ctx.emitter.instruction("add rsp, 16");                                     // drop the preserved associative-array hash-table pointer stack slot
    Ok(())
}

/// Appends the current AArch64 hash iterator value payload into the result values array.
fn emit_assoc_array_value_append_aarch64(ctx: &mut FunctionContext<'_>, value_ty: &PhpType) -> Result<()> {
    match value_ty {
        PhpType::Str => {
            ctx.emitter.instruction("mov x1, x3");                              // move the associative-array string pointer into the string-persist input register
            ctx.emitter.instruction("mov x2, x4");                              // move the associative-array string length into the string-persist input register
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            emit_append_string_value_aarch64(ctx, "x1", "x2");
        }
        PhpType::Mixed => {
            let reuse_box = ctx.next_label("avals_assoc_reuse_mixed");
            let store_box = ctx.next_label("avals_assoc_store_mixed");
            ctx.emitter.instruction("cmp x5, #7");                              // does this associative-array entry already store a boxed mixed value?
            ctx.emitter.instruction(&format!("b.eq {}", reuse_box));            // reuse existing mixed boxes instead of nesting them
            crate::codegen::emit_box_runtime_payload_as_mixed(ctx.emitter, "x5", "x3", "x4");
            ctx.emitter.instruction(&format!("b {}", store_box));               // skip the mixed-box reuse path once boxing is done
            ctx.emitter.label(&reuse_box);
            ctx.emitter.instruction("mov x0, x3");                              // move the existing mixed box pointer into the incref helper input register
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.label(&store_box);
            emit_append_word_value_aarch64(ctx, "x0");
        }
        other => {
            if other.is_refcounted() {
                ctx.emitter.instruction("mov x0, x3");                          // move the borrowed heap pointer into the incref helper input register
                abi::emit_call_label(ctx.emitter, "__rt_incref");
                emit_append_word_value_aarch64(ctx, "x0");
            } else if is_supported_assoc_array_value(other) {
                emit_append_word_value_aarch64(ctx, "x3");
            } else {
                return Err(CodegenIrError::unsupported(format!(
                    "array_values associative value PHP type {:?}",
                    other
                )));
            }
        }
    }
    Ok(())
}

/// Appends the current x86_64 hash iterator value payload into the result values array.
fn emit_assoc_array_value_append_x86_64(ctx: &mut FunctionContext<'_>, value_ty: &PhpType) -> Result<()> {
    match value_ty {
        PhpType::Str => {
            ctx.emitter.instruction("mov rax, rcx");                            // move the associative-array string pointer into the string-persist input register
            ctx.emitter.instruction("mov rdx, r8");                             // move the associative-array string length into the string-persist input register
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            emit_append_string_value_x86_64(ctx, "rax", "rdx");
        }
        PhpType::Mixed => {
            let reuse_box = ctx.next_label("avals_assoc_reuse_mixed");
            let store_box = ctx.next_label("avals_assoc_store_mixed");
            ctx.emitter.instruction("cmp r9, 7");                               // does this associative-array entry already store a boxed mixed value?
            ctx.emitter.instruction(&format!("je {}", reuse_box));              // reuse existing mixed boxes instead of nesting them
            crate::codegen::emit_box_runtime_payload_as_mixed(ctx.emitter, "r9", "rcx", "r8");
            ctx.emitter.instruction(&format!("jmp {}", store_box));             // skip the mixed-box reuse path once boxing is done
            ctx.emitter.label(&reuse_box);
            ctx.emitter.instruction("mov rax, rcx");                            // move the existing mixed box pointer into the incref helper input register
            abi::emit_call_label(ctx.emitter, "__rt_incref");
            ctx.emitter.label(&store_box);
            emit_append_word_value_x86_64(ctx, "rax");
        }
        other => {
            if other.is_refcounted() {
                ctx.emitter.instruction("mov rax, rcx");                        // move the borrowed heap pointer into the incref helper input register
                abi::emit_call_label(ctx.emitter, "__rt_incref");
                emit_append_word_value_x86_64(ctx, "rax");
            } else if is_supported_assoc_array_value(other) {
                emit_append_word_value_x86_64(ctx, "rcx");
            } else {
                return Err(CodegenIrError::unsupported(format!(
                    "array_values associative value PHP type {:?}",
                    other
                )));
            }
        }
    }
    Ok(())
}

/// Appends a 16-byte string payload into the AArch64 result values array.
fn emit_append_string_value_aarch64(ctx: &mut FunctionContext<'_>, ptr_reg: &str, len_reg: &str) {
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // load the result values array pointer from the fixed stack layout
    ctx.emitter.instruction("ldr x10, [x9]");                                   // load the current result values array length before appending
    ctx.emitter.instruction("lsl x11, x10, #4");                                // convert the result length into a 16-byte string-slot offset
    ctx.emitter.instruction("add x11, x9, x11");                                // advance from the array base to the selected string slot
    ctx.emitter.instruction("add x11, x11, #24");                               // skip the fixed indexed-array header to reach the payload region
    ctx.emitter.instruction(&format!("str {}, [x11]", ptr_reg));                // store the owned string pointer into the next result values slot
    ctx.emitter.instruction(&format!("str {}, [x11, #8]", len_reg));            // store the owned string length into the next result values slot
    ctx.emitter.instruction("add x10, x10, #1");                                // increment the result values array length after the append
    ctx.emitter.instruction("str x10, [x9]");                                   // persist the updated result values array length in the header
}

/// Appends an 8-byte payload into the AArch64 result values array.
fn emit_append_word_value_aarch64(ctx: &mut FunctionContext<'_>, value_reg: &str) {
    ctx.emitter.instruction("ldr x9, [sp, #16]");                               // load the result values array pointer from the fixed stack layout
    ctx.emitter.instruction("ldr x10, [x9]");                                   // load the current result values array length before appending
    ctx.emitter.instruction("add x11, x9, #24");                                // point at the result values array payload region after the fixed header
    ctx.emitter.instruction(&format!("str {}, [x11, x10, lsl #3]", value_reg)); // store the value payload into the next result values slot
    ctx.emitter.instruction("add x10, x10, #1");                                // increment the result values array length after the append
    ctx.emitter.instruction("str x10, [x9]");                                   // persist the updated result values array length in the header
}

/// Appends a 16-byte string payload into the x86_64 result values array.
fn emit_append_string_value_x86_64(ctx: &mut FunctionContext<'_>, ptr_reg: &str, len_reg: &str) {
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                   // load the result values array pointer from the fixed stack layout
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load the current result values array length before appending
    ctx.emitter.instruction("mov rcx, r11");                                    // copy the result length before scaling it into a string-slot offset
    ctx.emitter.instruction("shl rcx, 4");                                      // convert the result length into a 16-byte string-slot offset
    ctx.emitter.instruction("add rcx, r10");                                    // advance from the array base to the selected string slot
    ctx.emitter.instruction("add rcx, 24");                                     // skip the fixed indexed-array header to reach the payload region
    ctx.emitter.instruction(&format!("mov QWORD PTR [rcx], {}", ptr_reg));      // store the owned string pointer into the next result values slot
    ctx.emitter.instruction(&format!("mov QWORD PTR [rcx + 8], {}", len_reg));  // store the owned string length into the next result values slot
    ctx.emitter.instruction("add r11, 1");                                      // increment the result values array length after the append
    ctx.emitter.instruction("mov QWORD PTR [r10], r11");                        // persist the updated result values array length in the header
}

/// Appends an 8-byte payload into the x86_64 result values array.
fn emit_append_word_value_x86_64(ctx: &mut FunctionContext<'_>, value_reg: &str) {
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                   // load the result values array pointer from the fixed stack layout
    ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                        // load the current result values array length before appending
    ctx.emitter.instruction(&format!("mov QWORD PTR [r10 + r11 * 8 + 24], {}", value_reg)); // store the value payload into the next result values slot
    ctx.emitter.instruction("add r11, 1");                                      // increment the result values array length after the append
    ctx.emitter.instruction("mov QWORD PTR [r10], r11");                        // persist the updated result values array length in the header
}

/// Returns the indexed-array element slot width required for values of this PHP type.
fn indexed_array_element_size(value_ty: &PhpType) -> usize {
    if matches!(value_ty, PhpType::Str) {
        16
    } else {
        8
    }
}

/// Writes the runtime value_type tag into an indexed-array heap header.
fn emit_indexed_array_value_type_stamp(ctx: &mut FunctionContext<'_>, array_reg: &str, value_ty: &PhpType) {
    let Some(value_type_tag) = indexed_array_value_type_tag(value_ty) else {
        return;
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr x10, [{}, #-8]", array_reg)); // load the packed array kind word from the heap header
            ctx.emitter.instruction("mov x12, #0x80ff");                        // preserve the indexed-array kind and persistent COW flag
            ctx.emitter.instruction("and x10, x10, x12");                       // keep only the persistent indexed-array metadata bits
            ctx.emitter.instruction(&format!("mov x11, #{}", value_type_tag));  // materialize the runtime array value_type tag
            ctx.emitter.instruction("lsl x11, x11, #8");                        // move the value_type tag into the packed kind-word byte lane
            ctx.emitter.instruction("orr x10, x10, x11");                       // combine the heap kind with the array value_type tag
            ctx.emitter.instruction(&format!("str x10, [{}, #-8]", array_reg)); // persist the packed array kind word in the heap header
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "r12");
            ctx.emitter.instruction(&format!("mov r10, QWORD PTR [{} - 8]", array_reg)); // load the packed array kind word from the heap header
            ctx.emitter.instruction("mov r12, 0xffffffff000080ff");             // materialize the heap-kind preservation mask without clobbering the array base
            ctx.emitter.instruction("and r10, r12");                            // preserve heap magic plus indexed-array metadata bits
            ctx.emitter.instruction(&format!("mov r12, {}", value_type_tag));   // materialize the runtime array value_type tag
            ctx.emitter.instruction("shl r12, 8");                              // move the value_type tag into the packed kind-word byte lane
            ctx.emitter.instruction("or r10, r12");                             // combine the preserved heap kind with the stamped value_type tag
            ctx.emitter.instruction(&format!("mov QWORD PTR [{} - 8], r10", array_reg)); // persist the packed array kind word in the heap header
            abi::emit_pop_reg(ctx.emitter, "r12");
        }
    }
}

/// Returns the runtime array value_type tag for indexed array payload metadata.
fn indexed_array_value_type_tag(value_ty: &PhpType) -> Option<i64> {
    match value_ty {
        PhpType::Float => Some(2),
        PhpType::Bool => Some(3),
        PhpType::Str => Some(1),
        PhpType::Array(_) => Some(4),
        PhpType::AssocArray { .. } => Some(5),
        PhpType::Object(_) => Some(6),
        PhpType::Mixed | PhpType::Union(_) => Some(7),
        PhpType::Void => Some(8),
        _ => None,
    }
}

/// Returns true when associative `array_values()` can append this value representation.
fn is_supported_assoc_array_value(value_ty: &PhpType) -> bool {
    matches!(
        value_ty,
        PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Float | PhpType::Void | PhpType::Never
    ) || value_ty.is_refcounted()
}
