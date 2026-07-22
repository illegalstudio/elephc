//! Purpose:
//! Emits the output-buffering introspection runtime helpers: `__rt_ob_get_status`
//! (the `ob_get_status()` status hash, simple and full modes) and
//! `__rt_ob_list_handlers` (the `ob_list_handlers()` handler-name array).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Status entries mirror PHP's default-handler shape: name = "default output
//!   handler", type = 0, flags = 112 (PHP_OUTPUT_HANDLER_STDFLAGS), level =
//!   0-based buffer index, chunk_size = 0, buffer_size = capacity, buffer_used =
//!   used bytes. elephc does not support user handlers, so every entry reports
//!   the default handler.
//! - Hashes are built with `__rt_hash_new(cap, value_type 7 = mixed)` +
//!   `__rt_hash_set(hash, key_ptr, key_len, value_lo, value_hi, tag)`; the hash
//!   pointer may move on insert, so it is reloaded/saved around every set.
//!   Nested status entries use mixed tag 5 (assoc hash payload).
//! - Key strings (`_ob_k_*`) and the handler name (`_ob_handler_name`) live in
//!   the fixed runtime data section.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// The `ob_get_status()` entry keys with their symbol names and byte lengths.
#[cfg(test)]
const OB_STATUS_KEYS: [(&str, i64); 7] = [
    ("_ob_k_name", 4),
    ("_ob_k_type", 4),
    ("_ob_k_flags", 5),
    ("_ob_k_level", 5),
    ("_ob_k_chunk_size", 10),
    ("_ob_k_buffer_size", 11),
    ("_ob_k_buffer_used", 11),
];

