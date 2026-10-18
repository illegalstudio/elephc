//! Purpose:
//! Protects public serialize calls and their reference registries across nested hooks.
//!
//! Called from:
//! - The serializer emitter and EIR serialize builtin lowering.
//!
//! Key details:
//! - Legacy callbacks share references; other calls snapshot the populated registry on the stack.
//! - A guarded dispatch restores state before propagating a callback exception.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::exceptions::emit_guarded_cleanup_call;

const FRAME: usize = 96;
const PENDING: usize = 8;
const LEGACY: usize = 16;
const COUNTER: usize = 24;
const COUNT: usize = 32;
const SNAPSHOT: usize = 40;
const LENGTH: usize = 48;
const POINTER: usize = 56;
const HIGH: usize = 64;
const LOW: usize = 72;
const TAG: usize = 80;

/// Emits an exception-safe public entry and its unary guarded dispatch adapter.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    // -- preserve public operands and select shared or isolated references --
    emitter.blank();
    emitter.label_global("__rt_serialize_call_value");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [TAG, LOW, HIGH].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_store_zero_to_local_slot(emitter, PENDING);
    abi::emit_load_symbol_to_reg(emitter, result, "_ser_legacy_active", 0);
    abi::store_at_offset(emitter, result, LEGACY);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_serialize_context_ready");
    for (symbol, offset) in [("_ser_value_counter", COUNTER), ("_ser_obj_count", COUNT)] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    emit_reference_copy(emitter, false);
    abi::emit_call_label(emitter, "__rt_serialize_begin");
    emitter.label("__rt_serialize_context_ready");
    // -- guard callback execution, then restore state before returning or rethrowing --
    abi::emit_frame_slot_address(emitter, result, TAG);
    emit_guarded_cleanup_call(emitter, "__rt_serialize_guarded_dispatch", result, PENDING);
    abi::load_at_offset(emitter, result, LEGACY);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_serialize_context_restored");
    emit_reference_copy(emitter, true);
    for (symbol, offset) in [("_ser_value_counter", COUNTER), ("_ser_obj_count", COUNT)] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
    emitter.label("__rt_serialize_context_restored");
    abi::load_at_offset(emitter, result, LEGACY);
    abi::emit_store_reg_to_symbol(emitter, result, "_ser_legacy_active", 0);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_serialize_context_return");
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label("__rt_serialize_context_return");
    let (pointer, length) = abi::string_result_regs(emitter);
    abi::load_at_offset(emitter, pointer, POINTER);
    abi::load_at_offset(emitter, length, LENGTH);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);

    // -- adapt the protected unary callback to the raw serializer ABI --
    emitter.blank();
    emitter.label_global("__rt_serialize_guarded_dispatch");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::load_at_offset(emitter, scratch, 8);
    for (index, offset) in [0, 8, 16].into_iter().enumerate() {
        abi::emit_load_from_address(emitter, abi::int_arg_reg_name(emitter.target, index), scratch, offset);
    }
    abi::emit_call_label(emitter, "__rt_serialize_value");
    abi::load_at_offset(emitter, scratch, 8);
    let (pointer, length) = abi::string_result_regs(emitter);
    abi::emit_store_to_address(emitter, pointer, scratch, TAG - POINTER);
    abi::emit_store_to_address(emitter, length, scratch, TAG - LENGTH);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Copies only populated pointer/index pairs, preserving snapshots across callback unwinding.
