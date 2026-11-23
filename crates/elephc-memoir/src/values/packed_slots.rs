//! Purpose:
//! Publishes initialized slots and transfers existing owners in packed native arrays.
//!
//! Called from:
//! - Preallocated values builders and indexed Mixed overwrite emitters.
//!
//! Key details:
//! - The array owner remains live while its slot is updated before retirement can reenter.

use crate::{Arch, Emitter};

/// Appends a 16-byte string payload into the AArch64 result values array.
pub fn emit_append_string_value_aarch64(emitter: &mut impl Emitter, ptr_reg: &str, len_reg: &str) {
    emitter.instruction("ldr x9, [sp, #16]");                                   // load the result values array pointer from the fixed stack layout
    emitter.instruction("ldr x10, [x9]");                                       // load the current result values array length before appending
    emitter.instruction("lsl x11, x10, #4");                                    // convert the result length into a 16-byte string-slot offset
    emitter.instruction("add x11, x9, x11");                                    // advance from the array base to the selected string slot
    emitter.instruction("add x11, x11, #24");                                   // skip the fixed indexed-array header to reach the payload region
    emitter.instruction(&format!("str {}, [x11]", ptr_reg));                    // store the owned string pointer into the next result values slot
    emitter.instruction(&format!("str {}, [x11, #8]", len_reg));                // store the owned string length into the next result values slot
    emitter.instruction("add x10, x10, #1");                                    // increment the result values array length after the append
    emitter.instruction("str x10, [x9]");                                       // persist the updated result values array length in the header
}

/// Appends an 8-byte payload into the AArch64 result values array.
pub fn emit_append_word_value_aarch64(emitter: &mut impl Emitter, value_reg: &str) {
    emitter.instruction("ldr x9, [sp, #16]");                                   // load the result values array pointer from the fixed stack layout
    emit_append_word_aarch64(emitter, "x9", value_reg);
}

/// Appends one word to the caller-selected preallocated array register.
fn emit_append_word_aarch64(emitter: &mut impl Emitter, array: &str, value_reg: &str) {
    emitter.instruction(&format!("ldr x10, [{array}]"));                        // load the current result values array length before appending
    emitter.instruction(&format!("add x11, {array}, #24"));                     // point at the result values array payload region after the fixed header
    emitter.instruction(&format!("str {}, [x11, x10, lsl #3]", value_reg));     // store the value payload into the next result values slot
    emitter.instruction("add x10, x10, #1");                                    // increment the result values array length after the append
    emitter.instruction(&format!("str x10, [{array}]"));                        // persist the updated result values array length in the header
}

/// Appends a 16-byte string payload into the x86_64 result values array.
pub fn emit_append_string_value_x86_64(emitter: &mut impl Emitter, ptr_reg: &str, len_reg: &str) {
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // load the result values array pointer from the fixed stack layout
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current result values array length before appending
    emitter.instruction("mov rcx, r11");                                        // copy the result length before scaling it into a string-slot offset
    emitter.instruction("shl rcx, 4");                                          // convert the result length into a 16-byte string-slot offset
    emitter.instruction("add rcx, r10");                                        // advance from the array base to the selected string slot
    emitter.instruction("add rcx, 24");                                         // skip the fixed indexed-array header to reach the payload region
    emitter.instruction(&format!("mov QWORD PTR [rcx], {}", ptr_reg));          // store the owned string pointer into the next result values slot
    emitter.instruction(&format!("mov QWORD PTR [rcx + 8], {}", len_reg));      // store the owned string length into the next result values slot
    emitter.instruction("add r11, 1");                                          // increment the result values array length after the append
    emitter.instruction("mov QWORD PTR [r10], r11");                            // persist the updated result values array length in the header
}

/// Appends an 8-byte payload into the x86_64 result values array.
pub fn emit_append_word_value_x86_64(emitter: &mut impl Emitter, value_reg: &str) {
    emitter.instruction("mov r10, QWORD PTR [rsp + 16]");                       // load the result values array pointer from the fixed stack layout
    emit_append_word_x86_64(emitter, "r10", "r11", value_reg);
}