/// Emits `__rt_ob_status_entry`: build the status hash for one buffer slot.
///
/// Input: slot index in `x0`/`rax`. Output: hash pointer in `x0`/`rax`.
/// Internal helper shared by the simple and full modes of `__rt_ob_get_status`.
pub fn emit_ob_status_entry(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_status_entry_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_status_entry ---");
    emitter.label_global("__rt_ob_status_entry");
    // frame: [0]=slot index, [8]=hash pointer
    emitter.instruction("sub sp, sp, #32");                                     // allocate the status-entry frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the status-entry frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the buffer slot index
    emitter.instruction("mov x0, #16");                                         // initial capacity 16 (>= 7 entries, avoids a mid-build realloc)
    emitter.instruction("mov x1, #7");                                          // value type = mixed (int and string values)
    emitter.instruction("bl __rt_hash_new");                                    // allocate the status hash
    emitter.instruction("str x0, [sp, #8]");                                    // save the hash pointer
    // -- "name" → persisted copy of the slot's handler display name --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the buffer slot index
    abi::emit_symbol_address(emitter, "x10", "_ob_name_ptrs");                  // materialize the handler-name pointer array
    emitter.instruction("ldr x1, [x10, x9, lsl #3]");                           // load the slot's display-name pointer
    abi::emit_symbol_address(emitter, "x10", "_ob_name_lens");                  // materialize the handler-name length array
    emitter.instruction("ldr x2, [x10, x9, lsl #3]");                           // load the slot's display-name length
    emitter.instruction("bl __rt_str_persist");                                 // copy the name to the heap → x1=ptr, x2=len
    emitter.instruction("mov x3, x1");                                          // value_lo = heap string pointer
    emitter.instruction("mov x4, x2");                                          // value_hi = string length
    emitter.instruction("mov x5, #1");                                          // value tag = string
    abi::emit_symbol_address(emitter, "x1", "_ob_k_name");                      // key = "name"
    emitter.instruction("mov x2, #4");                                          // length of "name"
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the hash pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert "name" → handler-name string
    emitter.instruction("str x0, [sp, #8]");                                    // save the (possibly reallocated) hash pointer
    // -- "type" → 1 for user handlers, 0 for the default handler --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the buffer slot index
    abi::emit_symbol_address(emitter, "x10", "_ob_handler_stubs");              // materialize the handler-stub slot array
    emitter.instruction("ldr x3, [x10, x9, lsl #3]");                           // load the slot's handler stub
    emitter.instruction("cmp x3, #0");                                          // is a user handler installed?
    emitter.instruction("cset x3, ne");                                         // value_lo = 1 (user) or 0 (internal)
    emitter.instruction("mov x4, #0");                                          // value_hi = 0
    emitter.instruction("mov x5, #0");                                          // value tag = int
    abi::emit_symbol_address(emitter, "x1", "_ob_k_type");                      // key = "type"
    emitter.instruction("mov x2, #4");                                          // length of "type"
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the hash pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert "type" → 0
    emitter.instruction("str x0, [sp, #8]");                                    // save the (possibly reallocated) hash pointer
    // -- "flags" → stored flags | user bit | PHP started/processed bits --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the buffer slot index
    abi::emit_symbol_address(emitter, "x10", "_ob_flags");                      // materialize the flags slot array
    emitter.instruction("ldr x3, [x10, x9, lsl #3]");                           // load the slot's stored flags word
    abi::emit_symbol_address(emitter, "x10", "_ob_handler_stubs");              // materialize the handler-stub slot array
    emitter.instruction("ldr x11, [x10, x9, lsl #3]");                          // load the slot's handler stub
    emitter.instruction("cmp x11, #0");                                         // is a user handler installed?
    emitter.instruction("cset x11, ne");                                        // user handlers add PHP's user-handler bit
    emitter.instruction("orr x3, x3, x11");                                     // fold the user-handler bit into the flags
    abi::emit_symbol_address(emitter, "x10", "_ob_started");                    // materialize the started-flag slot array
    emitter.instruction("ldr x11, [x10, x9, lsl #3]");                          // load the slot's started flag
    emitter.instruction("cbz x11, __rt_ob_status_flags_ready");                 // an unstarted handler keeps the base flags
    emitter.instruction("mov x11, #0x5000");                                    // PHP's STARTED (0x1000) | PROCESSED (0x4000) bits
    emitter.instruction("orr x3, x3, x11");                                     // fold the started/processed bits into the flags
    emitter.label("__rt_ob_status_flags_ready");
    emitter.instruction("mov x4, #0");                                          // value_hi = 0
    emitter.instruction("mov x5, #0");                                          // value tag = int
    abi::emit_symbol_address(emitter, "x1", "_ob_k_flags");                     // key = "flags"
    emitter.instruction("mov x2, #5");                                          // length of "flags"
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the hash pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert "flags" → 112
    emitter.instruction("str x0, [sp, #8]");                                    // save the (possibly reallocated) hash pointer
    // -- "level" → the 0-based buffer index --
    emitter.instruction("ldr x3, [sp, #0]");                                    // value_lo = the buffer slot index
    emitter.instruction("mov x4, #0");                                          // value_hi = 0
    emitter.instruction("mov x5, #0");                                          // value tag = int
    abi::emit_symbol_address(emitter, "x1", "_ob_k_level");                     // key = "level"
    emitter.instruction("mov x2, #5");                                          // length of "level"
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the hash pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert "level" → index
    emitter.instruction("str x0, [sp, #8]");                                    // save the (possibly reallocated) hash pointer
    // -- "chunk_size" → the slot's stored chunk size --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the buffer slot index
    abi::emit_symbol_address(emitter, "x10", "_ob_chunk_sizes");                // materialize the chunk-size slot array
    emitter.instruction("ldr x3, [x10, x9, lsl #3]");                           // value_lo = the stored chunk size
    emitter.instruction("mov x4, #0");                                          // value_hi = 0
    emitter.instruction("mov x5, #0");                                          // value tag = int
    abi::emit_symbol_address(emitter, "x1", "_ob_k_chunk_size");                // key = "chunk_size"
    emitter.instruction("mov x2, #10");                                         // length of "chunk_size"
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the hash pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert "chunk_size" → 0
    emitter.instruction("str x0, [sp, #8]");                                    // save the (possibly reallocated) hash pointer
    // -- "buffer_size" → the slot capacity --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the buffer slot index
    abi::emit_symbol_address(emitter, "x10", "_ob_caps");                       // materialize the capacity slot array
    emitter.instruction("ldr x3, [x10, x9, lsl #3]");                           // value_lo = the buffer capacity
    emitter.instruction("mov x4, #0");                                          // value_hi = 0
    emitter.instruction("mov x5, #0");                                          // value tag = int
    abi::emit_symbol_address(emitter, "x1", "_ob_k_buffer_size");               // key = "buffer_size"
    emitter.instruction("mov x2, #11");                                         // length of "buffer_size"
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the hash pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert "buffer_size" → capacity
    emitter.instruction("str x0, [sp, #8]");                                    // save the (possibly reallocated) hash pointer
    // -- "buffer_used" → the slot used byte count --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the buffer slot index
    abi::emit_symbol_address(emitter, "x10", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("ldr x3, [x10, x9, lsl #3]");                           // value_lo = the buffer used byte count
    emitter.instruction("mov x4, #0");                                          // value_hi = 0
    emitter.instruction("mov x5, #0");                                          // value tag = int
    abi::emit_symbol_address(emitter, "x1", "_ob_k_buffer_used");               // key = "buffer_used"
    emitter.instruction("mov x2, #11");                                         // length of "buffer_used"
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the hash pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert "buffer_used" → used bytes
    emitter.instruction("str x0, [sp, #8]");                                    // save the (possibly reallocated) hash pointer
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the completed status hash
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the status-entry frame
    emitter.instruction("ret");                                                 // return the status hash pointer
}

/// Emits the Linux x86_64 variant of `__rt_ob_status_entry`.
fn emit_ob_status_entry_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_status_entry ---");
    emitter.label_global("__rt_ob_status_entry");
    // frame: [rbp-8]=slot index, [rbp-16]=hash pointer
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the status-entry frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve the status-entry local slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the buffer slot index
    emitter.instruction("mov rdi, 16");                                         // initial capacity 16 (>= 7 entries, avoids a mid-build realloc)
    emitter.instruction("mov rsi, 7");                                          // value type = mixed (int and string values)
    emitter.instruction("call __rt_hash_new");                                  // allocate the status hash
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the hash pointer
    // -- "name" → persisted copy of the slot's handler display name --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the buffer slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_name_ptrs");                  // materialize the handler-name pointer array
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // load the slot's display-name pointer
    abi::emit_symbol_address(emitter, "r11", "_ob_name_lens");                  // materialize the handler-name length array
    emitter.instruction("mov rdx, QWORD PTR [r11 + r10*8]");                    // load the slot's display-name length
    emitter.instruction("call __rt_str_persist");                               // copy the name to the heap → rax=ptr, rdx=len
    emitter.instruction("mov rcx, rax");                                        // value_lo = heap string pointer
    emitter.instruction("mov r8, rdx");                                         // value_hi = string length
    emitter.instruction("mov r9, 1");                                           // value tag = string
    abi::emit_symbol_address(emitter, "rsi", "_ob_k_name");                     // key = "name"
    emitter.instruction("mov rdx, 4");                                          // length of "name"
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the hash pointer
    emitter.instruction("call __rt_hash_set");                                  // insert "name" → handler-name string
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the (possibly reallocated) hash pointer
    // -- "type" → 1 for user handlers, 0 for the default handler --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the buffer slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_handler_stubs");              // materialize the handler-stub slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // load the slot's handler stub
    emitter.instruction("test rcx, rcx");                                       // is a user handler installed?
    emitter.instruction("setnz cl");                                            // low byte = 1 (user) or 0 (internal)
    emitter.instruction("movzx rcx, cl");                                       // value_lo = the zero-extended handler type
    emitter.instruction("mov r8, 0");                                           // value_hi = 0
    emitter.instruction("mov r9, 0");                                           // value tag = int
    abi::emit_symbol_address(emitter, "rsi", "_ob_k_type");                     // key = "type"
    emitter.instruction("mov rdx, 4");                                          // length of "type"
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the hash pointer
    emitter.instruction("call __rt_hash_set");                                  // insert "type" → 0
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the (possibly reallocated) hash pointer
    // -- "flags" → stored flags | user bit | PHP started/processed bits --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the buffer slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_flags");                      // materialize the flags slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // load the slot's stored flags word
    abi::emit_symbol_address(emitter, "r11", "_ob_handler_stubs");              // materialize the handler-stub slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // load the slot's handler stub
    emitter.instruction("test rax, rax");                                       // is a user handler installed?
    emitter.instruction("setnz al");                                            // user handlers add PHP's user-handler bit
    emitter.instruction("movzx rax, al");                                       // zero-extend the user-handler bit
    emitter.instruction("or rcx, rax");                                         // fold the user-handler bit into the flags
    abi::emit_symbol_address(emitter, "r11", "_ob_started");                    // materialize the started-flag slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // load the slot's started flag
    emitter.instruction("test rax, rax");                                       // has the handler run at least once?
    emitter.instruction("jz __rt_ob_status_flags_ready_x86");                   // an unstarted handler keeps the base flags
    emitter.instruction("or rcx, 0x5000");                                      // fold PHP's STARTED|PROCESSED bits into the flags
    emitter.label("__rt_ob_status_flags_ready_x86");
    emitter.instruction("mov r8, 0");                                           // value_hi = 0
    emitter.instruction("mov r9, 0");                                           // value tag = int
    abi::emit_symbol_address(emitter, "rsi", "_ob_k_flags");                    // key = "flags"
    emitter.instruction("mov rdx, 5");                                          // length of "flags"
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the hash pointer
    emitter.instruction("call __rt_hash_set");                                  // insert "flags" → 112
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the (possibly reallocated) hash pointer
    // -- "level" → the 0-based buffer index --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // value_lo = the buffer slot index
    emitter.instruction("mov r8, 0");                                           // value_hi = 0
    emitter.instruction("mov r9, 0");                                           // value tag = int
    abi::emit_symbol_address(emitter, "rsi", "_ob_k_level");                    // key = "level"
    emitter.instruction("mov rdx, 5");                                          // length of "level"
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the hash pointer
    emitter.instruction("call __rt_hash_set");                                  // insert "level" → index
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the (possibly reallocated) hash pointer
    // -- "chunk_size" → the slot's stored chunk size --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the buffer slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_chunk_sizes");                // materialize the chunk-size slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // value_lo = the stored chunk size
    emitter.instruction("mov r8, 0");                                           // value_hi = 0
    emitter.instruction("mov r9, 0");                                           // value tag = int
    abi::emit_symbol_address(emitter, "rsi", "_ob_k_chunk_size");               // key = "chunk_size"
    emitter.instruction("mov rdx, 10");                                         // length of "chunk_size"
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the hash pointer
    emitter.instruction("call __rt_hash_set");                                  // insert "chunk_size" → 0
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the (possibly reallocated) hash pointer
    // -- "buffer_size" → the slot capacity --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the buffer slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_caps");                       // materialize the capacity slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // value_lo = the buffer capacity
    emitter.instruction("mov r8, 0");                                           // value_hi = 0
    emitter.instruction("mov r9, 0");                                           // value tag = int
    abi::emit_symbol_address(emitter, "rsi", "_ob_k_buffer_size");              // key = "buffer_size"
    emitter.instruction("mov rdx, 11");                                         // length of "buffer_size"
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the hash pointer
    emitter.instruction("call __rt_hash_set");                                  // insert "buffer_size" → capacity
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the (possibly reallocated) hash pointer
    // -- "buffer_used" → the slot used byte count --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the buffer slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov rcx, QWORD PTR [r11 + r10*8]");                    // value_lo = the buffer used byte count
    emitter.instruction("mov r8, 0");                                           // value_hi = 0
    emitter.instruction("mov r9, 0");                                           // value tag = int
    abi::emit_symbol_address(emitter, "rsi", "_ob_k_buffer_used");              // key = "buffer_used"
    emitter.instruction("mov rdx, 11");                                         // length of "buffer_used"
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the hash pointer
    emitter.instruction("call __rt_hash_set");                                  // insert "buffer_used" → used bytes
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the (possibly reallocated) hash pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the completed status hash
    emitter.instruction("add rsp, 16");                                         // release the status-entry local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the status hash pointer
}

