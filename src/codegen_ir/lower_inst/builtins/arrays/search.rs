//! Purpose:
//! Lowers associative-array membership and search builtins for the EIR backend.
//! Scans hash entries in insertion order and compares stored values against a
//! scalar or string needle.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::arrays`.
//!
//! Key details:
//! - `AssocArray<Mixed>` entries are boxed Mixed cells in EIR, so the search
//!   path must unbox entry payloads before comparing concrete runtime tags.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::ValueId;
use crate::types::PhpType;

use super::super::super::super::context::FunctionContext;
use super::runtime_value_tag;

/// Attempts to lower `array_search()` for an associative-array operand.
pub(super) fn try_lower_assoc_array_search(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    needle_ty: PhpType,
    array_ty: PhpType,
) -> Result<bool> {
    let PhpType::AssocArray { value, .. } = array_ty.codegen_repr() else {
        return Ok(false);
    };
    let needle_ty = needle_ty.codegen_repr();
    let value_ty = value.codegen_repr();
    require_assoc_search_value("array_search", &needle_ty, &value_ty)?;
    lower_assoc_array_search(ctx, needle, array, &needle_ty, &value_ty)?;
    Ok(true)
}

/// Attempts to lower `in_array()` for an associative-array operand.
pub(super) fn try_lower_assoc_in_array(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    needle_ty: PhpType,
    array_ty: PhpType,
) -> Result<bool> {
    let PhpType::AssocArray { value, .. } = array_ty.codegen_repr() else {
        return Ok(false);
    };
    let needle_ty = needle_ty.codegen_repr();
    let value_ty = value.codegen_repr();
    require_assoc_search_value("in_array", &needle_ty, &value_ty)?;
    lower_assoc_in_array(ctx, needle, array, &needle_ty, &value_ty)?;
    Ok(true)
}

/// Verifies that an associative-array value search has a supported comparison shape.
fn require_assoc_search_value(name: &str, needle_ty: &PhpType, value_ty: &PhpType) -> Result<()> {
    match value_ty {
        PhpType::Str if needle_ty == &PhpType::Str => Ok(()),
        PhpType::Int | PhpType::Bool if matches!(needle_ty, PhpType::Int | PhpType::Bool) => {
            Ok(())
        }
        PhpType::Mixed if matches!(needle_ty, PhpType::Int | PhpType::Bool | PhpType::Str) => {
            Ok(())
        }
        _ => Err(CodegenIrError::unsupported(format!(
            "{} needle PHP type {:?} for associative-array value PHP type {:?}",
            name, needle_ty, value_ty
        ))),
    }
}

/// Emits associative-array `array_search()` and boxes the found key or false.
fn lower_assoc_array_search(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    needle_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_assoc_array_search_aarch64(ctx, needle, array, needle_ty, value_ty),
        Arch::X86_64 => lower_assoc_array_search_x86_64(ctx, needle, array, needle_ty, value_ty),
    }
}

/// Emits associative-array `in_array()` and returns a boolean integer.
fn lower_assoc_in_array(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    needle_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_assoc_in_array_aarch64(ctx, needle, array, needle_ty, value_ty),
        Arch::X86_64 => lower_assoc_in_array_x86_64(ctx, needle, array, needle_ty, value_ty),
    }
}

/// Pushes the search needle as one 16-byte temporary stack slot.
fn push_assoc_search_needle(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    needle_ty: &PhpType,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => match needle_ty {
            PhpType::Str => {
                ctx.load_string_value_to_regs(needle, "x1", "x2")?;
                abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            }
            PhpType::Int | PhpType::Bool => {
                ctx.load_value_to_reg(needle, "x1")?;
                abi::emit_push_reg(ctx.emitter, "x1");
            }
            other => {
                return Err(CodegenIrError::unsupported(format!(
                    "associative-array search needle PHP type {:?}",
                    other
                )))
            }
        },
        Arch::X86_64 => match needle_ty {
            PhpType::Str => {
                ctx.load_string_value_to_regs(needle, "rdi", "rdx")?;
                abi::emit_push_reg_pair(ctx.emitter, "rdi", "rdx");
            }
            PhpType::Int | PhpType::Bool => {
                ctx.load_value_to_reg(needle, "rdi")?;
                abi::emit_push_reg(ctx.emitter, "rdi");
            }
            other => {
                return Err(CodegenIrError::unsupported(format!(
                    "associative-array search needle PHP type {:?}",
                    other
                )))
            }
        },
    }
    Ok(())
}

