//! Purpose:
//! Hydrates a validated Serializable C payload in the recursive object's parser frame.
//!
//! Called from:
//! - Both native decoders after class allocation, reference publication, and envelope parsing.
//!
//! Key details:
//! - Raw string arguments borrow the owned input; boxed arguments transfer to context cleanup.
//! - Blocked and unknown classes remain incomplete objects without running user hooks.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::data::SerializableReturnAbi;

/// Emits C payload dispatch while leaving O property parsing on its existing path.
pub(super) fn emit_body(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let (base, pos, count, object, policy, marker, pointer, length, method, data) = if arm {
        (272, 264, 240, 248, 192, 152, 216, 232, 200, 160)
    } else { (8, 16, 40, 32, 80, 120, 56, 48, 72, 112) };
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let arg2 = abi::int_arg_reg_name(emitter.target, 2);
    abi::load_at_offset(emitter, result, marker);
    if arm {
        emitter.instruction("cmp x0, #67");                                     // only C envelopes contain raw payload bytes
        emitter.instruction("b.ne __rt_unser_custom_ordinary");                 // O resumes its property and modern-hook path
    } else {
        emitter.instruction("cmp rax, 67");                                     // only C envelopes contain raw payload bytes
        emitter.instruction("jne __rt_unser_custom_ordinary");                  // O resumes its property and modern-hook path
    }
    abi::load_at_offset(emitter, result, pos);
    abi::load_at_offset(emitter, scratch, base);
    if arm {
        emitter.instruction("add x10, x10, x0");                                // point into the validated raw byte payload
    } else {
        emitter.instruction("add r10, rax");                                    // point into the validated raw byte payload
    }
    abi::store_at_offset(emitter, scratch, pointer);
    abi::load_at_offset(emitter, scratch, count);
    abi::store_at_offset(emitter, scratch, length);
    if arm {
        emitter.instruction("add x0, x0, x10");                                 // skip the declared opaque payload length
    } else {
        emitter.instruction("add rax, r10");                                    // skip the declared opaque payload length
    }
    abi::store_at_offset(emitter, result, pos);
    abi::load_at_offset(emitter, result, policy);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unser_custom_incomplete");
    load_class_entry(emitter, object, "_class_serializable_unserialize_ptrs");
    abi::store_at_offset(emitter, result, method);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unser_custom_done");
    load_class_entry(emitter, object, "_class_serializable_unserialize_boxed");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unser_custom_raw");
    let (lo, hi) = if arm { ("x1", "x2") } else { ("rdi", "rsi") };
    abi::load_at_offset(emitter, lo, pointer);
    abi::load_at_offset(emitter, hi, length);
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, result, data);
    abi::store_at_offset(emitter, result, pointer);
    abi::emit_call_label(emitter, "__rt_unserialize_defer_data");
    super::parser_owners::emit_transfer_data(emitter);
    abi::load_at_offset(emitter, arg0, object);
    abi::load_at_offset(emitter, arg1, pointer);
    abi::load_at_offset(emitter, scratch, method);
    abi::emit_load_int_immediate(emitter, arg2, 0);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 3), scratch);
    abi::emit_call_label(emitter, "__rt_unserialize_invoke_legacy");
    abi::emit_jump(emitter, "__rt_unser_custom_retire_result");
    emitter.label("__rt_unser_custom_raw");
    abi::load_at_offset(emitter, arg0, object);
    abi::load_at_offset(emitter, arg1, pointer);
    abi::load_at_offset(emitter, arg2, length);
    abi::load_at_offset(emitter, scratch, method);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 3), scratch);
    abi::emit_call_label(emitter, "__rt_unserialize_invoke_legacy");
    emitter.label("__rt_unser_custom_retire_result");
    emit_retire_result(emitter, object, pointer, method);
    abi::emit_jump(emitter, "__rt_unser_custom_done");
    emitter.label("__rt_unser_custom_incomplete");
    abi::emit_load_int_immediate(emitter, arg0, 0);
    abi::emit_load_int_immediate(emitter, arg1, 7);
    abi::emit_call_label(emitter, "__rt_hash_new");
    abi::load_at_offset(emitter, scratch, object);
    abi::emit_store_to_address(emitter, result, scratch, 24);
    emitter.label("__rt_unser_custom_done");
    abi::emit_jump(emitter, "__rt_unser_at_obj_box");
    emitter.label("__rt_unser_custom_ordinary");
}

/// Retires ignored legacy hydration returns according to their checked native ownership ABI.
fn emit_retire_result(emitter: &mut Emitter, object: usize, pointer: usize, value: usize) {
    let result = abi::int_result_reg(emitter);
    let string_pointer = abi::string_result_regs(emitter).0;
    abi::store_at_offset(emitter, result, value);
    abi::store_at_offset(emitter, string_pointer, pointer);
    load_class_entry(emitter, object, "_class_serializable_unserialize_return_abi");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{}", SerializableReturnAbi::Null as u8)); // void hydration has no returned owner
            emitter.instruction("b.eq __rt_unser_custom_return_retired");       // ignore an ordinary void return
            emitter.instruction(&format!("cmp x0, #{}", SerializableReturnAbi::Scalar as u8)); // scalar returns carry no lifetime state
            emitter.instruction("b.eq __rt_unser_custom_return_retired");       // ignore a tentative scalar return
            emitter.instruction(&format!("cmp x0, #{}", SerializableReturnAbi::TaggedScalar as u8)); // nullable integers carry no heap ownership
            emitter.instruction("b.eq __rt_unser_custom_return_retired");       // ignore either inline scalar arm
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {}", SerializableReturnAbi::Null as u8)); // void hydration has no returned owner
            emitter.instruction("je __rt_unser_custom_return_retired");         // ignore an ordinary void return
            emitter.instruction(&format!("cmp rax, {}", SerializableReturnAbi::Scalar as u8)); // scalar returns carry no lifetime state
            emitter.instruction("je __rt_unser_custom_return_retired");         // ignore a tentative scalar return
            emitter.instruction(&format!("cmp rax, {}", SerializableReturnAbi::TaggedScalar as u8)); // nullable integers carry no heap ownership
            emitter.instruction("je __rt_unser_custom_return_retired");         // ignore either inline scalar arm
        }
    }
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unser_custom_return_string");
    abi::load_at_offset(emitter, result, value);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_jump(emitter, "__rt_unser_custom_return_retired");
    emitter.label("__rt_unser_custom_return_string");
    abi::load_at_offset(emitter, result, pointer);
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.label("__rt_unser_custom_return_retired");
}

/// Loads a class-indexed entry using the parser's live raw receiver.
fn load_class_entry(emitter: &mut Emitter, object: usize, table: &str) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, result, object);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_symbol_address(emitter, scratch, table);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("ldr x0, [x10, x0, lsl #3]"),      // select the legacy deserializer or argument representation
        Arch::X86_64 => emitter.instruction("mov rax, QWORD PTR [r10 + rax * 8]"), // select the legacy deserializer or argument representation
    }
}