/// Appends one word to the caller-selected preallocated array register.
fn emit_append_word_x86_64(emitter: &mut impl Emitter, array: &str, length: &str, value_reg: &str) {
    emitter.instruction(&format!("mov {length}, QWORD PTR [{array}]"));         // load the current result values array length before appending
    emitter.instruction(                                                        // store the value payload into the next result values slot
        &format!("mov QWORD PTR [{array} + {length} * 8 + 24], {}", value_reg)
    );
    emitter.instruction(&format!("add {length}, 1"));                           // increment the result values array length after the append
    emitter.instruction(&format!("mov QWORD PTR [{array}], {length}"));         // persist the updated result values array length in the header
}

/// Publishes an overwritten Mixed array slot and retires its displaced owner.
pub fn emit_publish_mixed_overwrite_aarch64(emitter: &mut impl Emitter) {
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the attached array before any destructor can reenter
    emitter.instruction("ldr x9, [sp, #40]");                                   // recover the index after marker inspection
    emitter.instruction("ldr x11, [sp, #24]");                                  // transfer the consumed replacement into the attached slot
    emitter.instruction("add x12, x10, #24");                                   // locate the existing slot while its array remains valid
    emitter.instruction("str x11, [x12, x9, lsl #3]");                          // publish before retiring the previous owner's final reference
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(emitter, "__rt_decref_mixed", "x0");
}

/// Publishes an overwritten Mixed array slot and retires its displaced owner.
pub fn emit_publish_mixed_overwrite_x86_64(emitter: &mut impl Emitter) {
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the attached array before any destructor can reenter
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // recover the index after marker inspection
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // transfer the consumed replacement into the attached slot
    emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], r11");              // publish before retiring the previous owner's final reference
    crate::cleanup_preserve::emit_unary_cleanup_preserving_exception(emitter, "__rt_decref_mixed", "rax");
}

/// Frame slots for a relocated indexed array attached to a Mixed cell.
///
/// Offsets are relative to the current SP on AArch64, and distances below RBP on x86_64.
pub struct AttachedPackedSlots {
    /// Slot holding the current array address.
    pub array: usize,
    /// Slot holding the requested index.
    pub index: usize,
    /// Slot holding the owner cell.
    pub owner: usize,
}

impl AttachedPackedSlots {
    /// Grows until the index fits, publishing every relocation back to the cell.
    ///
    /// Ends with the current array pointer and index in x10/x9 or r10/r9.
    pub fn emit_grow_to_index(&self, emitter: &mut impl Emitter, prefix: &str) {
        let check = format!("{prefix}_grow_check");
        let ready = format!("{prefix}_grow_ready");
        let (array, index, capacity, fits) = match emitter.arch() {
            Arch::AArch64 => ("x10", "x9", "x12", "b.lo"),
            Arch::X86_64 => ("r10", "r9", "r11", "jb"),
        };
        emitter.label(&check);
        Self::emit_load_slot(emitter, array, self.array);
        Self::emit_load_slot(emitter, index, self.index);
        emitter.emit_load_from_address(capacity, array, 8);
        emitter.instruction(&format!("cmp {index}, {capacity}"));               // compare the requested index with the allocated slot count
        emitter.instruction(&format!("{fits} {ready}"));                        // use the current array when the index fits
        emitter.emit_reg_move(emitter.int_arg_reg_name(0), array);
        emitter.emit_call_label("__rt_array_grow");
        let result = emitter.int_result_reg();
        match emitter.arch() {
            Arch::AArch64 => emitter.instruction(&format!("str {result}, [sp, #{}]", self.array)), // save the relocated pointer in the active SP-relative frame
            Arch::X86_64 => emitter.store_at_offset(result, self.array),
        }
        Self::emit_load_slot(emitter, array, self.owner);
        emitter.emit_store_to_address(result, array, 8);
        emitter.emit_jump(&check);
        emitter.label(&ready);
    }

    /// Publishes the fresh child and retains an independent result owner.
    pub fn emit_install_child(&self, emitter: &mut impl Emitter) {
        self.emit_store_child(emitter);
        retain_selected_child(emitter);
    }

