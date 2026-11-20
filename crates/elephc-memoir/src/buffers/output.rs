//! Purpose:
//! Emits storage operations for the nested output capture buffers.
//!
//! Called from:
//! - The compiler's output-buffering runtime and web request reset.
//!
//! Key details:
//! - The compiler chooses capacity policy, handler phases and output routing.
//! - A closed slot is detached before payload releases and before its handler can reenter.
//! - These routines emit target assembly from the compiler.

use crate::{Arch, Emitter};

/// Number of slots in the output-buffer stack.
pub const MAX_LEVELS: i64 = 64;

/// One field of the per-level native output storage.
pub enum BufferField {
    /// The owned byte allocation, borrowed by this read.
    Pointer,
    /// The initialized byte count.
    Length,
    /// The allocated byte capacity.
    Capacity,
}

/// Loads the requested field for a level already checked by the caller.
pub fn emit_load_buffer_field(
    emitter: &mut impl Emitter, field: BufferField, address: &str, level: &str, destination: &str,
) {
    let symbol = match field {
        BufferField::Pointer => "_ob_ptrs",
        BufferField::Length => "_ob_lens",
        BufferField::Capacity => "_ob_caps",
    };
    emitter.emit_symbol_address(address, symbol);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("ldr {destination}, [{address}, {level}, lsl #3]")), // project one field of the selected slot
        Arch::X86_64 => emitter.instruction(&format!("mov {destination}, QWORD PTR [{address} + {level}*8]")), // project one field of the selected slot
    }
}

/// Allocates and publishes the slot's byte storage from the capacity selected by the caller.
/// Keeps the depth and slot index in their native start-helper registers.
pub fn emit_allocate_slot(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => aarch64_allocate_slot(emitter),
        Arch::X86_64 => x86_64_allocate_slot(emitter),
    }
}

/// Appends the staged bytes and leaves the updated length and index for the flush policy.
pub fn emit_append_bytes(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => aarch64_append_bytes(emitter),
        Arch::X86_64 => x86_64_append_bytes(emitter),
    }
}

/// Per-level word arrays carrying the handler owners.
const HANDLER_OWNERS: [&str; 3] = ["_ob_handler_stubs", "_ob_handler_envs", "_ob_name_ptrs"];
/// Per-level fields cleared when the owners detach.
const HANDLER_METADATA: [&str; 4] = ["_ob_name_lens", "_ob_chunk_sizes", "_ob_flags", "_ob_started"];

/// Declares the handler rows with the same level limit as the output storage.
pub fn append_handler_storage(out: &mut String, target: &impl crate::data::DataTarget) {
    for field in HANDLER_OWNERS.into_iter().chain(HANDLER_METADATA) {
        out.push_str(&target.common(field, MAX_LEVELS as usize * 8));
    }
}

