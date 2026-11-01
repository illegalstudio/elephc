//! Purpose:
//! Emits `_eir_invoker_assoc_to_indexed`, the one routine that turns a named-argument container
//! into the indexed container every descriptor invoker body already knows how to bind.
//!
//! Called from:
//! - The Mixed-container branch of `super::emit_loaded_mixed_array_callback_call`, for every
//!   signature the indexed path can serve alone (see `super::assoc_normalizable`).
//!
//! Key details:
//! - Binding a named container used to be a second, complete argument builder inside every
//!   invoker body: a lookup, a default or missing-argument arm and a value-conversion arm per
//!   parameter. It was the largest part of a body and repeated, signature after signature.
//! - The routine reads the signature from the data table the invoker's stub hands over (see
//!   `super::emit_invoker_name_table`): validation of the container (unknown names, a name that
//!   collides with its own position, a position after a name) is the unchanged scan, emitted
//!   here once; each parameter is then looked up by name and then by position, and the entry is
//!   stored as one owned boxed Mixed cell at its parameter index. An omitted parameter stays a
//!   zero cell, which the indexed path reads as "use the default". An omitted parameter with no
//!   default makes the routine answer zero, and the body raises the missing-argument error.
//! - The result is a fresh array whose length is the highest bound index plus one, so the
//!   indexed path's own required-count check and default arms behave exactly as for a plain
//!   positional container. The caller owns it and records it with the other argument owners.

use super::*;

/// Global symbol of the shared routine.
pub(super) const ASSOC_NORMALIZER: &str = "_eir_invoker_assoc_to_indexed";

const HASH_SLOT: usize = 8;
const ARRAY_SLOT: usize = 16;
const INDEX_SLOT: usize = 24;
const COUNT_SLOT: usize = 32;
// INVOKER_TABLE_OFFSET (40) holds the name table: the validation scan reads it from there.
const BOUND_SLOT: usize = 48;
const SAVED_SLOT: usize = 56;
const ELEM_SLOT: usize = 64;
const FRAME_SIZE: usize = 96;

/// Emits the routine once per module (the helper markers deduplicate across parallel workers).
pub(super) fn emit_once(emitter: &mut Emitter) {
    emitter.emit_keyed_once(
        ASSOC_NORMALIZER,
        crate::codegen::context::HELPER_MARKER_OPEN,
        crate::codegen::context::HELPER_MARKER_CLOSE,
        emit_routine,
    );
}

