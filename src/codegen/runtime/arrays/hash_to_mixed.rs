//! Purpose:
//! Emits the `__rt_hash_to_mixed` runtime helper for associative arrays that
//! widen entry payloads to boxed Mixed cells.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Conversion performs COW first, then transfers each existing entry payload
//!   into a Mixed box so by-reference foreach can alias a stable pointer slot.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits the `__rt_hash_to_mixed` runtime helper.
/// Converts all entry payloads of an associative array to boxed Mixed cells.
/// COW is enforced first via `__rt_hash_ensure_unique` so entries can be safely rewritten.
/// Each entry is stamped with value_type tag 7. The hash header is also stamped with 7.
/// Dispatches to `emit_hash_to_mixed_linux_x86_64` on x86_64; uses ARM64 otherwise.
pub fn emit_hash_to_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_to_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_to_mixed ---");
    emitter.label_global("__rt_hash_to_mixed");

    emitter.instruction("sub sp, sp, #96");                                     // reserve conversion frame slots and saved return state
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish a stable conversion frame
    emitter.instruction("bl __rt_hash_ensure_unique");                          // split shared hashes before rewriting entry payloads
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique hash pointer
    emitter.instruction("str xzr, [sp, #8]");                                   // initialize the insertion-order cursor

    emitter.label("__rt_hash_to_mixed_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the unique hash pointer for iteration
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch the next hash entry and its mutable value address
    emitter.instruction("cmn x0, #1");                                          // did iteration reach the end sentinel?
    emitter.instruction("b.eq __rt_hash_to_mixed_stamp");                       // stamp the hash header once every entry is converted
    emitter.instruction("str x0, [sp, #8]");                                    // save the next insertion-order cursor
    emitter.instruction("str x3, [sp, #16]");                                   // save the entry value low payload word
    emitter.instruction("str x4, [sp, #24]");                                   // save the entry value high payload word
    emitter.instruction("str x5, [sp, #32]");                                   // save the entry runtime value tag
    emitter.instruction("str x6, [sp, #40]");                                   // save the mutable entry value address
    emitter.instruction("cmp x5, #7");                                          // does this entry already hold a boxed Mixed cell?
    emitter.instruction("b.eq __rt_hash_to_mixed_entry_ready");                 // already-mixed entries only need metadata normalization
    emitter.instruction("mov x0, x5");                                          // pass the source runtime value tag to the owned-box helper
    emitter.instruction("mov x1, x3");                                          // pass the entry low payload word to the owned-box helper
    emitter.instruction("mov x2, x4");                                          // pass the entry high payload word to the owned-box helper
    emitter.instruction("bl __rt_hash_to_mixed_box_owned");                     // allocate a Mixed cell that takes over the entry payload
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload the mutable entry value address
    emitter.instruction("str x0, [x6]");                                        // store the boxed Mixed pointer in value_lo

    emitter.label("__rt_hash_to_mixed_entry_ready");
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload the mutable entry value address
    emitter.instruction("str xzr, [x6, #8]");                                   // normalize value_hi for boxed Mixed entries
    emitter.instruction("mov x9, #7");                                          // runtime value tag 7 = boxed Mixed
    emitter.instruction("str x9, [x6, #16]");                                   // stamp the entry payload as boxed Mixed
    emitter.instruction("b __rt_hash_to_mixed_loop");                           // continue converting insertion-order entries

    emitter.label("__rt_hash_to_mixed_stamp");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the unique hash pointer for return
    emitter.instruction("mov x9, #7");                                          // runtime value_type 7 = boxed Mixed
    emitter.instruction("str x9, [x0, #16]");                                   // stamp the hash header value_type as Mixed
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the conversion frame
    emitter.instruction("ret");                                                 // return the converted hash pointer

    emitter.label("__rt_hash_to_mixed_box_owned");
    emitter.instruction("sub sp, sp, #48");                                     // reserve a helper frame for tag and payload words
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save helper frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the runtime value tag
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the payload words that transfer into the Mixed box
    emitter.instruction("mov x0, #24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the boxed Mixed cell
    emitter.instruction("mov x9, #5");                                          // low byte 5 = boxed Mixed heap kind
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap allocation as a Mixed cell
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the saved runtime value tag
    emitter.instruction("str x10, [x0]");                                       // store the runtime value tag in the Mixed cell
    emitter.instruction("ldp x11, x12, [sp, #8]");                              // reload the payload words
    emitter.instruction("stp x11, x12, [x0, #8]");                              // store the payload words in the Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore helper frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the Mixed cell pointer
}

/// Generates the x86_64 Linux version of the `__rt_hash_to_mixed` runtime helper.
/// Converts each hash entry payload to a boxed Mixed cell via `__rt_hash_to_mixed_x86_box_owned`,
/// stamps the hash header with value_type 7, and returns the unique hash pointer.
/// Calling convention: rdi = hash pointer, rax = converted hash pointer.
fn emit_hash_to_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_to_mixed ---");
    emitter.label_global("__rt_hash_to_mixed");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before converting entries
    emitter.instruction("mov rbp, rsp");                                        // establish a stable conversion frame
    emitter.instruction("sub rsp, 64");                                         // reserve slots for hash pointer, cursor, payload, and entry address
    emitter.instruction("call __rt_hash_ensure_unique");                        // split shared hashes before rewriting entry payloads
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the unique hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // initialize the insertion-order cursor

    emitter.label("__rt_hash_to_mixed_x86_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the unique hash pointer for iteration
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next");                            // fetch the next hash entry and its mutable value address
    emitter.instruction("cmp rax, -1");                                         // did iteration reach the end sentinel?
    emitter.instruction("je __rt_hash_to_mixed_x86_stamp");                     // stamp the hash header once every entry is converted
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the next insertion-order cursor
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the entry value low payload word
    emitter.instruction("mov QWORD PTR [rbp - 32], r8");                        // save the entry value high payload word
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save the entry runtime value tag
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save the mutable entry value address
    emitter.instruction("cmp r9, 7");                                           // does this entry already hold a boxed Mixed cell?
    emitter.instruction("je __rt_hash_to_mixed_x86_entry_ready");               // already-mixed entries only need metadata normalization
    emitter.instruction("mov rax, r9");                                         // pass the source runtime value tag to the owned-box helper
    emitter.instruction("mov rdi, rcx");                                        // pass the entry low payload word to the owned-box helper
    emitter.instruction("mov rsi, r8");                                         // pass the entry high payload word to the owned-box helper
    emitter.instruction("call __rt_hash_to_mixed_x86_box_owned");               // allocate a Mixed cell that takes over the entry payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the mutable entry value address
    emitter.instruction("mov QWORD PTR [r10], rax");                            // store the boxed Mixed pointer in value_lo

    emitter.label("__rt_hash_to_mixed_x86_entry_ready");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the mutable entry value address
    emitter.instruction("mov QWORD PTR [r10 + 8], 0");                          // normalize value_hi for boxed Mixed entries
    emitter.instruction("mov QWORD PTR [r10 + 16], 7");                         // stamp the entry payload as boxed Mixed
    emitter.instruction("jmp __rt_hash_to_mixed_x86_loop");                     // continue converting insertion-order entries

    emitter.label("__rt_hash_to_mixed_x86_stamp");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the unique hash pointer for return
    emitter.instruction("mov QWORD PTR [rax + 16], 7");                         // stamp the hash header value_type as Mixed
    emitter.instruction("add rsp, 64");                                         // release the conversion frame slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the converted hash pointer

    emitter.label("__rt_hash_to_mixed_x86_box_owned");
    emitter.instruction("push rbp");                                            // preserve the conversion frame before allocating a Mixed box
    emitter.instruction("mov rbp, rsp");                                        // establish a helper frame for tag and payload words
    emitter.instruction("sub rsp, 32");                                         // reserve helper slots for tag, payload, and alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the runtime value tag
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the low payload word
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the high payload word
    emitter.instruction("mov rax, 24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the boxed Mixed cell
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 5)); // materialize the x86_64 Mixed heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap allocation as a Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the saved runtime value tag
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the runtime value tag in the Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the low payload word
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the low payload word in the Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the high payload word
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the high payload word in the Mixed cell
    emitter.instruction("add rsp, 32");                                         // release the helper frame slots
    emitter.instruction("pop rbp");                                             // restore the conversion frame pointer
    emitter.instruction("ret");                                                 // return the Mixed cell pointer
}