    /// Detaches a selected child, then reacquires the installed slot after retirement.
    pub fn emit_detach_child(&self, emitter: &mut impl Emitter, replaced: usize) {
        let result = emitter.int_result_reg();
        match emitter.arch() {
            Arch::AArch64 => emitter.emit_store_to_sp(result, replaced),
            Arch::X86_64 => emitter.store_at_offset(result, replaced),
        }
        emitter.emit_call_label("__rt_mixed_clone");
        self.emit_store_child(emitter);
        Self::emit_load_slot(emitter, result, replaced);
        emitter.emit_call_label("__rt_decref_mixed");

        let (array, index, address) = selected_child_registers(emitter.arch());
        Self::emit_load_slot(emitter, array, self.array);
        Self::emit_load_slot(emitter, index, self.index);
        emit_load_indexed_word(emitter, array, index, result, address);
        retain_selected_child(emitter);
    }

    /// Transfers the prepared result into the selected word in the current array.
    fn emit_store_child(&self, emitter: &mut impl Emitter) {
        let result = emitter.int_result_reg();
        let (array, index, address) = selected_child_registers(emitter.arch());
        Self::emit_load_slot(emitter, array, self.array);
        Self::emit_load_slot(emitter, index, self.index);
        match emitter.arch() {
            Arch::AArch64 => emit_store_indexed_word(emitter, array, index, result, address),
            Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [{array} + {PACKED_PAYLOAD_OFFSET} + {index} * 8], {result}")), // publish into the parent before retiring the displaced child
        }
    }

    /// Loads a slot without translating its offset to a different frame base.
    fn emit_load_slot(emitter: &mut impl Emitter, register: &str, offset: usize) {
        match emitter.arch() {
            Arch::AArch64 => emitter.instruction(&format!("ldr {register}, [sp, #{offset}]")), // resolve an SP-relative caller slot
            Arch::X86_64 => emitter.load_at_offset(register, offset),
        }
    }
}

/// Returns the registers that address a child slot in a write fetch.
fn selected_child_registers(arch: Arch) -> (&'static str, &'static str, &'static str) {
    match arch {
        Arch::AArch64 => ("x10", "x9", "x13"),
        Arch::X86_64 => ("r10", "r9", "r10"),
    }
}

/// Retains the selected child without losing its pointer in the result register.
fn retain_selected_child(emitter: &mut impl Emitter) {
    let result = emitter.int_result_reg();
    if emitter.arch() == Arch::X86_64 {
        emitter.emit_push_reg(result);
    }
    emitter.emit_call_label("__rt_incref");
    if emitter.arch() == Arch::X86_64 {
        emitter.emit_pop_reg(result);
    }
}

/// Clears newly exposed gap slots and publishes a length covering the target index.
///
/// Array, target index and old length arrive in x10/x9/x11 or r10/r9/r11. For fetch-for-write
/// the target slot is still empty and is cleared too, before a child allocation can observe it.
pub fn emit_publish_mixed_extension(
    emitter: &mut impl Emitter, prefix: &str, clear_target: bool,
) {
    let fill = format!("{prefix}_fill_gap");
    let publish = format!("{prefix}_publish_length");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("add x13, x10, #24");                           // skip the fixed indexed-array header
            emitter.label(&fill);
            emitter.instruction("cmp x11, x9");                                 // compare the gap cursor with the written index
            emitter.instruction(&format!("b.hs {publish}"));                    // stop before the target slot
            emitter.instruction("str xzr, [x13, x11, lsl #3]");                 // initialize the newly visible slot to null
            emitter.instruction("add x11, x11, #1");                            // advance to the next gap slot
            emitter.emit_jump(&fill);
            emitter.label(&publish);
            if clear_target {
                emitter.instruction("str xzr, [x13, x9, lsl #3]");              // make the pending child slot safe for heap traversal
            }
            emitter.instruction("add x12, x9, #1");                             // include the target slot in the logical length
            emitter.instruction("str x12, [x10]");                              // publish the initialized extent
        }
        Arch::X86_64 => {
            emitter.label(&fill);
            emitter.instruction("cmp r11, r9");                                 // compare the gap cursor with the written index
            emitter.instruction(&format!("jae {publish}"));                     // stop before the target slot
            emitter.instruction("mov QWORD PTR [r10 + 24 + r11 * 8], 0");       // initialize the newly visible slot to null
            emitter.instruction("add r11, 1");                                  // advance to the next gap slot
            emitter.emit_jump(&fill);
            emitter.label(&publish);
            if clear_target {
                emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], 0");    // make the pending child slot safe for heap traversal
            }
            emitter.instruction("lea r8, [r9 + 1]");                            // include the target slot in the logical length
            emitter.instruction("mov QWORD PTR [r10], r8");                     // publish the initialized extent
        }
    }
}