fn emit_routine(emitter: &mut Emitter) {
    let arch = emitter.target.arch;
    let hash_reg = match arch {
        Arch::AArch64 => "x20",
        Arch::X86_64 => "r13",
    };
    let mut ctx = InvokerEmitContext::new(
        ASSOC_NORMALIZER,
        InvokerArgumentOwners::new(INVOKER_BOUNDARY_FRAME_SIZE, 0),
        false,
        false,
        Vec::new(),
        emitter.target,
    );
    let loop_label = ctx.next_label("assoc_norm_loop");
    let found_label = ctx.next_label("assoc_norm_found");
    let boxit_label = ctx.next_label("assoc_norm_box");
    let store_label = ctx.next_label("assoc_norm_store");
    let notfound_label = ctx.next_label("assoc_norm_notfound");
    let next_label = ctx.next_label("assoc_norm_next");
    let done_label = ctx.next_label("assoc_norm_done");
    let missing_label = ctx.next_label("assoc_norm_missing");
    let exit_label = ctx.next_label("assoc_norm_exit");
    let table_zero_label = ctx.next_label("assoc_norm_no_table");

    emitter.blank();
    emitter.comment("shared named-argument normalizer: named container -> indexed container");
    emitter.raw(".align 2");
    emitter.label_global(ASSOC_NORMALIZER);
    abi::emit_frame_prologue(emitter, FRAME_SIZE);
    abi::store_at_offset(emitter, hash_reg, SAVED_SLOT);
    let hash_arg = abi::int_arg_reg_name(emitter.target, 0);
    let table_arg = abi::int_arg_reg_name(emitter.target, 1);
    abi::store_at_offset(emitter, hash_arg, HASH_SLOT);
    abi::store_at_offset(emitter, table_arg, INVOKER_TABLE_OFFSET);
    emitter.instruction(&format!("mov {}, {}", hash_reg, hash_arg));            // the validation scan reads the container from a callee-saved register
    emit_reject_invalid_named_arguments(hash_reg, 1, false, emitter, &mut ctx, &mut DataSection::new());

    // -- count = table ? table[0] : 0 --
    match arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x9", INVOKER_TABLE_OFFSET);
            emitter.instruction("mov x10, #0");                                 // no table means no declared names
            emitter.instruction(&format!("cbz x9, {}", table_zero_label));      // keep the zero count
            emitter.instruction("ldr x10, [x9]");                               // the table's first word is the number of declared names
            emitter.label(&table_zero_label);
            abi::store_at_offset(emitter, "x10", COUNT_SLOT);
            emitter.instruction("add x0, x10, #1");                             // capacity: one slot per name, never zero
            emitter.instruction("mov x1, #8");                                  // eight-byte boxed Mixed elements
            emitter.instruction("bl __rt_array_new");                           // allocate the indexed container
            abi::store_at_offset(emitter, "x0", ARRAY_SLOT);
            emitter.instruction("mov x9, x0");                                  // the stamp helper takes the array in a scratch register
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r10", INVOKER_TABLE_OFFSET);
            emitter.instruction("xor r11d, r11d");                              // no table means no declared names
            emitter.instruction("test r10, r10");                               // is there a table?
            emitter.instruction(&format!("jz {}", table_zero_label));           // keep the zero count
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // the table's first word is the number of declared names
            emitter.label(&table_zero_label);
            abi::store_at_offset(emitter, "r11", COUNT_SLOT);
            emitter.instruction("lea rdi, [r11 + 1]");                          // capacity: one slot per name, never zero
            emitter.instruction("mov rsi, 8");                                  // eight-byte boxed Mixed elements
            emitter.instruction("call __rt_array_new");                         // allocate the indexed container
            abi::store_at_offset(emitter, "rax", ARRAY_SLOT);
            emitter.instruction("mov r11, rax");                                // the stamp helper takes the array in a scratch register
        }
    }
    let stamp_reg = match arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r11",
    };
    crate::codegen::emit_array_value_type_stamp(emitter, stamp_reg, &PhpType::Mixed);
    emit_zero_slot(emitter, INDEX_SLOT);
    emit_zero_slot(emitter, BOUND_SLOT);

    // -- one iteration per declared name --
    emitter.label(&loop_label);
    match arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x9", INDEX_SLOT);
            abi::load_at_offset(emitter, "x10", COUNT_SLOT);
            emitter.instruction("cmp x9, x10");                                 // has every declared name been looked up?
            emitter.instruction(&format!("b.hs {}", done_label));               // yes: finish the container
            abi::load_at_offset(emitter, "x0", HASH_SLOT);
            abi::load_at_offset(emitter, "x1", INVOKER_TABLE_OFFSET);
            emitter.instruction("mov x2, x9");                                  // the parameter index being bound
            emitter.instruction("bl __rt_invoker_lookup_param");                // the name first, then the positional key
            emitter.instruction(&format!("cbnz x0, {}", found_label));          // an argument was supplied
            emitter.instruction(&format!("b {}", notfound_label));              // no argument for this parameter
            emitter.label(&found_label);
            emitter.instruction("cmp x3, #7");                                  // is the entry already a boxed Mixed cell?
            emitter.instruction(&format!("b.ne {}", boxit_label));              // no: box the raw payload
            abi::store_at_offset(emitter, "x1", ELEM_SLOT);
            emitter.instruction("mov x0, x1");                                  // retain the borrowed cell for the container
            emitter.instruction("bl __rt_incref");                              // the container shares the cell
            abi::load_at_offset(emitter, "x0", ELEM_SLOT);
            emitter.instruction(&format!("b {}", store_label));                 // store the shared cell
            emitter.label(&boxit_label);
            emitter.instruction("mov x0, x3");                                  // runtime tag
            emitter.instruction("bl __rt_mixed_from_value");                    // box the raw payload into an owned cell (x1/x2 hold it)
            emitter.label(&store_label);
            abi::load_at_offset(emitter, "x9", ARRAY_SLOT);
            abi::load_at_offset(emitter, "x10", INDEX_SLOT);
            emitter.instruction("add x9, x9, x10, lsl #3");                     // address the element slot, eight bytes per parameter
            emitter.instruction("str x0, [x9, #24]");                           // elements start after the 24-byte array header
            emitter.instruction("add x10, x10, #1");                            // the bound length is the highest bound index plus one
            abi::store_at_offset(emitter, "x10", BOUND_SLOT);
            emitter.instruction(&format!("b {}", next_label));                  // next parameter
            emitter.label(&notfound_label);
            abi::load_at_offset(emitter, "x9", INVOKER_TABLE_OFFSET);
            emitter.instruction("ldr x9, [x9, #8]");                            // the table's second word marks the required parameters
            abi::load_at_offset(emitter, "x10", INDEX_SLOT);
            emitter.instruction("lsr x9, x9, x10");                             // bring this parameter's bit down
            emitter.instruction("tst x9, #1");                                  // is it required?
            emitter.instruction(&format!("b.ne {}", missing_label));            // an omitted required argument
            abi::load_at_offset(emitter, "x9", ARRAY_SLOT);
            emitter.instruction("add x9, x9, x10, lsl #3");                     // address the element slot
            emitter.instruction("str xzr, [x9, #24]");                          // a hole: the indexed path uses the default
            emitter.label(&next_label);
            abi::load_at_offset(emitter, "x10", INDEX_SLOT);
            emitter.instruction("add x10, x10, #1");                            // advance to the next declared name
            abi::store_at_offset(emitter, "x10", INDEX_SLOT);
            emitter.instruction(&format!("b {}", loop_label));                  // keep binding
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r10", INDEX_SLOT);
            abi::load_at_offset(emitter, "r11", COUNT_SLOT);
            emitter.instruction("cmp r10, r11");                                // has every declared name been looked up?
            emitter.instruction(&format!("jae {}", done_label));                // yes: finish the container
            abi::load_at_offset(emitter, "rdi", HASH_SLOT);
            abi::load_at_offset(emitter, "rsi", INVOKER_TABLE_OFFSET);
            emitter.instruction("mov rdx, r10");                                // the parameter index being bound
            emitter.instruction("call __rt_invoker_lookup_param");              // the name first, then the positional key
            emitter.instruction("test rax, rax");                               // was an argument supplied?
            emitter.instruction(&format!("jnz {}", found_label));               // yes
            emitter.instruction(&format!("jmp {}", notfound_label));            // no argument for this parameter
            emitter.label(&found_label);
            emitter.instruction("cmp rcx, 7");                                  // is the entry already a boxed Mixed cell?
            emitter.instruction(&format!("jne {}", boxit_label));               // no: box the raw payload
            abi::store_at_offset(emitter, "rdi", ELEM_SLOT);
            emitter.instruction("mov rax, rdi");                                // retain the borrowed cell for the container
            emitter.instruction("call __rt_incref");                            // the container shares the cell
            abi::load_at_offset(emitter, "rax", ELEM_SLOT);
            emitter.instruction(&format!("jmp {}", store_label));               // store the shared cell
            emitter.label(&boxit_label);
            emitter.instruction("mov rax, rcx");                                // runtime tag
            emitter.instruction("call __rt_mixed_from_value");                  // box the raw payload into an owned cell (rdi/rsi hold it)
            emitter.label(&store_label);
            abi::load_at_offset(emitter, "r10", ARRAY_SLOT);
            abi::load_at_offset(emitter, "r11", INDEX_SLOT);
            emitter.instruction("mov QWORD PTR [r10 + r11 * 8 + 24], rax");     // elements start after the 24-byte array header
            emitter.instruction("lea r11, [r11 + 1]");                          // the bound length is the highest bound index plus one
            abi::store_at_offset(emitter, "r11", BOUND_SLOT);
            emitter.instruction(&format!("jmp {}", next_label));               // next parameter
            emitter.label(&notfound_label);
            abi::load_at_offset(emitter, "r10", INVOKER_TABLE_OFFSET);
            emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                // the table's second word marks the required parameters
            abi::load_at_offset(emitter, "rcx", INDEX_SLOT);
            emitter.instruction("shr r10, cl");                                 // bring this parameter's bit down
            emitter.instruction("test r10, 1");                                 // is it required?
            emitter.instruction(&format!("jnz {}", missing_label));             // an omitted required argument
            abi::load_at_offset(emitter, "r10", ARRAY_SLOT);
            emitter.instruction("mov QWORD PTR [r10 + rcx * 8 + 24], 0");       // a hole: the indexed path uses the default
            emitter.label(&next_label);
            abi::load_at_offset(emitter, "r10", INDEX_SLOT);
            emitter.instruction("add r10, 1");                                  // advance to the next declared name
            abi::store_at_offset(emitter, "r10", INDEX_SLOT);
            emitter.instruction(&format!("jmp {}", loop_label));               // keep binding
        }
    }

    // -- done: publish the bound length and return the container --
    emitter.label(&done_label);
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, result, ARRAY_SLOT);
    abi::load_at_offset(emitter, scratch, BOUND_SLOT);
    abi::emit_store_to_address(emitter, scratch, result, 0);
    abi::emit_jump(emitter, &exit_label);

    // -- an omitted required argument: release the partial container and answer zero --
    emitter.label(&missing_label);
    abi::load_at_offset(emitter, result, ARRAY_SLOT);
    abi::load_at_offset(emitter, scratch, BOUND_SLOT);
    abi::emit_store_to_address(emitter, scratch, result, 0);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_load_int_immediate(emitter, result, 0);

    emitter.label(&exit_label);
    // The result register carries the answer; the saved register is restored through a scratch.
    abi::load_at_offset(emitter, scratch, SAVED_SLOT);
    emitter.instruction(&format!("mov {}, {}", hash_reg, scratch));             // give the caller its callee-saved register back
    abi::emit_frame_restore(emitter, FRAME_SIZE);
    abi::emit_return(emitter);
}

fn emit_zero_slot(emitter: &mut Emitter, offset: usize) {
    let zero = abi::secondary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, zero, 0);
    abi::store_at_offset(emitter, zero, offset);
}