/// Emits the AArch64 associative `array_search()` loop.
fn lower_assoc_array_search_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    needle_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    let found_label = ctx.next_label("assoc_array_search_found");
    let miss_label = ctx.next_label("assoc_array_search_miss");
    let loop_label = ctx.next_label("assoc_array_search_loop");
    let cleanup_label = ctx.next_label("assoc_array_search_cleanup");

    ctx.load_value_to_reg(array, "x10")?;
    abi::emit_push_reg(ctx.emitter, "x10");
    push_assoc_search_needle(ctx, needle, needle_ty)?;
    ctx.emitter.instruction("str xzr, [sp, #-16]!");                            // push the insertion-order iterator cursor

    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("ldr x0, [sp, #32]");                               // load the associative-array hash pointer for iteration
    ctx.emitter.instruction("ldr x1, [sp]");                                    // load the current insertion-order iterator cursor
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
    ctx.emitter.instruction("cmn x0, #1");                                      // check whether hash iteration reached the end sentinel
    ctx.emitter.instruction(&format!("b.eq {}", miss_label));                   // return false when no associative-array value matches
    ctx.emitter.instruction("str x0, [sp]");                                    // save the next iterator cursor for the following scan step
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    emit_assoc_value_match_aarch64(ctx, needle_ty, value_ty, 32, &found_label)?;
    ctx.emitter.instruction("add sp, sp, #16");                                 // discard the preserved key after a non-matching entry
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning associative-array entries

    ctx.emitter.label(&found_label);
    box_assoc_search_found_key_aarch64(ctx);
    ctx.emitter.instruction(&format!("b {}", cleanup_label));                   // clean temporary stack slots after boxing the found key

    ctx.emitter.label(&miss_label);
    box_assoc_search_false_aarch64(ctx);

    ctx.emitter.label(&cleanup_label);
    ctx.emitter.instruction("add sp, sp, #48");                                 // discard iterator cursor, needle, and hash pointer
    Ok(())
}

/// Emits the x86_64 associative `array_search()` loop.
fn lower_assoc_array_search_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    needle_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    let found_label = ctx.next_label("assoc_array_search_found");
    let miss_label = ctx.next_label("assoc_array_search_miss");
    let loop_label = ctx.next_label("assoc_array_search_loop");
    let cleanup_label = ctx.next_label("assoc_array_search_cleanup");

    ctx.load_value_to_reg(array, "r10")?;
    abi::emit_push_reg(ctx.emitter, "r10");
    push_assoc_search_needle(ctx, needle, needle_ty)?;
    ctx.emitter.instruction("sub rsp, 16");                                     // reserve the insertion-order iterator cursor slot
    ctx.emitter.instruction("mov QWORD PTR [rsp], 0");                          // initialize the iterator cursor to the hash head sentinel

    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                   // load the associative-array hash pointer for iteration
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp]");                        // load the current insertion-order iterator cursor
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
    ctx.emitter.instruction("cmp rax, -1");                                     // check whether hash iteration reached the end sentinel
    ctx.emitter.instruction(&format!("je {}", miss_label));                     // return false when no associative-array value matches
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // save the next iterator cursor for the following scan step
    abi::emit_push_reg_pair(ctx.emitter, "rdi", "rdx");
    emit_assoc_value_match_x86_64(ctx, needle_ty, value_ty, 32, &found_label)?;
    ctx.emitter.instruction("add rsp, 16");                                     // discard the preserved key after a non-matching entry
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning associative-array entries

    ctx.emitter.label(&found_label);
    box_assoc_search_found_key_x86_64(ctx);
    ctx.emitter.instruction(&format!("jmp {}", cleanup_label));                 // clean temporary stack slots after boxing the found key

    ctx.emitter.label(&miss_label);
    box_assoc_search_false_x86_64(ctx);

    ctx.emitter.label(&cleanup_label);
    ctx.emitter.instruction("add rsp, 48");                                     // discard iterator cursor, needle, and hash pointer
    Ok(())
}