/// Separates, grows and appends a tagged scalar from two saved temporary slots.
pub fn emit_append_tagged_scalar_aarch64(
    emitter: &mut impl Emitter, tag: crate::ValueTag, check_label: &str, grow_label: &str, done_label: &str,
) {
    emitter.emit_call_label("__rt_array_ensure_unique");
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the unique indexed-array pointer across the capacity check
    emitter.instruction("ldr x9, [x0]");                                        // load length before first-write tagged-scalar shape specialization
    emitter.instruction(&format!("cbnz x9, {}", check_label));                  // existing arrays already have their tagged-scalar shape fixed
    emitter.instruction("mov x10, #16");                                        // tagged-scalar slots store payload and runtime tag words
    emitter.instruction("str x10, [x0, #16]");                                  // elem_size = 16 before growth can copy tagged-scalar slots
    super::array_header::emit_array_value_type_stamp(emitter, "x0", tag);
    emitter.label(check_label);
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the current indexed-array pointer before checking capacity
    emitter.instruction("ldr x9, [x0]");                                        // load the current logical length
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the current capacity
    emitter.instruction("cmp x9, x10");                                         // is the tagged-scalar array already full?
    emitter.instruction(&format!("b.ge {}", grow_label));                       // grow before writing when the append would exceed capacity
    emitter.instruction("lsl x10, x9, #4");                                     // convert length to a byte offset for 16-byte tagged-scalar slots
    emitter.instruction("add x10, x0, x10");                                    // move to the selected append slot base
    emitter.instruction("add x10, x10, #24");                                   // skip the indexed-array header before storing the slot
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the tagged-scalar payload for the appended slot
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload the tagged-scalar runtime tag for the appended slot
    emitter.instruction("str x11, [x10]");                                      // store the tagged-scalar payload word in the append slot
    emitter.instruction("str x12, [x10, #8]");                                  // store the tagged-scalar runtime tag word in the append slot
    emitter.instruction("add x9, x9, #1");                                      // advance the indexed-array logical length
    emitter.instruction("str x9, [x0]");                                        // publish the updated logical length
    emitter.instruction(&format!("b {}", done_label));                          // skip the growth path after storing the tagged-scalar slot
    emitter.label(grow_label);
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the unique indexed-array pointer for growth
    emitter.emit_call_label("__rt_array_grow");
    emitter.instruction("str x0, [sp, #16]");                                   // preserve the grown indexed-array pointer before retrying the append
    emitter.instruction(&format!("b {}", check_label));                         // retry the capacity check against the grown storage
    emitter.label(done_label);
    emitter.instruction("add sp, sp, #32");                                     // release tagged-scalar append spill slots
}