/// Emits `__rt_ob_get_status`: build the `ob_get_status()` result hash.
///
/// Input: full-status flag in `x0`/`rax`. Output: hash pointer in `x0`/`rax`.
/// Simple mode returns the top buffer's status entry (or an empty hash when no
/// buffer is active); full mode returns an int-keyed hash of one entry per level.
pub fn emit_ob_get_status(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_get_status_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_get_status ---");
    emitter.label_global("__rt_ob_get_status");
    // frame: [0]=outer hash, [8]=loop cursor, [16]=buffer count
    emitter.instruction("sub sp, sp, #48");                                     // allocate the get_status frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the get_status frame pointer
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("str x10, [sp, #16]");                                  // save the buffer count
    emitter.instruction("cbnz x0, __rt_ob_get_status_full");                    // non-zero flag selects full-status mode
    // -- simple mode: status of the top buffer, or an empty hash --
    emitter.instruction("cbz x10, __rt_ob_get_status_empty");                   // no active buffer — return an empty hash
    emitter.instruction("sub x0, x10, #1");                                     // top slot index = depth - 1
    emitter.instruction("bl __rt_ob_status_entry");                             // build the top buffer's status hash
    emitter.instruction("b __rt_ob_get_status_done");                           // return the status hash
    emitter.label("__rt_ob_get_status_empty");
    emitter.instruction("mov x0, #8");                                          // minimal capacity for the empty hash
    emitter.instruction("mov x1, #7");                                          // value type = mixed
    emitter.instruction("bl __rt_hash_new");                                    // allocate the empty hash
    emitter.instruction("b __rt_ob_get_status_done");                           // return the empty hash
    // -- full mode: int-keyed hash of one status entry per level --
    emitter.label("__rt_ob_get_status_full");
    emitter.instruction("mov x0, #8");                                          // initial capacity for the outer hash
    emitter.instruction("mov x1, #7");                                          // value type = mixed
    emitter.instruction("bl __rt_hash_new");                                    // allocate the outer hash
    emitter.instruction("str x0, [sp, #0]");                                    // save the outer hash pointer
    emitter.instruction("str xzr, [sp, #8]");                                   // start the level cursor at the bottom slot
    emitter.label("__rt_ob_get_status_full_loop");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the level cursor
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the buffer count
    emitter.instruction("cmp x9, x10");                                         // visited every level?
    emitter.instruction("b.ge __rt_ob_get_status_full_done");                   // yes — return the outer hash
    emitter.instruction("mov x0, x9");                                          // pass the slot index to the entry builder
    emitter.instruction("bl __rt_ob_status_entry");                             // build this level's status hash
    emitter.instruction("mov x3, x0");                                          // value_lo = the nested status hash pointer
    emitter.instruction("mov x4, #0");                                          // value_hi = 0
    emitter.instruction("mov x5, #5");                                          // value tag = assoc hash payload
    emitter.instruction("ldr x1, [sp, #8]");                                    // key_lo = the level cursor
    emitter.instruction("mov x2, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the outer hash pointer
    emitter.instruction("bl __rt_hash_set");                                    // insert level → status entry
    emitter.instruction("str x0, [sp, #0]");                                    // save the (possibly reallocated) outer hash pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the level cursor
    emitter.instruction("add x9, x9, #1");                                      // advance to the next level
    emitter.instruction("str x9, [sp, #8]");                                    // save the advanced level cursor
    emitter.instruction("b __rt_ob_get_status_full_loop");                      // continue building entries
    emitter.label("__rt_ob_get_status_full_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the outer hash pointer
    emitter.label("__rt_ob_get_status_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the get_status frame
    emitter.instruction("ret");                                                 // return the status hash pointer
}

/// Emits the Linux x86_64 variant of `__rt_ob_get_status`.
fn emit_ob_get_status_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_get_status ---");
    emitter.label_global("__rt_ob_get_status");
    // frame: [rbp-8]=outer hash, [rbp-16]=loop cursor, [rbp-24]=buffer count
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the get_status frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve the get_status local slots (16-aligned)
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the buffer count
    emitter.instruction("test rax, rax");                                       // was the full-status flag passed?
    emitter.instruction("jnz __rt_ob_get_status_full_x86");                     // non-zero flag selects full-status mode
    // -- simple mode: status of the top buffer, or an empty hash --
    emitter.instruction("test r10, r10");                                       // is any buffer active?
    emitter.instruction("jz __rt_ob_get_status_empty_x86");                     // no active buffer — return an empty hash
    emitter.instruction("mov rax, r10");                                        // copy the depth for slot indexing
    emitter.instruction("sub rax, 1");                                          // top slot index = depth - 1
    emitter.instruction("call __rt_ob_status_entry");                           // build the top buffer's status hash
    emitter.instruction("jmp __rt_ob_get_status_done_x86");                     // return the status hash
    emitter.label("__rt_ob_get_status_empty_x86");
    emitter.instruction("mov rdi, 8");                                          // minimal capacity for the empty hash
    emitter.instruction("mov rsi, 7");                                          // value type = mixed
    emitter.instruction("call __rt_hash_new");                                  // allocate the empty hash
    emitter.instruction("jmp __rt_ob_get_status_done_x86");                     // return the empty hash
    // -- full mode: int-keyed hash of one status entry per level --
    emitter.label("__rt_ob_get_status_full_x86");
    emitter.instruction("mov rdi, 8");                                          // initial capacity for the outer hash
    emitter.instruction("mov rsi, 7");                                          // value type = mixed
    emitter.instruction("call __rt_hash_new");                                  // allocate the outer hash
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the outer hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // start the level cursor at the bottom slot
    emitter.label("__rt_ob_get_status_full_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the level cursor
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // visited every level?
    emitter.instruction("jge __rt_ob_get_status_full_done_x86");                // yes — return the outer hash
    emitter.instruction("mov rax, r10");                                        // pass the slot index to the entry builder
    emitter.instruction("call __rt_ob_status_entry");                           // build this level's status hash
    emitter.instruction("mov rcx, rax");                                        // value_lo = the nested status hash pointer
    emitter.instruction("mov r8, 0");                                           // value_hi = 0
    emitter.instruction("mov r9, 5");                                           // value tag = assoc hash payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // key_lo = the level cursor
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the outer hash pointer
    emitter.instruction("call __rt_hash_set");                                  // insert level → status entry
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the (possibly reallocated) outer hash pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the level cursor
    emitter.instruction("add r10, 1");                                          // advance to the next level
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the advanced level cursor
    emitter.instruction("jmp __rt_ob_get_status_full_loop_x86");                // continue building entries
    emitter.label("__rt_ob_get_status_full_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the outer hash pointer
    emitter.label("__rt_ob_get_status_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the get_status local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the status hash pointer
}

/// Emits `__rt_ob_list_handlers`: build the `ob_list_handlers()` string array.
///
/// No inputs. Output: array pointer in `x0`/`rax`, one "default output handler"
/// element per active buffer level (empty array when no buffer is active).
pub fn emit_ob_list_handlers(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_list_handlers_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_list_handlers ---");
    emitter.label_global("__rt_ob_list_handlers");
    // frame: [0]=array pointer, [8]=loop cursor, [16]=buffer count
    emitter.instruction("sub sp, sp, #48");                                     // allocate the list_handlers frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the list_handlers frame pointer
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("str x10, [sp, #16]");                                  // save the buffer count
    emitter.instruction("mov x0, x10");                                         // request one element slot per level
    emitter.instruction("cmp x0, #1");                                          // guard against a zero-capacity allocation
    emitter.instruction("csinc x0, x0, xzr, ge");                               // clamp the requested capacity to at least 1
    emitter.instruction("mov x1, #16");                                         // elem_size = 16 (string ptr + len slots)
    emitter.instruction("bl __rt_array_new");                                   // allocate the result array
    emitter.instruction("str x0, [sp, #0]");                                    // save the array pointer
    emitter.instruction("str xzr, [sp, #8]");                                   // start the level cursor at the bottom slot
    emitter.label("__rt_ob_list_handlers_loop");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the level cursor
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the buffer count
    emitter.instruction("cmp x9, x10");                                         // pushed one name per level?
    emitter.instruction("b.ge __rt_ob_list_handlers_done");                     // yes — return the array
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the level cursor (the slot index)
    abi::emit_symbol_address(emitter, "x10", "_ob_name_ptrs");                  // materialize the handler-name pointer array
    emitter.instruction("ldr x1, [x10, x9, lsl #3]");                           // element = the slot's display-name pointer
    abi::emit_symbol_address(emitter, "x10", "_ob_name_lens");                  // materialize the handler-name length array
    emitter.instruction("ldr x2, [x10, x9, lsl #3]");                           // element length = the slot's display-name length
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the array pointer
    emitter.instruction("bl __rt_array_push_str");                              // append the handler name (push_str persists the bytes)
    emitter.instruction("str x0, [sp, #0]");                                    // save the possibly-grown array pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the level cursor
    emitter.instruction("add x9, x9, #1");                                      // advance to the next level
    emitter.instruction("str x9, [sp, #8]");                                    // save the advanced level cursor
    emitter.instruction("b __rt_ob_list_handlers_loop");                        // continue pushing names
    emitter.label("__rt_ob_list_handlers_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the completed array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the list_handlers frame
    emitter.instruction("ret");                                                 // return the array handle
}

/// Emits the Linux x86_64 variant of `__rt_ob_list_handlers`.
fn emit_ob_list_handlers_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_list_handlers ---");
    emitter.label_global("__rt_ob_list_handlers");
    // frame: [rbp-8]=array pointer, [rbp-16]=loop cursor, [rbp-24]=buffer count
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the list_handlers frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve the list_handlers local slots (16-aligned)
    abi::emit_symbol_address(emitter, "r9", "_ob_level");                       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the buffer count
    emitter.instruction("mov rdi, r10");                                        // request one element slot per level
    emitter.instruction("test rdi, rdi");                                       // guard against a zero-capacity allocation
    emitter.instruction("jnz __rt_ob_list_handlers_cap_ok_x86");                // non-zero level count keeps its own capacity
    emitter.instruction("mov edi, 1");                                          // clamp the requested capacity to at least 1
    emitter.label("__rt_ob_list_handlers_cap_ok_x86");
    emitter.instruction("mov esi, 16");                                         // elem_size = 16 (string ptr + len slots)
    emitter.instruction("call __rt_array_new");                                 // allocate the result array
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // start the level cursor at the bottom slot
    emitter.label("__rt_ob_list_handlers_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the level cursor
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // pushed one name per level?
    emitter.instruction("jge __rt_ob_list_handlers_done_x86");                  // yes — return the array
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the level cursor (the slot index)
    abi::emit_symbol_address(emitter, "r11", "_ob_name_ptrs");                  // materialize the handler-name pointer array
    emitter.instruction("mov rsi, QWORD PTR [r11 + r10*8]");                    // element = the slot's display-name pointer
    abi::emit_symbol_address(emitter, "r11", "_ob_name_lens");                  // materialize the handler-name length array
    emitter.instruction("mov rdx, QWORD PTR [r11 + r10*8]");                    // element length = the slot's display-name length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the array pointer
    emitter.instruction("call __rt_array_push_str");                            // append the handler name (push_str persists the bytes)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the possibly-grown array pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the level cursor
    emitter.instruction("add r10, 1");                                          // advance to the next level
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the advanced level cursor
    emitter.instruction("jmp __rt_ob_list_handlers_loop_x86");                  // continue pushing names
    emitter.label("__rt_ob_list_handlers_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the completed array pointer
    emitter.instruction("add rsp, 32");                                         // release the list_handlers local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the array handle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Renders the ob_* status helpers for one target.
    fn render(platform: Platform, arch: Arch) -> String {
        let mut emitter = Emitter::new(Target::new(platform, arch));
        emit_ob_status_entry(&mut emitter);
        emit_ob_get_status(&mut emitter);
        emit_ob_list_handlers(&mut emitter);
        emitter.output()
    }

    /// Verifies every target exports the status helper labels.
    #[test]
    fn emits_global_labels_for_all_targets() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let asm = render(platform, arch);
            for label in [
                "__rt_ob_status_entry",
                "__rt_ob_get_status",
                "__rt_ob_list_handlers",
            ] {
                assert!(
                    asm.contains(&format!(".globl {label}\n")),
                    "missing {label} for {:?}/{:?}",
                    platform,
                    arch
                );
            }
        }
    }

    /// Verifies the status entry references every PHP status key symbol.
    #[test]
    fn status_entry_uses_all_status_keys() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let asm = render(platform, arch);
            for (sym, _) in OB_STATUS_KEYS {
                assert!(asm.contains(sym), "missing {sym} for {:?}/{:?}", platform, arch);
            }
        }
    }

    /// Verifies list_handlers builds the result through the string-array helpers.
    #[test]
    fn list_handlers_builds_a_string_array() {
        let mac = render(Platform::MacOS, Arch::AArch64);
        assert!(mac.contains("bl __rt_array_new"));
        assert!(mac.contains("bl __rt_array_push_str"));
        let linux_x86 = render(Platform::Linux, Arch::X86_64);
        assert!(linux_x86.contains("call __rt_array_new"));
        assert!(linux_x86.contains("call __rt_array_push_str"));
    }
}