/// Publishes initialized handler storage and then makes the next output level visible.
pub fn emit_publish_slot(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::AArch64 {
        emitter.emit_symbol_address("x11", "_ob_handler_stubs");              // materialize the handler-stub slot array
        emitter.instruction("ldr x12, [sp, #0]");                                   // reload the handler stub
        emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the handler stub
        emitter.emit_symbol_address("x11", "_ob_handler_envs");               // materialize the handler-env slot array
        emitter.instruction("ldr x12, [sp, #8]");                                   // reload the handler env word
        emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the handler env word
        emitter.emit_symbol_address("x11", "_ob_name_ptrs");                  // materialize the handler-name pointer array
        emitter.instruction("ldr x12, [sp, #32]");                                  // reload the owned name pointer
        emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the owned name pointer
        emitter.emit_symbol_address("x11", "_ob_name_lens");                  // materialize the handler-name length array
        emitter.instruction("ldr x12, [sp, #40]");                                  // reload the owned name length
        emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the owned name length
        emitter.emit_symbol_address("x11", "_ob_chunk_sizes");                // materialize the chunk-size slot array
        emitter.instruction("ldr x12, [sp, #16]");                                  // reload the chunk size
        emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the chunk size
        emitter.emit_symbol_address("x11", "_ob_flags");                      // materialize the flags slot array
        emitter.instruction("ldr x12, [sp, #24]");                                  // reload the flags word
        emitter.instruction("and x12, x12, #0xfffffffffffffff0");                   // handler type is selected by the runtime rather than caller flags
        emitter.instruction("bic x12, x12, #0xf000");                               // PHP 8.4 and later forbid caller-supplied status bits
        emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the flags word
        emitter.emit_symbol_address("x11", "_ob_started");                    // materialize the started-flag slot array
        emitter.instruction("str xzr, [x11, x10, lsl #3]");                         // the handler has not run yet
        emitter.instruction("add x10, x10, #1");                                    // the stack is now one level deeper
        emitter.instruction("str x10, [x9]");                                       // publish the new depth
    } else {
        emitter.emit_symbol_address("r11", "_ob_handler_stubs");              // materialize the handler-stub slot array
        emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the handler stub
        emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the handler stub
        emitter.emit_symbol_address("r11", "_ob_handler_envs");               // materialize the handler-env slot array
        emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the handler env word
        emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the handler env word
        emitter.emit_symbol_address("r11", "_ob_name_ptrs");                  // materialize the handler-name pointer array
        emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the owned name pointer
        emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the owned name pointer
        emitter.emit_symbol_address("r11", "_ob_name_lens");                  // materialize the handler-name length array
        emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the owned name length
        emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the owned name length
        emitter.emit_symbol_address("r11", "_ob_chunk_sizes");                // materialize the chunk-size slot array
        emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the chunk size
        emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the chunk size
        emitter.emit_symbol_address("r11", "_ob_flags");                      // materialize the flags slot array
        emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the flags word
        emitter.instruction("and rcx, -61456");                                     // clear caller-supplied handler type and lifecycle status bits
        emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the flags word
        emitter.emit_symbol_address("r11", "_ob_started");                    // materialize the started-flag slot array
        emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");                      // the handler has not run yet
        emitter.instruction("add r10, 1");                                          // the stack is now one level deeper
        emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the new depth
    }
}

/// Detaches the top slot and frees its byte storage and owned name before handler retirement.
/// The caller selects the release path using the returned stub and environment.
pub fn emit_pop_storage(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => aarch64_pop_storage(emitter, HANDLER_OWNERS, &HANDLER_METADATA),
        Arch::X86_64 => x86_64_pop_storage(emitter, HANDLER_OWNERS, &HANDLER_METADATA),
    }
}

/// Emits the return path for an empty output stack after the handler dispatcher.
pub fn emit_pop_empty_return(emitter: &mut impl Emitter) {
    emitter.label("__rt_ob_pop_empty");
    match emitter.arch() {
        Arch::AArch64 => aarch64_pop_empty_return(emitter),
        Arch::X86_64 => x86_64_pop_empty_return(emitter),
    }
}

/// Truncates the selected buffer while retaining its bytes allocation.
/// The index is staged in `x10` or `r10` by the flush and clean operation.
pub fn emit_truncate_slot(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.emit_symbol_address("x11", "_ob_lens");
            emitter.instruction("str xzr, [x11, x10, lsl #3]");                 // clear the visible used length
        }
        Arch::X86_64 => {
            emitter.emit_symbol_address("r11", "_ob_lens");
            emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");              // clear the visible used length
        }
    }
}

/// Drops the active depth when the request hands ownership of its heap back to the arena.
pub fn emit_reset_depth(emitter: &mut impl Emitter) {
    emitter.emit_store_zero_to_symbol("_ob_level", 0);
}