/// Separates, grows and appends a tagged scalar from two saved temporary slots.
pub fn emit_append_tagged_scalar_x86_64(
    emitter: &mut impl Emitter, tag: crate::ValueTag, check_label: &str, grow_label: &str, done_label: &str,
) {
    emitter.emit_call_label("__rt_array_ensure_unique");
    emitter.instruction("mov QWORD PTR [rsp + 16], rax");                       // preserve the unique indexed-array pointer across the capacity check
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load length before first-write tagged-scalar shape specialization
    emitter.instruction("test r10, r10");                                       // is this the first append into a tagged-scalar array?
    emitter.instruction(&format!("jnz {}", check_label));                       // existing arrays already have their tagged-scalar shape fixed
    emitter.instruction("mov QWORD PTR [rax + 16], 16");                        // elem_size = 16 before growth can copy tagged-scalar slots
    super::array_header::emit_array_value_type_stamp(emitter, "rax", tag);
    emitter.label(check_label);
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // reload the current indexed-array pointer before checking capacity
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the current logical length
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // load the current capacity
    emitter.instruction("cmp r10, r11");                                        // is the tagged-scalar array already full?
    emitter.instruction(&format!("jae {}", grow_label));                        // grow before writing when the append would exceed capacity
    emitter.instruction("mov rcx, r10");                                        // copy the logical length before scaling it into a byte offset
    emitter.instruction("shl rcx, 4");                                          // convert length to a byte offset for 16-byte tagged-scalar slots
    emitter.instruction("lea rcx, [rax + rcx + 24]");                           // compute the address of the next tagged-scalar append slot
    emitter.instruction("mov r8, QWORD PTR [rsp]");                             // reload the tagged-scalar payload for the appended slot
    emitter.instruction("mov r9, QWORD PTR [rsp + 8]");                         // reload the tagged-scalar runtime tag for the appended slot
    emitter.instruction("mov QWORD PTR [rcx], r8");                             // store the tagged-scalar payload word in the append slot
    emitter.instruction("mov QWORD PTR [rcx + 8], r9");                         // store the tagged-scalar runtime tag word in the append slot
    emitter.instruction("add r10, 1");                                          // advance the indexed-array logical length
    emitter.instruction("mov QWORD PTR [rax], r10");                            // publish the updated logical length
    emitter.instruction(&format!("jmp {}", done_label));                        // skip the growth path after storing the tagged-scalar slot
    emitter.label(grow_label);
    emitter.instruction("mov rdi, rax");                                        // pass the unique indexed-array pointer to the growth helper
    emitter.emit_call_label("__rt_array_grow");
    emitter.instruction("mov QWORD PTR [rsp + 16], rax");                       // preserve the grown indexed-array pointer before retrying the append
    emitter.instruction(&format!("jmp {}", check_label));                       // retry the capacity check against the grown storage
    emitter.label(done_label);
    emitter.instruction("add rsp, 32");                                         // release tagged-scalar append spill slots
}

/// Appends one word when the caller has established enough capacity.
pub fn emit_append_reserved_word(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => emit_append_word_aarch64(emitter, "x9", "x1"),
        Arch::X86_64 => emit_append_word_x86_64(emitter, "rdi", "r10", "rsi"),
    }
}

/// Prepares a unique indexed array for a writable element address.
pub fn emit_prepare_address_aarch64(
    emitter: &mut impl Emitter, wide: bool, labels: [&str; 5],
) {
    let [grow_check, ready, fill_loop, store_len, done] = labels;
    emitter.label(grow_check);
    emitter.instruction("ldr x10, [x0, #8]");                                   // load indexed-array capacity before exposing an element address
    emitter.instruction("cmp x1, x10");                                         // does the referenced slot fit in the current allocation?
    emitter.instruction(&format!("b.lo {}", ready));                            // skip growth once the slot is addressable
    emitter.emit_push_reg("x1");
    emitter.emit_call_label("__rt_array_grow");
    emitter.emit_pop_reg("x1");
    emitter.instruction(&format!("b {}", grow_check));                          // keep growing until the by-reference slot fits
    emitter.label(ready);
    emitter.instruction("ldr x9, [x0]");                                        // load current logical length before filling missing by-reference slots
    emitter.instruction("cmp x1, x9");                                          // is the referenced slot already inside the logical array length?
    emitter.instruction(&format!("b.lo {}", done));                             // existing slots can be referenced without extending length
    emitter.instruction("mov x11, x9");                                         // start zero-filling at the previous logical end
    emitter.label(fill_loop);
    emitter.instruction("cmp x11, x1");                                         // have all gap slots before the referenced slot been initialized?
    emitter.instruction(&format!("b.ge {}", store_len));                        // stop filling before the referenced slot
    emit_zero_aarch64(emitter, wide, "x0", "x11");
    emitter.instruction("add x11, x11, #1");                                    // advance to the next gap slot
    emitter.instruction(&format!("b {}", fill_loop));                           // continue zero-filling until the referenced slot
    emitter.label(store_len);
    emit_zero_aarch64(emitter, wide, "x0", "x11");
    emitter.instruction("add x11, x1, #1");                                     // compute new logical length after materializing the reference slot
    emitter.instruction("str x11, [x0]");                                       // publish the extended indexed-array length
    emitter.label(done);
}