/// Emits the AArch64 associative `in_array()` loop.
fn lower_assoc_in_array_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    needle_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    let found_label = ctx.next_label("assoc_in_array_found");
    let miss_label = ctx.next_label("assoc_in_array_miss");
    let loop_label = ctx.next_label("assoc_in_array_loop");
    let done_label = ctx.next_label("assoc_in_array_done");

    ctx.load_value_to_reg(array, "x10")?;
    abi::emit_push_reg(ctx.emitter, "x10");
    push_assoc_search_needle(ctx, needle, needle_ty)?;
    ctx.emitter.instruction("str xzr, [sp, #-16]!");                            // push the insertion-order iterator cursor

    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("ldr x0, [sp, #32]");                               // load the associative-array hash pointer for iteration
    ctx.emitter.instruction("ldr x1, [sp]");                                    // load the current insertion-order iterator cursor
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
    ctx.emitter.instruction("cmn x0, #1");                                      // check whether hash iteration reached the end sentinel
    ctx.emitter.instruction(&format!("b.eq {}", miss_label));                   // return false when no associative-array value matches
    ctx.emitter.instruction("str x0, [sp]");                                    // save the next iterator cursor for the following scan step
    emit_assoc_value_match_aarch64(ctx, needle_ty, value_ty, 16, &found_label)?;
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning associative-array entries

    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov x0, #1");                                      // return true after finding a matching associative-array value
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the false result after a successful membership check
    ctx.emitter.label(&miss_label);
    ctx.emitter.instruction("mov x0, #0");                                      // return false after exhausting associative-array values
    ctx.emitter.label(&done_label);
    ctx.emitter.instruction("add sp, sp, #48");                                 // discard iterator cursor, needle, and hash pointer
    Ok(())
}

/// Emits the x86_64 associative `in_array()` loop.
fn lower_assoc_in_array_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle: ValueId,
    array: ValueId,
    needle_ty: &PhpType,
    value_ty: &PhpType,
) -> Result<()> {
    let found_label = ctx.next_label("assoc_in_array_found");
    let miss_label = ctx.next_label("assoc_in_array_miss");
    let loop_label = ctx.next_label("assoc_in_array_loop");
    let done_label = ctx.next_label("assoc_in_array_done");

    ctx.load_value_to_reg(array, "r10")?;
    abi::emit_push_reg(ctx.emitter, "r10");
    push_assoc_search_needle(ctx, needle, needle_ty)?;
    ctx.emitter.instruction("sub rsp, 16");                                     // reserve the insertion-order iterator cursor slot
    ctx.emitter.instruction("mov QWORD PTR [rsp], 0");                          // initialize the iterator cursor to the hash head sentinel

    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                   // load the associative-array hash pointer for iteration
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp]");                        // load the current insertion-order iterator cursor
    abi::emit_call_label(ctx.emitter, "__rt_hash_iter_next");
    ctx.emitter.instruction("cmp rax, -1");                                     // check whether hash iteration reached the end sentinel
    ctx.emitter.instruction(&format!("je {}", miss_label));                     // return false when no associative-array value matches
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // save the next iterator cursor for the following scan step
    emit_assoc_value_match_x86_64(ctx, needle_ty, value_ty, 16, &found_label)?;
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning associative-array entries

    ctx.emitter.label(&found_label);
    ctx.emitter.instruction("mov rax, 1");                                      // return true after finding a matching associative-array value
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the false result after a successful membership check
    ctx.emitter.label(&miss_label);
    ctx.emitter.instruction("xor eax, eax");                                    // return false after exhausting associative-array values
    ctx.emitter.label(&done_label);
    ctx.emitter.instruction("add rsp, 48");                                     // discard iterator cursor, needle, and hash pointer
    Ok(())
}

