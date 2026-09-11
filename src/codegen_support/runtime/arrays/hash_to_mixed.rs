//! Purpose:
//! Emits the `__rt_hash_to_mixed` runtime helper for associative arrays that
//! widen entry payloads to boxed Mixed cells.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Conversion performs COW first, then boxes each entry for a stable foreach reference slot.
//! - Ordinary payload owners transfer into their box; guarded destructor borrows acquire a new owner.
//! - Existing owned Mixed entries stay unchanged; guarded borrows clone their PHP value into a new box.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::emit_branch_if_null_container;


/// Emits the `__rt_hash_to_mixed` runtime helper.
/// Converts all entry payloads of an associative array to boxed Mixed cells.
/// COW is enforced first via `__rt_hash_ensure_unique` so entries can be safely rewritten.
/// Guarded payload borrows acquire new owners; existing borrowed Mixed cells are cloned.
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
    emitter.instruction("b.ne __rt_hash_to_mixed_claim");                       // raw entries always require a representation change
    emitter.instruction("stp x1, x2, [sp, #48]");                               // keep both key words across a read-only guard lookup
    emitter.instruction("ldr x0, [sp, #0]");                                    // inspect the selected hash without claiming an unchanged entry
    emitter.instruction("bl __rt_hash_write_guard_owns");                       // distinguish an existing owner from an active release borrow
    emitter.instruction("cbnz x0, __rt_hash_to_mixed_entry_ready");             // preserve an already owned box without invalidating its guard
    emitter.instruction("ldp x1, x2, [sp, #48]");                               // restore the borrowed entry's key before claiming its rewrite
    emitter.label("__rt_hash_to_mixed_claim");

    // -- claim the entry before transferring or acquiring its payload owner --
    emitter.instruction("ldr x0, [sp, #0]");                                    // supply the unique hash beside the iterator's key words
    emitter.instruction("bl __rt_hash_write_guard_claim");                      // identify a payload already borrowed by protected destruction
    emitter.instruction("mov x9, x0");                                          // preserve ownership while restoring the raw value
    emitter.instruction("ldr x0, [sp, #32]");                                   // restore the PHP value tag after key comparison
    emitter.instruction("ldp x1, x2, [sp, #16]");                               // restore both raw payload words
    emitter.instruction("cmp x0, #7");                                          // recognize a borrowed box requiring a detached PHP value
    emitter.instruction("b.eq __rt_hash_to_mixed_clone");                       // clone the dying box instead of retaining its zero-owner storage
    emitter.instruction("cbz x9, __rt_hash_to_mixed_borrowed");                 // acquire a fresh owner when the old release is active
    emitter.instruction("bl __rt_hash_to_mixed_box_owned");                     // transfer the hash's existing payload owner into its box
    emitter.instruction("b __rt_hash_to_mixed_box_ready");                      // publish the completed owned box
    emitter.label("__rt_hash_to_mixed_borrowed");
    emitter.instruction("bl __rt_mixed_from_value");                            // retain the borrowed payload before publishing a new box
    emitter.instruction("b __rt_hash_to_mixed_box_ready");                      // publish the new box after acquiring its child owner
    emitter.label("__rt_hash_to_mixed_clone");
    emitter.instruction("mov x0, x1");                                          // supply the borrowed Mixed cell to the value-copy helper
    emitter.instruction("bl __rt_mixed_clone");                                 // detach reference wrappers and retain the live PHP payload
    emitter.label("__rt_hash_to_mixed_box_ready");
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
    emitter.instruction("cmp x0, #4");                                          // only container-shaped tags can carry the null sentinel
    emitter.instruction("b.lt __rt_hash_to_mixed_box_owned_frame");             // preserve scalar payloads verbatim
    emitter.instruction("cmp x0, #6");                                          // indexed arrays, hashes, and objects occupy tags 4 through 6
    emitter.instruction("b.gt __rt_hash_to_mixed_box_owned_frame");             // nested Mixed and other tags use their ordinary payload
    emit_branch_if_null_container(
        emitter,
        "x1",
        "x9",
        "__rt_hash_to_mixed_box_owned_null",
    );
    emitter.instruction("b __rt_hash_to_mixed_box_owned_frame");                // box the valid transferred container payload
    emitter.label("__rt_hash_to_mixed_box_owned_null");
    emitter.instruction("mov x0, #8");                                          // normalize the transferred entry to canonical PHP null
    emitter.instruction("mov x1, #0");                                          // canonical null has no low payload word
    emitter.instruction("mov x2, #0");                                          // canonical null has no high payload word
    emitter.label("__rt_hash_to_mixed_box_owned_frame");
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
/// retaining guarded borrows or cloning their old boxes before publishing owned replacements.
/// Stamps the hash header with value_type 7 and returns the unique hash pointer.
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
    emitter.instruction("jne __rt_hash_to_mixed_x86_claim");                    // raw entries always require a representation change
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // preserve the iterator key across ownership inspection
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // preserve the exact key length across ownership inspection
    emitter.instruction("mov rsi, rdi");                                        // supply the iterator key beside its length in rdx
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // inspect the selected hash without claiming an unchanged entry
    emitter.instruction("call __rt_hash_write_guard_owns");                     // distinguish an existing owner from an active release borrow
    emitter.instruction("test rax, rax");                                       // decide whether an existing Mixed entry needs a replacement
    emitter.instruction("jnz __rt_hash_to_mixed_x86_entry_ready");              // preserve an already owned box without invalidating its guard
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // restore the borrowed entry key before claiming its rewrite
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // restore the exact binary or integer key discriminator
    emitter.label("__rt_hash_to_mixed_x86_claim");

    // -- claim the entry before transferring or acquiring its payload owner --
    emitter.instruction("mov rsi, rdi");                                        // supply the iterator's key beside its length in rdx
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // identify the unique hash being converted
    emitter.instruction("call __rt_hash_write_guard_claim");                    // identify a payload already borrowed by protected destruction
    emitter.instruction("mov r11, rax");                                        // preserve ownership while restoring the raw value
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // restore the PHP value tag after key comparison
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // restore the low payload word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // restore the high payload word
    emitter.instruction("cmp rax, 7");                                          // recognize a borrowed box requiring a detached PHP value
    emitter.instruction("je __rt_hash_to_mixed_x86_clone");                     // clone the dying box instead of retaining its zero-owner storage
    emitter.instruction("test r11, r11");                                       // select transfer or acquisition of the payload owner
    emitter.instruction("jz __rt_hash_to_mixed_x86_borrowed");                  // acquire a fresh owner when the old release is active
    emitter.instruction("call __rt_hash_to_mixed_x86_box_owned");               // transfer the hash's existing payload owner into its box
    emitter.instruction("jmp __rt_hash_to_mixed_x86_box_ready");                // publish the completed owned box
    emitter.label("__rt_hash_to_mixed_x86_borrowed");
    emitter.instruction("call __rt_mixed_from_value");                          // retain the borrowed payload before publishing a new box
    emitter.instruction("jmp __rt_hash_to_mixed_x86_box_ready");                // publish the new box after acquiring its child owner
    emitter.label("__rt_hash_to_mixed_x86_clone");
    emitter.instruction("mov rax, rdi");                                        // supply the borrowed Mixed cell to the value-copy helper
    emitter.instruction("call __rt_mixed_clone");                               // detach reference wrappers and retain the live PHP payload
    emitter.label("__rt_hash_to_mixed_x86_box_ready");
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
    emitter.instruction("cmp rax, 4");                                          // only container-shaped tags can carry the null sentinel
    emitter.instruction("jl __rt_hash_to_mixed_x86_box_owned_frame");           // preserve scalar payloads verbatim
    emitter.instruction("cmp rax, 6");                                          // indexed arrays, hashes, and objects occupy tags 4 through 6
    emitter.instruction("jg __rt_hash_to_mixed_x86_box_owned_frame");           // nested Mixed and other tags use their ordinary payload
    emit_branch_if_null_container(
        emitter,
        "rdi",
        "r10",
        "__rt_hash_to_mixed_x86_box_owned_null",
    );
    emitter.instruction("jmp __rt_hash_to_mixed_x86_box_owned_frame");          // box the valid transferred container payload
    emitter.label("__rt_hash_to_mixed_x86_box_owned_null");
    emitter.instruction("mov rax, 8");                                          // normalize the transferred entry to canonical PHP null
    emitter.instruction("xor edi, edi");                                        // canonical null has no low payload word
    emitter.instruction("xor esi, esi");                                        // canonical null has no high payload word
    emitter.label("__rt_hash_to_mixed_x86_box_owned_frame");
    emitter.instruction("push rbp");                                            // preserve the conversion frame before allocating a Mixed box
    emitter.instruction("mov rbp, rsp");                                        // establish a helper frame for tag and payload words
    emitter.instruction("sub rsp, 32");                                         // reserve helper slots for tag, payload, and alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the runtime value tag
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the low payload word
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the high payload word
    emitter.instruction("mov rax, 24");                                         // Mixed cells store tag plus two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the boxed Mixed cell
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the x86_64 Mixed heap kind word
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