/// Initializes the physical words of a previously hidden slot.
fn emit_zero_aarch64(emitter: &mut impl Emitter, wide: bool, array_reg: &str, index_reg: &str) {
    match wide {
        false => {
            emitter.instruction(&format!("add x12, {}, #24", array_reg));       // compute the base address of pointer-sized indexed-array slots
            emitter
                .instruction(
                &format!("str xzr, [x12, {}, lsl #3]", index_reg)
            );                                                                  // initialize the missing by-reference slot to null
        }
        true => {
            emitter.instruction(&format!("lsl x12, {}, #4", index_reg));        // scale the gap index by the two-word slot size
            emitter.instruction(&format!("add x12, {}, x12", array_reg));       // move to the selected two-word indexed-array slot
            emitter.instruction("add x12, x12, #24");                           // skip the indexed-array header before clearing the slot
            emitter.instruction("stp xzr, xzr, [x12]");                         // initialize both words of the missing by-reference slot
        }
    }
}

/// Prepares a unique indexed array for a writable element address.
pub fn emit_prepare_address_x86_64(
    emitter: &mut impl Emitter, wide: bool, labels: [&str; 5],
) {
    let [grow_check, ready, fill_loop, store_len, done] = labels;
    emitter.label(grow_check);
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load indexed-array capacity before exposing an element address
    emitter.instruction("cmp rsi, r10");                                        // does the referenced slot fit in the current allocation?
    emitter.instruction(&format!("jb {}", ready));                              // skip growth once the slot is addressable
    emitter.emit_push_reg("rsi");
    emitter.instruction("mov rdi, rax");                                        // pass the current indexed-array pointer to the growth helper
    emitter.emit_call_label("__rt_array_grow");
    emitter.emit_pop_reg("rsi");
    emitter.instruction(&format!("jmp {}", grow_check));                        // keep growing until the by-reference slot fits
    emitter.label(ready);
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // load current logical length before filling missing by-reference slots
    emitter.instruction("cmp rsi, r9");                                         // is the referenced slot already inside the logical array length?
    emitter.instruction(&format!("jb {}", done));                               // existing slots can be referenced without extending length
    emitter.instruction("mov r11, r9");                                         // start zero-filling at the previous logical end
    emitter.label(fill_loop);
    emitter.instruction("cmp r11, rsi");                                        // have all gap slots before the referenced slot been initialized?
    emitter.instruction(&format!("jae {}", store_len));                         // stop filling before the referenced slot
    emit_zero_x86_64(emitter, wide, "rax", "r11");
    emitter.instruction("add r11, 1");                                          // advance to the next gap slot
    emitter.instruction(&format!("jmp {}", fill_loop));                         // continue zero-filling until the referenced slot
    emitter.label(store_len);
    emit_zero_x86_64(emitter, wide, "rax", "r11");
    emitter.instruction("lea r11, [rsi + 1]");                                  // compute new logical length after materializing the reference slot
    emitter.instruction("mov QWORD PTR [rax], r11");                            // publish the extended indexed-array length
    emitter.label(done);
}

/// Initializes the physical words of a previously hidden slot.
fn emit_zero_x86_64(emitter: &mut impl Emitter, wide: bool, array_reg: &str, index_reg: &str) {
    match wide {
        false => {
            let clear_slot = format!(
                "mov QWORD PTR [{} + 24 + {} * 8], 0",
                array_reg, index_reg
            );
            emitter.instruction(&clear_slot);                                   // initialize the missing by-reference slot to null
        }
        true => {
            emitter.instruction(&format!("mov r10, {}", index_reg));            // copy the gap index before scaling for a two-word slot
            emitter.instruction("shl r10, 4");                                  // scale the gap index by the two-word slot size
            emitter
                .instruction(
                &format!("mov QWORD PTR [{} + 24 + r10], 0", array_reg)
            );                                                                  // initialize the first word of the missing slot
            emitter
                .instruction(
                &format!("mov QWORD PTR [{} + 32 + r10], 0", array_reg)
            );                                                                  // initialize the second word of the missing slot
        }
    }
}