/// Emits AArch64 comparison for one current associative-array entry.
fn emit_assoc_value_match_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle_ty: &PhpType,
    value_ty: &PhpType,
    needle_offset: i32,
    found_label: &str,
) -> Result<()> {
    match value_ty {
        PhpType::Str => {
            ctx.emitter.instruction("mov x1, x3");                              // move the entry string pointer into the comparison argument
            ctx.emitter.instruction("mov x2, x4");                              // move the entry string length into the comparison argument
            ctx.emitter.instruction(&format!("ldp x3, x4, [sp, #{}]", needle_offset)); // reload the searched string needle from the stack
            abi::emit_call_label(ctx.emitter, "__rt_str_eq");
            ctx.emitter.instruction(&format!("cbnz x0, {}", found_label));      // branch when the entry string matches the needle
        }
        PhpType::Int | PhpType::Bool => {
            ctx.emitter.instruction(&format!("ldr x6, [sp, #{}]", needle_offset)); // reload the searched scalar needle from the stack
            ctx.emitter.instruction("cmp x3, x6");                              // compare the entry scalar payload against the needle
            ctx.emitter.instruction(&format!("b.eq {}", found_label));          // branch when the scalar entry matches the needle
        }
        PhpType::Mixed => {
            emit_mixed_assoc_value_match_aarch64(ctx, needle_ty, needle_offset, found_label)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "associative-array search value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Emits x86_64 comparison for one current associative-array entry.
fn emit_assoc_value_match_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle_ty: &PhpType,
    value_ty: &PhpType,
    needle_offset: i32,
    found_label: &str,
) -> Result<()> {
    match value_ty {
        PhpType::Str => {
            ctx.emitter.instruction("mov rdi, rcx");                            // move the entry string pointer into the comparison argument
            ctx.emitter.instruction("mov rsi, r8");                             // move the entry string length into the comparison argument
            ctx.emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", needle_offset)); // reload the searched string needle pointer
            ctx.emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", needle_offset + 8)); // reload the searched string needle length
            abi::emit_call_label(ctx.emitter, "__rt_str_eq");
            ctx.emitter.instruction("test rax, rax");                           // check whether the entry string matched the needle
            ctx.emitter.instruction(&format!("jne {}", found_label));           // branch when the entry string matches the needle
        }
        PhpType::Int | PhpType::Bool => {
            ctx.emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", needle_offset)); // reload the searched scalar needle from the stack
            ctx.emitter.instruction("cmp rcx, r10");                            // compare the entry scalar payload against the needle
            ctx.emitter.instruction(&format!("je {}", found_label));            // branch when the scalar entry matches the needle
        }
        PhpType::Mixed => {
            emit_mixed_assoc_value_match_x86_64(ctx, needle_ty, needle_offset, found_label)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "associative-array search value PHP type {:?}",
                other
            )));
        }
    }
    Ok(())
}

/// Emits AArch64 comparison for boxed Mixed associative-array entries.
fn emit_mixed_assoc_value_match_aarch64(
    ctx: &mut FunctionContext<'_>,
    needle_ty: &PhpType,
    needle_offset: i32,
    found_label: &str,
) -> Result<()> {
    let mismatch_label = ctx.next_label("assoc_mixed_search_mismatch");
    let expected_tag = runtime_value_tag("associative-array search", needle_ty)? as i64;
    ctx.emitter.instruction("mov x0, x3");                                      // pass the boxed Mixed entry value to the unbox helper
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    abi::emit_load_int_immediate(ctx.emitter, "x6", expected_tag);
    ctx.emitter.instruction("cmp x0, x6");                                      // compare the entry runtime tag against the searched needle tag
    ctx.emitter.instruction(&format!("b.ne {}", mismatch_label));               // skip entries whose concrete type differs from the needle
    match needle_ty {
        PhpType::Str => {
            ctx.emitter.instruction(&format!("ldp x3, x4, [sp, #{}]", needle_offset)); // reload the searched string needle from the stack
            abi::emit_call_label(ctx.emitter, "__rt_str_eq");
            ctx.emitter.instruction(&format!("cbnz x0, {}", found_label));      // branch when the unboxed string entry matches the needle
        }
        PhpType::Int | PhpType::Bool => {
            ctx.emitter.instruction(&format!("ldr x6, [sp, #{}]", needle_offset)); // reload the searched scalar needle from the stack
            ctx.emitter.instruction("cmp x1, x6");                              // compare the unboxed scalar payload against the needle
            ctx.emitter.instruction(&format!("b.eq {}", found_label));          // branch when the unboxed scalar entry matches the needle
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "associative-array mixed search needle PHP type {:?}",
                other
            )));
        }
    }
    ctx.emitter.label(&mismatch_label);
    Ok(())
}

