//! Purpose:
//! Extracts a string-keyed column from packed or associative boxed PHP arrays.
//!
//! Called from:
//! - The typed ArrayColumn backend for declared PHP array arguments.
//!
//! Key details:
//! - Borrows source rows without changing their internal cursors or storage.
//! - Presence checks preserve explicit nulls and skip missing keys or non-array rows.
//! - Each result slot adopts one owned read cell; no callbacks or warnings are emitted.
//! - Stamping result elements preserves the x86_64 heap marker used by boxing and cleanup.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const FRAME: usize = 96;
const SOURCE: usize = 0;
const KEY_LO: usize = 8;
const KEY_HI: usize = 16;
const CURSOR: usize = 24;
const ROW: usize = 32;
const OUTPUT: usize = 40;
const CELL: usize = 48;

/// Emits one equivalent instruction for the selected native ABI.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // keep the same operation on both native architectures
}

/// Borrows a source box and string key in C ABI args 0..2; returns an owned array or zero.
pub fn emit_array_column_boxed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let arg2 = abi::int_arg_reg_name(emitter.target, 2);
    let low = if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" };

    emitter.blank();
    emitter.label_global("__rt_array_column_boxed");
    abi::emit_frame_prologue(emitter, FRAME);
    abi::emit_store_to_sp(emitter, arg1, KEY_LO);
    abi::emit_store_to_sp(emitter, arg2, KEY_HI);
    abi::emit_reg_move(emitter, result, arg0);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    ins(emitter, "sub x9, x0, #4", "lea r10, [rax - 4]");
    ins(emitter, "cmp x9, #1", "cmp r10, 1");
    ins(emitter, "b.hi __rt_array_column_boxed_invalid", "ja __rt_array_column_boxed_invalid");
    abi::emit_store_to_sp(emitter, low, SOURCE);

    // Normalize numeric-string column keys before either row layout is inspected.
    let (key, length) = if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rax", "rdx") };
    abi::emit_load_temporary_stack_slot(emitter, key, KEY_LO);
    abi::emit_load_temporary_stack_slot(emitter, length, KEY_HI);
    abi::emit_call_label(emitter, "__rt_hash_normalize_key");
    abi::emit_store_to_sp(emitter, key, KEY_LO);
    abi::emit_store_to_sp(emitter, length, KEY_HI);

    // The fresh result owns pointer-sized Mixed cells, including when it stays empty.
    abi::emit_load_int_immediate(emitter, arg0, 8);
    abi::emit_load_int_immediate(emitter, arg1, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    ins(emitter, "ldr x9, [x0, #-8]", "mov r10, QWORD PTR [rax - 8]");
    ins(emitter, "mov x10, #0x80ff", "mov r11, 0xffffffff000080ff");
    ins(emitter, "and x9, x9, x10", "and r10, r11");
    ins(emitter, "orr x9, x9, #0x700", "or r10, 0x700");
    ins(emitter, "str x9, [x0, #-8]", "mov QWORD PTR [rax - 8], r10");
    abi::emit_store_to_sp(emitter, result, OUTPUT);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_store_to_sp(emitter, result, CURSOR);

    emitter.label("__rt_array_column_boxed_loop");
    abi::emit_load_temporary_stack_slot(emitter, arg0, SOURCE);
    abi::emit_load_temporary_stack_slot(emitter, arg1, CURSOR);
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    ins(emitter, "cmn x0, #1", "cmp rax, -1");
    ins(emitter, "b.eq __rt_array_column_boxed_done", "je __rt_array_column_boxed_done");
    abi::emit_store_to_sp(emitter, result, CURSOR);
    let fields = if emitter.target.arch == Arch::AArch64 {
        ["x3", "x4", "x5"]
    } else {
        ["r8", "r9", "r10"]
    };
    for (reg, offset) in fields.into_iter().zip([CELL, CELL + 8, CELL + 16]) {
        abi::emit_store_to_sp(emitter, reg, offset);
    }
    abi::emit_temporary_stack_address(emitter, result, CELL);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    ins(emitter, "sub x9, x0, #4", "lea r10, [rax - 4]");
    ins(emitter, "cmp x9, #1", "cmp r10, 1");
    ins(emitter, "b.hi __rt_array_column_boxed_loop", "ja __rt_array_column_boxed_loop");
    abi::emit_store_to_sp(emitter, low, ROW);

    // A presence probe distinguishes an absent column from a present null value.
    load_row_key(emitter);
    abi::emit_call_label(emitter, "__rt_array_key_exists_mixed_key");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_column_boxed_loop");
    load_row_key(emitter);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 3), 0);
    abi::emit_call_label(emitter, "__rt_array_get_mixed_key");
    abi::emit_reg_move(emitter, arg1, result);
    abi::emit_load_temporary_stack_slot(emitter, arg0, OUTPUT);
    abi::emit_call_label(emitter, "__rt_array_push_int");
    abi::emit_store_to_sp(emitter, result, OUTPUT);
    abi::emit_jump(emitter, "__rt_array_column_boxed_loop");

    emitter.label("__rt_array_column_boxed_invalid");
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_jump(emitter, "__rt_array_column_boxed_return");
    emitter.label("__rt_array_column_boxed_done");
    abi::emit_load_temporary_stack_slot(emitter, result, OUTPUT);
    emitter.label("__rt_array_column_boxed_return");
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Restores the borrowed row and normalized column key for presence and read helpers.
fn load_row_key(emitter: &mut Emitter) {
    for (index, offset) in [ROW, KEY_LO, KEY_HI].into_iter().enumerate() {
        abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every target probes presence before reading and transfers each owned read cell once.
    #[test]
    fn boxed_column_uses_presence_and_owned_reads_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_array_column_boxed(&mut emitter);
            let asm = emitter.output();
            let probe = asm.find("__rt_array_key_exists_mixed_key").unwrap();
            let read = asm.find("__rt_array_get_mixed_key").unwrap();
            let append = asm.find("__rt_array_push_int").unwrap();
            assert!(probe < read && read < append, "{target}");
            assert!(asm.contains("__rt_array_iter_next"), "{target}");
            assert!(asm.contains("__rt_hash_normalize_key"), "{target}");
            if target == "linux-x86_64" {
                assert!(asm.contains("mov r11, 0xffffffff000080ff"),
                    "the result must remain recognizable to heap_kind and typed decref");
                assert!(!asm.contains("mov r11, 0x80ff"), "do not erase the heap identity marker");
            }
            assert!(!asm.contains("__rt_incref"), "{target}: the read already owns its cell");
            assert!(!asm.contains("__rt_decref"), "{target}: result slots adopt read owners");
        }
    }
}