/// Allocates a buffer and records it in the free slot on aarch64.
fn aarch64_allocate_slot(emitter: &mut impl Emitter) {
    emitter.label("__rt_ob_start_alloc");
    emitter.instruction("str x0, [sp, #48]");                                   // save the chosen capacity
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the capture buffer (raw kind)
    // -- record every per-slot field --
    emitter.emit_symbol_address("x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // reload the depth (the new slot index)
    emitter.emit_symbol_address("x11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("str x0, [x11, x10, lsl #3]");                          // record the new buffer base pointer
    emitter.emit_symbol_address("x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("str xzr, [x11, x10, lsl #3]");                         // the new buffer starts empty
    emitter.emit_symbol_address("x11", "_ob_caps");                       // materialize the capacity slot array
    emitter.instruction("ldr x12, [sp, #48]");                                  // reload the chosen capacity
    emitter.instruction("str x12, [x11, x10, lsl #3]");                         // record the capacity
}

/// Grows, copies and publishes the appended bytes on aarch64.
fn aarch64_append_bytes(emitter: &mut impl Emitter) {
    emitter.emit_symbol_address("x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("cbz x10, __rt_ob_append_done");                        // no active buffer, defensive no-op
    emitter.instruction("sub x10, x10, #1");                                    // top slot index = depth - 1
    emitter.instruction("str x10, [sp, #16]");                                  // save the top slot index
    emitter.emit_symbol_address("x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the top buffer's used byte count
    emitter.instruction("str x12, [sp, #24]");                                  // save the used byte count
    emitter.emit_symbol_address("x13", "_ob_caps");                       // materialize the capacity slot array
    emitter.instruction("ldr x14, [x13, x10, lsl #3]");                         // load the top buffer's capacity
    emitter.instruction("ldr x15, [sp, #8]");                                   // reload the incoming byte length
    emitter.instruction("add x15, x12, x15");                                   // needed bytes = used + incoming length
    emitter.instruction("cmp x15, x14");                                        // does the payload fit the current capacity?
    emitter.instruction("b.ls __rt_ob_append_copy");                            // fits, skip the growth path
    // -- grow: double the capacity until the payload fits --
    emitter.label("__rt_ob_append_grow_size");
    emitter.instruction("lsl x14, x14, #1");                                    // double the candidate capacity
    emitter.instruction("cmp x14, x15");                                        // does the doubled capacity fit the payload?
    emitter.instruction("b.lo __rt_ob_append_grow_size");                       // keep doubling until it fits
    emitter.instruction("str x14, [sp, #32]");                                  // save the new capacity
    emitter.instruction("mov x0, x14");                                         // request the new capacity from the allocator
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the replacement buffer (raw kind)
    // -- copy the used prefix from the old buffer into the replacement --
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the top slot index
    emitter.emit_symbol_address("x11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the old buffer base pointer
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the used byte count
    emitter.instruction("mov x14, #0");                                         // start the copy at offset zero
    emitter.label("__rt_ob_append_grow_copy");
    emitter.instruction("cmp x14, x13");                                        // copied all used bytes?
    emitter.instruction("b.ge __rt_ob_append_grow_swap");                       // yes, publish the replacement buffer
    emitter.instruction("ldrb w15, [x12, x14]");                                // load the next byte from the old buffer
    emitter.instruction("strb w15, [x0, x14]");                                 // store the byte into the replacement buffer
    emitter.instruction("add x14, x14, #1");                                    // advance the copy cursor
    emitter.instruction("b __rt_ob_append_grow_copy");                          // continue copying the used prefix
    emitter.label("__rt_ob_append_grow_swap");
    emitter.instruction("str x0, [x11, x10, lsl #3]");                          // publish the replacement buffer base pointer
    emitter.emit_symbol_address("x13", "_ob_caps");                       // materialize the capacity slot array
    emitter.instruction("ldr x14, [sp, #32]");                                  // reload the new capacity
    emitter.instruction("str x14, [x13, x10, lsl #3]");                         // publish the new capacity
    emitter.instruction("mov x0, x12");                                         // pass the old buffer to the deallocator
    emitter.instruction("bl __rt_heap_free");                                   // release the old buffer block
    // -- copy the incoming bytes to buffer base + used --
    emitter.label("__rt_ob_append_copy");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the top slot index
    emitter.emit_symbol_address("x11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the (possibly replaced) buffer base pointer
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the used byte count
    emitter.instruction("add x12, x12, x13");                                   // destination cursor = base + used
    emitter.instruction("ldr x14, [sp, #0]");                                   // reload the source byte pointer
    emitter.instruction("ldr x15, [sp, #8]");                                   // reload the source byte length
    emitter.label("__rt_ob_append_copy_loop");
    emitter.instruction("cbz x15, __rt_ob_append_publish");                     // no bytes left, publish the new length
    emitter.instruction("ldrb w9, [x14]");                                      // load the next source byte
    emitter.instruction("strb w9, [x12]");                                      // store the byte at the destination cursor
    emitter.instruction("add x14, x14, #1");                                    // advance the source cursor
    emitter.instruction("add x12, x12, #1");                                    // advance the destination cursor
    emitter.instruction("sub x15, x15, #1");                                    // one byte fewer to copy
    emitter.instruction("b __rt_ob_append_copy_loop");                          // continue copying the incoming bytes
    emitter.label("__rt_ob_append_publish");
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the used byte count
    emitter.instruction("ldr x14, [sp, #8]");                                   // reload the incoming byte length
    emitter.instruction("add x13, x13, x14");                                   // new used count = used + incoming length
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the top slot index
    emitter.emit_symbol_address("x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("str x13, [x11, x10, lsl #3]");                         // publish the new used byte count
}

/// Allocates a buffer and records it in the free slot on x86_64.
fn x86_64_allocate_slot(emitter: &mut impl Emitter) {
    emitter.label("__rt_ob_start_alloc_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the chosen capacity
    emitter.instruction("call __rt_heap_alloc");                                // allocate the capture buffer (raw kind)
    // -- record every per-slot field --
    emitter.emit_symbol_address("r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // reload the depth (the new slot index)
    emitter.emit_symbol_address("r11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rax");                    // record the new buffer base pointer
    emitter.emit_symbol_address("r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");                      // the new buffer starts empty
    emitter.emit_symbol_address("r11", "_ob_caps");                       // materialize the capacity slot array
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the chosen capacity
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // record the capacity
}

/// Grows, copies and publishes the appended bytes on x86_64.
fn x86_64_append_bytes(emitter: &mut impl Emitter) {
    emitter.emit_symbol_address("r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("test r10, r10");                                       // is any buffer active?
    emitter.instruction("jz __rt_ob_append_done_x86");                          // no active buffer, defensive no-op
    emitter.instruction("sub r10, 1");                                          // top slot index = depth - 1
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the top slot index
    emitter.emit_symbol_address("r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // load the top buffer's used byte count
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the used byte count
    emitter.emit_symbol_address("r11", "_ob_caps");                       // materialize the capacity slot array
    emitter.instruction("mov rdx, QWORD PTR [r11 + r10*8]");                    // load the top buffer's capacity
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the incoming byte length
    emitter.instruction("add r8, rcx");                                         // needed bytes = used + incoming length
    emitter.instruction("cmp r8, rdx");                                         // does the payload fit the current capacity?
    emitter.instruction("jbe __rt_ob_append_copy_x86");                         // fits, skip the growth path
    // -- grow: double the capacity until the payload fits --
    emitter.label("__rt_ob_append_grow_size_x86");
    emitter.instruction("shl rdx, 1");                                          // double the candidate capacity
    emitter.instruction("cmp rdx, r8");                                         // does the doubled capacity fit the payload?
    emitter.instruction("jb __rt_ob_append_grow_size_x86");                     // keep doubling until it fits
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the new capacity
    emitter.instruction("mov rax, rdx");                                        // request the new capacity from the allocator
    emitter.instruction("call __rt_heap_alloc");                                // allocate the replacement buffer (raw kind)
    // -- copy the used prefix from the old buffer into the replacement --
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the top slot index
    emitter.emit_symbol_address("r11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // load the old buffer base pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the used byte count
    emitter.instruction("xor r8d, r8d");                                        // start the copy at offset zero
    emitter.label("__rt_ob_append_grow_copy_x86");
    emitter.instruction("cmp r8, rdx");                                         // copied all used bytes?
    emitter.instruction("jge __rt_ob_append_grow_swap_x86");                    // yes, publish the replacement buffer
    emitter.instruction("mov r9b, BYTE PTR [rcx + r8]");                        // load the next byte from the old buffer
    emitter.instruction("mov BYTE PTR [rax + r8], r9b");                        // store the byte into the replacement buffer
    emitter.instruction("add r8, 1");                                           // advance the copy cursor
    emitter.instruction("jmp __rt_ob_append_grow_copy_x86");                    // continue copying the used prefix
    emitter.label("__rt_ob_append_grow_swap_x86");
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rax");                    // publish the replacement buffer base pointer
    emitter.emit_symbol_address("r9", "_ob_caps");                        // materialize the capacity slot array
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the new capacity
    emitter.instruction("mov QWORD PTR [r9 + r10*8], rdx");                     // publish the new capacity
    emitter.instruction("mov rax, rcx");                                        // pass the old buffer to the deallocator
    emitter.instruction("call __rt_heap_free");                                 // release the old buffer block
    // -- copy the incoming bytes to buffer base + used --
    emitter.label("__rt_ob_append_copy_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the top slot index
    emitter.emit_symbol_address("r11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // load the (possibly replaced) buffer base pointer
    emitter.instruction("add rcx, QWORD PTR [rbp - 32]");                       // destination cursor = base + used
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // reload the source byte pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the source byte length
    emitter.label("__rt_ob_append_copy_loop_x86");
    emitter.instruction("test r8, r8");                                         // any bytes left to copy?
    emitter.instruction("jz __rt_ob_append_publish_x86");                       // no bytes left, publish the new length
    emitter.instruction("mov r9b, BYTE PTR [rdx]");                             // load the next source byte
    emitter.instruction("mov BYTE PTR [rcx], r9b");                             // store the byte at the destination cursor
    emitter.instruction("add rdx, 1");                                          // advance the source cursor
    emitter.instruction("add rcx, 1");                                          // advance the destination cursor
    emitter.instruction("sub r8, 1");                                           // one byte fewer to copy
    emitter.instruction("jmp __rt_ob_append_copy_loop_x86");                    // continue copying the incoming bytes
    emitter.label("__rt_ob_append_publish_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the used byte count
    emitter.instruction("add rcx, QWORD PTR [rbp - 16]");                       // new used count = used + incoming length
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the top slot index
    emitter.emit_symbol_address("r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rcx");                    // publish the new used byte count
}

/// Emits `__rt_ob_contents`: return a persisted copy of the top buffer contents.
///
/// No inputs. Returns the platform string result pair (AArch64 `x1`=ptr, `x2`=len /
/// x86_64 `rax`=ptr, `rdx`=len); a null pointer signals "no active buffer" so the
/// caller can box PHP `false`.
pub fn emit_ob_contents(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::X86_64 {
        emit_ob_contents_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_contents ---");
    emitter.label_global("__rt_ob_contents");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the persist call
    emitter.emit_symbol_address("x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("cbz x10, __rt_ob_contents_none");                      // no active buffer, return the null failure pair
    emitter.instruction("sub x10, x10, #1");                                    // top slot index = depth - 1
    emitter.emit_symbol_address("x11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("ldr x1, [x11, x10, lsl #3]");                          // load the top buffer base pointer
    emitter.emit_symbol_address("x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("ldr x2, [x11, x10, lsl #3]");                          // load the top buffer's used byte count
    emitter.instruction("bl __rt_str_persist");                                 // copy the contents into an owned heap string
    emitter.instruction("b __rt_ob_contents_done");                             // return the persisted string pair
    emitter.label("__rt_ob_contents_none");
    emitter.instruction("mov x1, #0");                                          // null pointer signals "no active buffer"
    emitter.instruction("mov x2, #0");                                          // zero length for the failure pair
    emitter.label("__rt_ob_contents_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the string result pair
}

/// Emits the Linux x86_64 variant of `__rt_ob_contents`.
fn emit_ob_contents_x86_64(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_contents ---");
    emitter.label_global("__rt_ob_contents");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the persist call
    emitter.instruction("mov rbp, rsp");                                        // establish a frame pointer for the persist call
    emitter.emit_symbol_address("r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("test r10, r10");                                       // is any buffer active?
    emitter.instruction("jz __rt_ob_contents_none_x86");                        // no active buffer, return the null failure pair
    emitter.instruction("sub r10, 1");                                          // top slot index = depth - 1
    emitter.emit_symbol_address("r11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // load the top buffer base pointer
    emitter.emit_symbol_address("r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov rdx, QWORD PTR [r11 + r10*8]");                    // load the top buffer's used byte count
    emitter.instruction("call __rt_str_persist");                               // copy the contents into an owned heap string
    emitter.instruction("jmp __rt_ob_contents_done_x86");                       // return the persisted string pair
    emitter.label("__rt_ob_contents_none_x86");
    emitter.instruction("xor eax, eax");                                        // null pointer signals "no active buffer"
    emitter.instruction("xor edx, edx");                                        // zero length for the failure pair
    emitter.label("__rt_ob_contents_done_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the string result pair
}
/// Detaches and retires buffer-owned allocations on aarch64.
fn aarch64_pop_storage(emitter: &mut impl Emitter, handler_fields: [&str; 3], metadata: &[&str]) {
    emitter.instruction("sub sp, sp, #48");                                     // reserve four detached fields plus native linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller across storage release
    emitter.instruction("add x29, sp, #32");                                    // establish the detached-buffer frame
    emitter.emit_symbol_address("x9", "_ob_level");
    emitter.instruction("ldr x10, [x9]");                                       // inspect the current output depth
    emitter.instruction("cbz x10, __rt_ob_pop_empty");                          // no buffer owns fields to release
    emitter.instruction("sub x10, x10, #1");                                    // identify the slot being removed
    emitter.instruction("str x10, [x9]");                                       // publish the closed buffer before running any cleanup
    for (index, symbol) in handler_fields.into_iter().chain(["_ob_ptrs"]).enumerate() {
        emitter.emit_symbol_address("x11", symbol);
        emitter.instruction("ldr x12, [x11, x10, lsl #3]");                     // copy one retired slot field before external cleanup
        emitter.instruction(&format!("str x12, [sp, #{}]", index * 8));         // keep detached fields independent of any replacement buffer
        emitter.instruction("str xzr, [x11, x10, lsl #3]");                     // remove the old slot's ownership claim
    }
    for symbol in ["_ob_lens", "_ob_caps"].into_iter().chain(metadata.iter().copied()) {
        emitter.emit_symbol_address("x11", symbol);
        emitter.instruction("str xzr, [x11, x10, lsl #3]");                     // clear metadata before the slot may be reused
    }
    emitter.instruction("ldr x0, [sp, #16]");                                   // consume the copied display-name owner
    emitter.instruction("bl __rt_decref_any");                                  // retire display bytes while no PHP destructor can run
    emitter.instruction("ldr x0, [sp, #24]");                                   // consume the detached capture-storage block
    emitter.instruction("bl __rt_heap_free");                                   // free buffer bytes before a callback owner can reenter output APIs
    emitter.instruction("ldr x12, [sp]");                                       // identify the detached callback representation
    emitter.instruction("ldr x0, [sp, #8]");                                    // transfer its environment word to the appropriate release path
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore linkage after every non-callback owner has been consumed
    emitter.instruction("add sp, sp, #48");                                     // leave no buffer-owned state beneath the callback destructor
}

/// Returns from an empty pop on aarch64.
fn aarch64_pop_empty_return(emitter: &mut impl Emitter) {
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore linkage for an empty stack
    emitter.instruction("add sp, sp, #48");                                     // release the unused detached-field frame
    emitter.instruction("ret");                                                 // leave an empty buffer stack unchanged
}

/// Detaches and retires buffer-owned allocations on x86_64.
fn x86_64_pop_storage(emitter: &mut impl Emitter, handler_fields: [&str; 3], metadata: &[&str]) {
    emitter.instruction("push rbp");                                            // align the stack and preserve caller linkage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for detached owners
    emitter.instruction("sub rsp, 32");                                         // reserve the four retired slot fields
    emitter.emit_symbol_address("r9", "_ob_level");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // inspect the current stack depth
    emitter.instruction("test r10, r10");                                       // distinguish an empty stack from a live buffer
    emitter.instruction("jz __rt_ob_pop_empty");                                // skip cleanup when no buffer is active
    emitter.instruction("sub r10, 1");                                          // identify the slot being closed
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the removal before any cleanup callback
    for (index, symbol) in handler_fields.into_iter().chain(["_ob_ptrs"]).enumerate() {
        emitter.emit_symbol_address("r11", symbol);
        emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                // snapshot one field before the slot may be reused
        emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", index * 8)); // retain detached ownership across allocator calls
        emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");                  // remove this field from the retired stack slot
    }
    for symbol in ["_ob_lens", "_ob_caps"].into_iter().chain(metadata.iter().copied()) {
        emitter.emit_symbol_address("r11", symbol);
        emitter.instruction("mov QWORD PTR [r11 + r10*8], 0");                  // erase metadata before a destructor can replace this slot
    }
    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                       // transfer the persisted display-name owner
    emitter.instruction("call __rt_decref_any");                                // release auxiliary name bytes before callback destruction
    emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                       // transfer the detached buffer allocation
    emitter.instruction("call __rt_heap_free");                                 // retire capture storage before any PHP reentry
    emitter.instruction("mov r11, QWORD PTR [rsp]");                            // recover the callback representation
    emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                        // transfer the retained descriptor or eval registration id
    emitter.instruction("leave");                                               // remove the detached-owner frame before the final release
}

/// Returns from an empty pop on x86_64.
fn x86_64_pop_empty_return(emitter: &mut impl Emitter) {
    emitter.instruction("leave");                                               // restore the caller when the output stack was empty
    emitter.instruction("ret");                                                 // complete the no-op pop
}

/// Emits the borrowed handler row queries used by the eval C interface.
pub fn emit_handler_queries(emitter: &mut impl Emitter) {
    if emitter.arch() == Arch::AArch64 {
        emitter.label_global(&emitter.extern_symbol("__elephc_eval_ob_slot_meta"));
        emitter.emit_symbol_address("x9", "_ob_level");       // materialize the address of the buffer-stack depth
        emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
        emitter.instruction("cmp x0, x10");                                         // is the requested slot index within the stack?
        emitter.instruction("b.hs __elephc_eval_ob_slot_meta_none");                // out-of-range index — report failure
        emitter.emit_symbol_address("x11", "_ob_chunk_sizes"); // materialize the chunk-size slot array
        emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's chunk size
        emitter.instruction("str x12, [x1]");                                       // store it through the caller's out_chunk
        emitter.emit_symbol_address("x11", "_ob_flags");      // materialize the flags slot array
        emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's stored flags word
        emitter.instruction("str x12, [x2]");                                       // store it through the caller's out_flags
        emitter.emit_symbol_address("x11", "_ob_handler_stubs"); // materialize the handler-stub slot array
        emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's handler stub
        emitter.instruction("cmp x12, #0");                                         // is a user handler installed?
        emitter.instruction("cset x12, ne");                                        // bit 0 = user-handler flag
        emitter.emit_symbol_address("x11", "_ob_started");    // materialize the started-flag slot array
        emitter.instruction("ldr x13, [x11, x0, lsl #3]");                          // load the slot's started flag
        emitter.instruction("cmp x13, #0");                                         // has the handler run at least once?
        emitter.instruction("cset x13, ne");                                        // normalize the started flag to 0/1
        emitter.instruction("orr x12, x12, x13, lsl #1");                           // bit 1 = started flag
        emitter.instruction("str x12, [x3]");                                       // store the packed user/started bits
        emitter.instruction("mov x0, #1");                                          // report success to Rust
        emitter.instruction("ret");                                                 // return to Rust
        emitter.label("__elephc_eval_ob_slot_meta_none");
        emitter.instruction("mov x0, #0");                                          // report "no such buffer" to Rust
        emitter.instruction("ret");                                                 // return to Rust

        emitter.label_global(&emitter.extern_symbol("__elephc_eval_ob_slot_name"));
        emitter.emit_symbol_address("x9", "_ob_level");       // materialize the address of the buffer-stack depth
        emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
        emitter.instruction("cmp x0, x10");                                         // is the requested slot index within the stack?
        emitter.instruction("b.hs __elephc_eval_ob_slot_name_none");                // out-of-range index — report failure
        emitter.emit_symbol_address("x11", "_ob_name_ptrs");  // materialize the handler-name pointer array
        emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's display-name pointer
        emitter.instruction("str x12, [x1]");                                       // store it through the caller's out_ptr
        emitter.emit_symbol_address("x11", "_ob_name_lens");  // materialize the handler-name length array
        emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's display-name length
        emitter.instruction("str x12, [x2]");                                       // store it through the caller's out_len
        emitter.instruction("mov x0, #1");                                          // report success to Rust (bytes copied immediately)
        emitter.instruction("ret");                                                 // return to Rust
        emitter.label("__elephc_eval_ob_slot_name_none");
        emitter.instruction("mov x0, #0");                                          // report "no such buffer" to Rust
        emitter.instruction("ret");                                                 // return to Rust
    } else {
        emitter.label_global(&emitter.extern_symbol("__elephc_eval_ob_slot_meta"));
        emitter.emit_symbol_address("r9", "_ob_level");       // materialize the address of the buffer-stack depth
        emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
        emitter.instruction("cmp rdi, r10");                                        // is the requested slot index within the stack?
        emitter.instruction("jae __elephc_eval_ob_slot_meta_none_x86");             // out-of-range index — report failure
        emitter.emit_symbol_address("r11", "_ob_chunk_sizes"); // materialize the chunk-size slot array
        emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's chunk size
        emitter.instruction("mov QWORD PTR [rsi], rax");                            // store it through the caller's out_chunk
        emitter.emit_symbol_address("r11", "_ob_flags");      // materialize the flags slot array
        emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's stored flags word
        emitter.instruction("mov QWORD PTR [rdx], rax");                            // store it through the caller's out_flags
        emitter.emit_symbol_address("r11", "_ob_handler_stubs"); // materialize the handler-stub slot array
        emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's handler stub
        emitter.instruction("test rax, rax");                                       // is a user handler installed?
        emitter.instruction("setnz al");                                            // bit 0 = user-handler flag
        emitter.instruction("movzx rax, al");                                       // zero-extend the packed bits
        emitter.emit_symbol_address("r11", "_ob_started");    // materialize the started-flag slot array
        emitter.instruction("mov r10, QWORD PTR [r11 + rdi*8]");                    // load the slot's started flag
        emitter.instruction("test r10, r10");                                       // has the handler run at least once?
        emitter.instruction("jz __elephc_eval_ob_slot_meta_store_x86");             // an unstarted handler keeps bit 1 clear
        emitter.instruction("or rax, 2");                                           // bit 1 = started flag
        emitter.label("__elephc_eval_ob_slot_meta_store_x86");
        emitter.instruction("mov QWORD PTR [rcx], rax");                            // store the packed user/started bits
        emitter.instruction("mov eax, 1");                                          // report success to Rust
        emitter.instruction("ret");                                                 // return to Rust
        emitter.label("__elephc_eval_ob_slot_meta_none_x86");
        emitter.instruction("xor eax, eax");                                        // report "no such buffer" to Rust
        emitter.instruction("ret");                                                 // return to Rust

        emitter.label_global(&emitter.extern_symbol("__elephc_eval_ob_slot_name"));
        emitter.emit_symbol_address("r9", "_ob_level");       // materialize the address of the buffer-stack depth
        emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
        emitter.instruction("cmp rdi, r10");                                        // is the requested slot index within the stack?
        emitter.instruction("jae __elephc_eval_ob_slot_name_none_x86");             // out-of-range index — report failure
        emitter.emit_symbol_address("r11", "_ob_name_ptrs");  // materialize the handler-name pointer array
        emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's display-name pointer
        emitter.instruction("mov QWORD PTR [rsi], rax");                            // store it through the caller's out_ptr
        emitter.emit_symbol_address("r11", "_ob_name_lens");  // materialize the handler-name length array
        emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's display-name length
        emitter.instruction("mov QWORD PTR [rdx], rax");                            // store it through the caller's out_len
        emitter.instruction("mov eax, 1");                                          // report success to Rust (bytes copied immediately)
        emitter.instruction("ret");                                                 // return to Rust
        emitter.label("__elephc_eval_ob_slot_name_none_x86");
        emitter.instruction("xor eax, eax");                                        // report "no such buffer" to Rust
        emitter.instruction("ret");                                                 // return to Rust
    }
}