/// Emits x86_64 comparison for boxed Mixed associative-array entries.
fn emit_mixed_assoc_value_match_x86_64(
    ctx: &mut FunctionContext<'_>,
    needle_ty: &PhpType,
    needle_offset: i32,
    found_label: &str,
) -> Result<()> {
    let mismatch_label = ctx.next_label("assoc_mixed_search_mismatch");
    let expected_tag = runtime_value_tag("associative-array search", needle_ty)? as i64;
    ctx.emitter.instruction("mov rdi, rcx");                                    // pass the boxed Mixed entry value to the unbox helper
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    abi::emit_load_int_immediate(ctx.emitter, "r10", expected_tag);
    ctx.emitter.instruction("cmp rax, r10");                                    // compare the entry runtime tag against the searched needle tag
    ctx.emitter.instruction(&format!("jne {}", mismatch_label));                // skip entries whose concrete type differs from the needle
    match needle_ty {
        PhpType::Str => {
            ctx.emitter.instruction("mov rsi, rdx");                            // move the unboxed entry string length into the comparison argument
            ctx.emitter.instruction(&format!("mov rdx, QWORD PTR [rsp + {}]", needle_offset)); // reload the searched string needle pointer
            ctx.emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {}]", needle_offset + 8)); // reload the searched string needle length
            abi::emit_call_label(ctx.emitter, "__rt_str_eq");
            ctx.emitter.instruction("test rax, rax");                           // check whether the unboxed string entry matched the needle
            ctx.emitter.instruction(&format!("jne {}", found_label));           // branch when the unboxed string entry matches the needle
        }
        PhpType::Int | PhpType::Bool => {
            ctx.emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", needle_offset)); // reload the searched scalar needle from the stack
            ctx.emitter.instruction("cmp rdi, r10");                            // compare the unboxed scalar payload against the needle
            ctx.emitter.instruction(&format!("je {}", found_label));            // branch when the unboxed scalar entry matches the needle
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "associative-array mixed search needle PHP type {:?}",
                other
            )));
        }
    }
    ctx.emitter.label(&mismatch_label);
    Ok(())
}

/// Boxes the preserved AArch64 associative-array key for a successful search.
fn box_assoc_search_found_key_aarch64(ctx: &mut FunctionContext<'_>) {
    let string_key_label = ctx.next_label("assoc_array_search_string_key");
    let boxed_label = ctx.next_label("assoc_array_search_key_boxed");
    abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
    ctx.emitter.instruction("cmn x2, #1");                                      // check whether the matching key is an integer key
    ctx.emitter.instruction(&format!("b.ne {}", string_key_label));             // string keys need string-tagged Mixed boxing
    ctx.emitter.instruction("mov x0, #0");                                      // runtime tag 0 = integer key
    ctx.emitter.instruction("mov x2, xzr");                                     // integer Mixed payloads do not use a high word
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction(&format!("b {}", boxed_label));                     // skip the string-key boxing path
    ctx.emitter.label(&string_key_label);
    ctx.emitter.instruction("mov x0, #1");                                      // runtime tag 1 = string key
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.label(&boxed_label);
}

/// Boxes the preserved x86_64 associative-array key for a successful search.
fn box_assoc_search_found_key_x86_64(ctx: &mut FunctionContext<'_>) {
    let string_key_label = ctx.next_label("assoc_array_search_string_key");
    let boxed_label = ctx.next_label("assoc_array_search_key_boxed");
    abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rdx");
    ctx.emitter.instruction("cmp rdx, -1");                                     // check whether the matching key is an integer key
    ctx.emitter.instruction(&format!("jne {}", string_key_label));              // string keys need string-tagged Mixed boxing
    ctx.emitter.instruction("xor esi, esi");                                    // integer Mixed payloads do not use a high word
    ctx.emitter.instruction("xor eax, eax");                                    // runtime tag 0 = integer key
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction(&format!("jmp {}", boxed_label));                   // skip the string-key boxing path
    ctx.emitter.label(&string_key_label);
    ctx.emitter.instruction("mov rsi, rdx");                                    // move the matching string-key length into the Mixed payload high word
    ctx.emitter.instruction("mov eax, 1");                                      // runtime tag 1 = string key
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.label(&boxed_label);
}

/// Boxes bool false for an AArch64 associative-array search miss.
fn box_assoc_search_false_aarch64(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.instruction("mov x1, #0");                                      // false Mixed payload is zero
    ctx.emitter.instruction("mov x2, #0");                                      // bool Mixed payloads do not use a high word
    ctx.emitter.instruction("mov x0, #3");                                      // runtime tag 3 = bool
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
}

/// Boxes bool false for an x86_64 associative-array search miss.
fn box_assoc_search_false_x86_64(ctx: &mut FunctionContext<'_>) {
    ctx.emitter.instruction("xor edi, edi");                                    // false Mixed payload is zero
    ctx.emitter.instruction("xor esi, esi");                                    // bool Mixed payloads do not use a high word
    ctx.emitter.instruction("mov eax, 3");                                      // runtime tag 3 = bool
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
}