/// Byte width of the length, capacity and stride words preceding packed slots.
/// Byte offset of the first slot after the packed array header.
pub const PACKED_PAYLOAD_OFFSET: usize = 24;

/// Borrows one word from a packed payload whose logical length and representation were checked.
pub fn emit_load_packed_word(emitter: &mut impl Emitter, destination: &str, array: &str, word: usize) {
    emitter.emit_load_from_address(destination, array, PACKED_PAYLOAD_OFFSET + word * 8);
}

/// Borrows the two words of a checked packed slot, in payload order.
pub fn emit_load_packed_pair(
    emitter: &mut impl Emitter, low: &str, high: &str, array: &str, slot: usize,
) {
    emit_load_packed_word(emitter, low, array, slot * 2);
    emit_load_packed_word(emitter, high, array, slot * 2 + 1);
}

/// Forms the AArch64 payload base from a validated packed array pointer.
pub fn emit_packed_payload_aarch64(emitter: &mut impl Emitter, destination: &str, array: &str) {
    emitter.instruction(&format!("add {destination}, {array}, #{PACKED_PAYLOAD_OFFSET}")); // skip the packed array header
}

/// Forms the x86_64 payload base from a validated packed array pointer.
pub fn emit_packed_payload_x86_64(emitter: &mut impl Emitter, destination: &str, array: &str) {
    emitter.instruction(&format!("lea {destination}, [{array} + {PACKED_PAYLOAD_OFFSET}]")); // skip the packed array header
}

/// Borrows a checked word-sized slot at a dynamic index.
pub fn emit_load_indexed_word(
    emitter: &mut impl Emitter, array: &str, index: &str, value: &str, address: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emit_packed_payload_aarch64(emitter, address, array);
            emitter.instruction(&format!("ldr {value}, [{address}, {index}, lsl #3]")); // borrow the checked packed slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {value}, QWORD PTR [{array} + {PACKED_PAYLOAD_OFFSET} + {index} * 8]")); // borrow the checked packed slot
        }
    }
}

/// Writes one prepared owner into a reserved word-sized slot at a dynamic index.
pub fn emit_store_indexed_word(
    emitter: &mut impl Emitter, array: &str, index: &str, value: &str, address: &str,
) {
    match emitter.arch() {
        Arch::AArch64 => {
            emit_packed_payload_aarch64(emitter, address, array);
            emitter.instruction(&format!("str {value}, [{address}, {index}, lsl #3]")); // transfer the prepared word to its slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [{array} + {index} * 8 + {PACKED_PAYLOAD_OFFSET}], {value}")); // transfer the prepared word to its slot
        }
    }
}

/// Initializes a constant-index word slot and publishes the initialized prefix through that slot.
pub fn emit_publish_word_at(
    emitter: &mut impl Emitter, array: &str, value: &str, length: &str, index: usize,
) {
    emitter.emit_store_to_address(value, array, PACKED_PAYLOAD_OFFSET + index * 8);
    emitter.emit_load_int_immediate(length, (index + 1) as i64);
    emit_publish_length(emitter, array, length);
}

/// Publishes the number of initialized slots to the native packed array.
pub fn emit_publish_length(emitter: &mut impl Emitter, array: &str, length: &str) {
    emitter.emit_store_to_address(length, array, 0);
}

/// Reads the logical length of checked packed storage.
pub fn emit_load_length(emitter: &mut impl Emitter, destination: &str, array: &str) {
    emitter.emit_load_from_address(destination, array, 0);
}

/// Loads the logical length and reserved capacity without touching the payload.
pub fn emit_load_length_and_capacity(
    emitter: &mut impl Emitter, array: &str, length: &str, capacity: &str,
) {
    emitter.emit_load_from_address(length, array, 0);
    emitter.emit_load_from_address(capacity, array, 8);
}