fn emit_reference_copy(emitter: &mut Emitter, restore: bool) {
    let phase = if restore { "restore" } else { "save" };
    let loop_label = format!("__rt_serialize_context_{phase}_loop");
    let done = format!("__rt_serialize_context_{phase}_done");
    // -- copy the populated registry prefix to or from its aligned stack snapshot --
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x12", COUNT);
            if restore {
                abi::load_at_offset(emitter, "x16", SNAPSHOT);
            } else {
                emitter.instruction("lsl x13, x12, #4");                        // reserve two words for every registered object
                emitter.instruction("sub sp, sp, x13");                         // keep the snapshot below the stable fixed frame
                emitter.instruction("mov x16, sp");                             // remember the aligned snapshot payload
                abi::store_at_offset(emitter, "x16", SNAPSHOT);
            }
            abi::emit_symbol_address(emitter, "x9", "_ser_obj_ptrs");
            abi::emit_symbol_address(emitter, "x10", "_ser_obj_idxs");
            emitter.instruction("mov x11, #0");                                 // start at the populated registry prefix
            emitter.label(&loop_label);
            emitter.instruction("cmp x11, x12");                                // test the bounded object count
            emitter.instruction(&format!("b.hs {done}"));                       // finish after every used pair is copied
            emitter.instruction("add x15, x16, x11, lsl #4");                   // address the interleaved snapshot pair
            if restore {
                emitter.instruction("ldp x13, x14, [x15]");                     // recover the outer object identity and reference index
                emitter.instruction("str x13, [x9, x11, lsl #3]");              // restore the outer object pointer
                emitter.instruction("str x14, [x10, x11, lsl #3]");             // restore its PHP reference index
            } else {
                emitter.instruction("ldr x13, [x9, x11, lsl #3]");              // read the outer object pointer
                emitter.instruction("ldr x14, [x10, x11, lsl #3]");             // read its PHP reference index
                emitter.instruction("stp x13, x14, [x15]");                     // snapshot the pair without acquiring a new owner
            }
            emitter.instruction("add x11, x11, #1");                            // advance to the next registered object
            emitter.instruction(&format!("b {loop_label}"));                    // continue copying the bounded prefix
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r8", COUNT);
            if restore {
                abi::load_at_offset(emitter, "r9", SNAPSHOT);
            } else {
                emitter.instruction("mov r9, r8");                              // begin sizing two words per registered object
                emitter.instruction("shl r9, 4");                               // retain sixteen-byte stack alignment
                emitter.instruction("sub rsp, r9");                             // reserve the bounded snapshot on the native stack
                emitter.instruction("mov r9, rsp");                             // remember its payload independently of nested frames
                abi::store_at_offset(emitter, "r9", SNAPSHOT);
            }
            abi::emit_symbol_address(emitter, "r10", "_ser_obj_ptrs");
            abi::emit_symbol_address(emitter, "r11", "_ser_obj_idxs");
            emitter.instruction("xor ecx, ecx");                                // start with the first populated registry pair
            emitter.label(&loop_label);
            emitter.instruction("cmp rcx, r8");                                 // test the bounded object count
            emitter.instruction(&format!("jae {done}"));                        // finish after the populated prefix
            emitter.instruction("mov rax, rcx");                                // compute the interleaved snapshot stride
            emitter.instruction("shl rax, 4");                                  // each saved pointer/index pair occupies sixteen bytes
            emitter.instruction("add rax, r9");                                 // address this saved pair
            if restore {
                emitter.instruction("mov rdx, QWORD PTR [rax]");                // recover the outer object identity
                emitter.instruction("mov rsi, QWORD PTR [rax + 8]");            // recover its PHP reference index
                emitter.instruction("mov QWORD PTR [r10 + rcx*8], rdx");        // restore the outer object pointer
                emitter.instruction("mov QWORD PTR [r11 + rcx*8], rsi");        // restore its reference index
            } else {
                emitter.instruction("mov rdx, QWORD PTR [r10 + rcx*8]");        // read the outer object identity
                emitter.instruction("mov rsi, QWORD PTR [r11 + rcx*8]");        // read its PHP reference index
                emitter.instruction("mov QWORD PTR [rax], rdx");                // snapshot the borrowed object identity
                emitter.instruction("mov QWORD PTR [rax + 8], rsi");            // snapshot its reference index
            }
            emitter.instruction("inc rcx");                                     // advance to the next registered object
            emitter.instruction(&format!("jmp {loop_label}"));                  // continue copying the bounded prefix
        }
    }
    emitter.label(&done);
}