/// Branches around single-slot growth when an initialized array has spare capacity.
/// The array is already in the native grow helper's argument register.
pub fn emit_grow_if_full(emitter: &mut impl Emitter, spare: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emit_load_length_and_capacity(emitter, "x0", "x9", "x10");
            emitter.instruction("cmp x9, x10");                                // check the old length against reserved slots
            emitter.instruction(&format!("b.lt {spare}"));                     // the next slot is available without relocating
        }
        Arch::X86_64 => {
            emit_load_length_and_capacity(emitter, "rdi", "r10", "r11");
            emitter.instruction("cmp r10, r11");                              // check the old length against reserved slots
            emitter.instruction(&format!("jb {spare}"));                       // the next slot is available without relocating
        }
    }
    emit_grow(emitter);
}

/// Grows full packed storage; the caller publishes the resulting owner to its receiver.
pub fn emit_grow(emitter: &mut impl Emitter) {
    emitter.emit_call_label("__rt_array_grow");
}

/// Moves the last owned word to the front without changing length or retaining any element.
/// The nonempty packed array remains in the native result register.
pub fn emit_rotate_tail_to_front(emitter: &mut impl Emitter, shift: &str, insert: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0]");                                // load the post-append brigade length
            emitter.instruction("sub x9, x9, #1");                              // point at the appended bucket's last slot
            emitter.instruction("add x10, x0, #24");                            // compute the brigade payload base
            emitter.instruction("ldr x11, [x10, x9, lsl #3]");                  // preserve the appended bucket while shifting
            emitter.label(shift);
            emitter.instruction("cmp x9, #0");                                  // check whether the front slot is now available
            emitter.instruction(&format!("b.eq {}", insert));                   // finish once every prior bucket moved right
            emitter.instruction("sub x12, x9, #1");                             // select the preceding bucket slot
            emitter.instruction("ldr x13, [x10, x12, lsl #3]");                 // load the preceding bucket pointer
            emitter.instruction("str x13, [x10, x9, lsl #3]");                  // shift the preceding bucket one slot right
            emitter.instruction("mov x9, x12");                                 // continue toward the brigade head
            emitter.instruction(&format!("b {}", shift));                       // shift the next preceding bucket
            emitter.label(insert);
            emitter.instruction("str x11, [x10]");                              // install the prepended bucket at index zero
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rax]");                    // load the post-append brigade length
            emitter.instruction("sub r10, 1");                                  // point at the appended bucket's last slot
            emitter.instruction("lea r11, [rax + 24]");                         // compute the brigade payload base
            emitter.instruction("mov r8, QWORD PTR [r11 + r10 * 8]");           // preserve the appended bucket while shifting
            emitter.label(shift);
            emitter.instruction("test r10, r10");                               // check whether the front slot is now available
            emitter.instruction(&format!("jz {}", insert));                     // finish once every prior bucket moved right
            emitter.instruction("lea rcx, [r10 - 1]");                          // select the preceding bucket slot
            emitter.instruction("mov rdx, QWORD PTR [r11 + rcx * 8]");          // load the preceding bucket pointer
            emitter.instruction("mov QWORD PTR [r11 + r10 * 8], rdx");          // shift the preceding bucket one slot right
            emitter.instruction("mov r10, rcx");                                // continue toward the brigade head
            emitter.instruction(&format!("jmp {}", shift));                     // shift the next preceding bucket
            emitter.label(insert);
            emitter.instruction("mov QWORD PTR [r11], r8");                     // install the prepended bucket at index zero
        }
    }
}

/// Publishes the new packed-array address and acquires the appended word's owner.
/// The child offset is relative to the temporary stack after spilling the result.
pub fn emit_publish_appended_owner(emitter: &mut impl Emitter, array_slot: usize, child_slot: usize) {
    let stack = if emitter.arch() == Arch::AArch64 { "sp" } else { "rsp" };
    let result = emitter.int_result_reg();
    emitter.emit_store_to_address(result, stack, array_slot);
    emitter.emit_push_reg(result);
    emitter.emit_load_from_address(result, stack, child_slot);
    emitter.emit_call_label("__rt_incref");
    emitter.emit_pop_reg(result);
}
